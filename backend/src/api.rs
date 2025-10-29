use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use url::Url;
use crate::{controller::{SiteState, SITE_CONTROLLERS}, database::DATABASE, server::ConnectionMetadata};
use log::*;
use std::collections::HashMap;

/// Helper function to send a JSON response
async fn send_json_response(mut stream: TcpStream, data: &impl Serialize) -> Result<(), ()> {
    let content = match serde_json::to_string(data) {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to serialize response: {}", e);
            send_error_response(stream, 500, "Failed to serialize response").await;
            return Err(());
        }
    };

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
    Ok(())
}

/// Helper function to send an error response
async fn send_error_response(mut stream: TcpStream, status_code: u16, message: &str) {
    let status_line = match status_code {
        404 => "HTTP/1.1 404 Not Found",
        500 => "HTTP/1.1 500 Internal Server Error",
        _ => "HTTP/1.1 500 Internal Server Error",
    };
    let length = message.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{message}");
    let _ = stream.write_all(response.as_bytes()).await;
}

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    #[serde(flatten)]
    pub metadata: ConnectionMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct StateHistoryEntry {
    #[serde(with = "chrono::serde::ts_seconds")]
    pub start_time: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub end_time: DateTime<Utc>,
    pub service: String,
    pub state: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub state: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_changed: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub hibernating_percentage: f64,
    pub available_percentage: f64,
    pub total_hibernations: usize,
    pub start_times_histogram: Vec<u64>, // Buckets of start times in milliseconds
    pub start_duration_estimate_ms: Option<u64>, // From get_start_duration_estimate
}

pub async fn handle_services_request(stream: TcpStream) {
    // SAFETY: This is safe because SITE_CONTROLLERS is only mutated once during initialization
    #[allow(static_mut_refs)]
    let services: Vec<ServiceInfo> = unsafe {
        SITE_CONTROLLERS.iter().map(|controller| {
            let (state, last_changed) = controller.get_state_with_last_changed();
            let state_str = match state {
                crate::controller::SiteState::Unknown => "unknown",
                crate::controller::SiteState::Down => "down",
                crate::controller::SiteState::Up => "up",
                crate::controller::SiteState::Starting => "starting",
            };
            ServiceInfo {
                name: controller.config.name.to_string(),
                state: state_str.to_string(),
                last_changed
            }
        }).collect()
    };

    let _ = send_json_response(stream, &services).await;
}

pub async fn handle_service_config_request(stream: TcpStream, service_name: &str) {
    trace!("Handling service config request for: {}", service_name);

    // SAFETY: This is safe because SITE_CONTROLLERS is only mutated once during initialization
    #[allow(static_mut_refs)]
    let controller = unsafe {
        SITE_CONTROLLERS.iter().find(|controller| controller.config.name == service_name)
    };

    let controller = match controller {
        Some(controller) => controller,
        None => {
            send_error_response(stream, 404, &format!("Service '{}' not found", service_name)).await;
            return;
        }
    };

    let _ = send_json_response(stream, &controller.config).await;
}

pub async fn handle_history_request(stream: TcpStream, url: &Url) {
    trace!("Handling history request: {}", url);

    let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service = query_pairs.get("service").map(|s| s.as_str());
    let before = query_pairs.get("before").and_then(|b| b.parse::<u64>().ok());
    let after = query_pairs.get("after").and_then(|a| a.parse::<u64>().ok());
    let min_results = query_pairs.get("minResults").and_then(|m| m.parse::<usize>().ok()).unwrap_or(10);

    let history = match DATABASE.get_connection_history(
        service,
        before.or(Some(u64::MAX)),
        after,
        min_results
    ) {
        Ok(history) => history,
        Err(e) => {
            error!("Failed to get connection history: {}", e);
            send_error_response(stream, 500, &format!("Failed to get connection history: {}", e)).await;
            return;
        }
    };

    let entries = history.into_iter().map(|(timestamp, metadata)| HistoryEntry {
        timestamp,
        metadata,
    }).collect::<Vec<_>>();

    let _ = send_json_response(stream, &entries).await;
}

pub async fn handle_state_history_request(stream: TcpStream, url: &Url) {
    trace!("Handling state history request: {}", url);

    let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service = query_pairs.get("service").map(|s| s.as_str());
    let before = query_pairs.get("before").and_then(|b| b.parse::<i64>().ok()).and_then(|ts| DateTime::from_timestamp(ts, 0));
    let after = query_pairs.get("after").and_then(|a| a.parse::<i64>().ok()).and_then(|ts| DateTime::from_timestamp(ts, 0));
    let min_results = query_pairs.get("minResults").and_then(|m| m.parse::<usize>().ok()).unwrap_or(10);

    let mut all_ranges = Vec::new();

    if let Some(svc) = service {
        // Query specific service
        let ranges = match DATABASE.get_state_history(
            svc,
            before.or_else(|| DateTime::from_timestamp(i64::MAX / 1_000_000_000, 0)),
            after,
            min_results
        ) {
            Ok(ranges) => ranges,
            Err(e) => {
                error!("Failed to get state history: {}", e);
                send_error_response(stream, 500, &format!("Failed to get state history: {}", e)).await;
                return;
            }
        };
        
        all_ranges = ranges;
    } else {
        // Query all services and collect results
        // SAFETY: This is safe because SITE_CONTROLLERS is only mutated once during initialization
        #[allow(static_mut_refs)]
        let services: Vec<&str> = unsafe {
            SITE_CONTROLLERS.iter().map(|controller| controller.config.name.as_str()).collect()
        };

        for svc in services {
            let ranges = match DATABASE.get_state_history(
                svc,
                before.or_else(|| DateTime::from_timestamp(i64::MAX / 1_000_000_000, 0)),
                after,
                min_results
            ) {
                Ok(ranges) => ranges,
                Err(e) => {
                    error!("Failed to get state history for service {}: {}", svc, e);
                    Vec::new()
                }
            };
            
            all_ranges.extend(ranges);
        }

        // Sort by start_time (newest first since we're querying backwards)
        all_ranges.sort_by(|a, b| b.0.cmp(&a.0));
        
        // Limit to min_results
        all_ranges.truncate(min_results);
    }

    // Convert to API format
    let entries: Vec<StateHistoryEntry> = all_ranges.into_iter().map(|(start_time, end_time, state)| {
        let state_str = match state {
            SiteState::Unknown => "unknown",
            SiteState::Down => "down",
            SiteState::Up => "up",
            SiteState::Starting => "starting",
        };
        StateHistoryEntry {
            start_time,
            end_time,
            service: service.unwrap_or("unknown").to_string(),
            state: state_str.to_string(),
        }
    }).collect();

    let _ = send_json_response(stream, &entries).await;
}

pub async fn handle_metrics_request(stream: TcpStream, service_name: &str, url: &Url) {
    trace!("Handling metrics request for: {}", service_name);

    // Parse the 'seconds' query parameter (default to 86400 = 24 hours)
    let query_pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();
    let seconds = query_pairs
        .get("seconds")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(86400);

    // SAFETY: This is safe because SITE_CONTROLLERS is only mutated once during initialization
    #[allow(static_mut_refs)]
    let controller = unsafe {
        SITE_CONTROLLERS.iter().find(|controller| controller.config.name == service_name)
    };

    let controller = match controller {
        Some(controller) => controller,
        None => {
            send_error_response(stream, 404, &format!("Service '{}' not found", service_name)).await;
            return;
        }
    };

    let now = Utc::now();
    let since = now - Duration::seconds(seconds);

    // Get state history for the time period
    let mut state_history = match DATABASE.get_state_history_since(service_name, since) {
        Ok(history) => history,
        Err(e) => {
            error!("Error fetching state history: {}", e);
            send_error_response(stream, 500, &format!("Error fetching metrics: {}", e)).await;
            return;
        }
    };

    // Duplicate the last element with current time
    if let Some((last_timestamp, last_state)) = state_history.last().cloned() {
        if last_timestamp < now {
            state_history.push((now, last_state));
        }
    }

    // Calculate uptime percentage and hibernations
    let mut total_uptime_ms = 0;
    let mut total_available_ms = 0;
    let mut total_hibernations = 0;
    let mut start_durations_ms = Vec::new();

    for i in 0..(state_history.len() - 1) {
        let (timestamp1, state1) = &state_history[i];
        let (timestamp2, state2) = &state_history[i + 1];
        let duration_ms = (timestamp2.timestamp_millis() - timestamp1.timestamp_millis()) as u64;

        match (state1, state2) {
            (SiteState::Unknown, _) | (_, SiteState::Unknown) => (),
            (SiteState::Down | SiteState::Starting, SiteState::Down | SiteState::Starting) => {
                // Stayed down
                total_available_ms += duration_ms;
            },
            (SiteState::Down | SiteState::Starting, SiteState::Up) => {
                // Went up
                total_available_ms += duration_ms;

                if state1 == &SiteState::Starting {
                    // Record start duration
                    start_durations_ms.push(duration_ms);
                }
            },
            (SiteState::Up, SiteState::Down | SiteState::Starting) => {
                // Went down
                total_available_ms += duration_ms;
                total_uptime_ms += duration_ms;
                total_hibernations += 1;
            }
            (SiteState::Up, SiteState::Up) => {
                // Stayed up
                total_available_ms += duration_ms;
                total_uptime_ms += duration_ms;
            }
        }
    }

    let hibernating_percentage = if total_available_ms > 0 {
        ((total_available_ms - total_uptime_ms) as f64 / total_available_ms as f64) * 100.0
    } else {
        0.0
    };

    let available_percentage = if seconds > 0 {
        (total_available_ms as f64 / (seconds as f64 * 1000.0)) * 100.0
    } else {
        0.0
    };

    // Create histogram with buckets (0-1s, 1-5s, 5-10s, 10-30s, 30s+)
    let histogram = vec![
        start_durations_ms.iter().filter(|&&d| d < 1000).count() as u64,
        start_durations_ms.iter().filter(|&&d| (1000..5000).contains(&d)).count() as u64,
        start_durations_ms.iter().filter(|&&d| (5000..10000).contains(&d)).count() as u64,
        start_durations_ms.iter().filter(|&&d| (10000..30000).contains(&d)).count() as u64,
        start_durations_ms.iter().filter(|&&d| d >= 30000).count() as u64,
    ];

    // Get start duration estimate from database
    let start_duration_estimate_ms = DATABASE
        .get_start_duration_estimate(service_name, controller.config.eta_percentile.0)
        .ok()
        .map(|d| d.as_millis() as u64);

    let metrics = ServiceMetrics {
        hibernating_percentage,
        available_percentage,
        total_hibernations,
        start_times_histogram: histogram,
        start_duration_estimate_ms,
    };

    let _ = send_json_response(stream, &metrics).await;
}
