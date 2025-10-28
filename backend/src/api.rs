use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use url::Url;
use crate::{controller::{SiteState, SITE_CONTROLLERS}, database::DATABASE, server::ConnectionMetadata};
use log::*;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    #[serde(flatten)]
    pub metadata: ConnectionMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct StateHistoryEntry {
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
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
    pub uptime_percentage: f64,
    pub total_hibernations: usize,
    pub start_times_histogram: Vec<u64>, // Buckets of start times in milliseconds
    pub start_duration_estimate_ms: Option<u64>, // From get_start_duration_estimate
}

pub async fn handle_services_request(mut stream: TcpStream) {
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

    let content = serde_json::to_string(&services).unwrap(); // FIXME

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}

pub async fn handle_service_config_request(mut stream: TcpStream, service_name: &str) {
    trace!("Handling service config request for: {}", service_name);

    // SAFETY: This is safe because SITE_CONTROLLERS is only mutated once during initialization
    #[allow(static_mut_refs)]
    let controller = unsafe {
        SITE_CONTROLLERS.iter().find(|controller| controller.config.name == service_name)
    };

    let controller = match controller {
        Some(controller) => controller,
        None => {
            let status_line = "HTTP/1.1 404 Not Found";
            let content = format!("Service '{}' not found", service_name);
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
            return;
        }
    };

    let content = serde_json::to_string(&controller.config).unwrap(); // FIXME

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}

pub async fn handle_history_request(mut stream: TcpStream, url: &Url) {
    trace!("Handling history request: {}", url);

    let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service = query_pairs.get("service").map(|s| s.as_str());
    let before = query_pairs.get("before").and_then(|b| b.parse::<u64>().ok());
    let after = query_pairs.get("after").and_then(|a| a.parse::<u64>().ok());
    let min_results = query_pairs.get("minResults").and_then(|m| m.parse::<usize>().ok()).unwrap_or(10);

    let history = DATABASE.get_connection_history(
        service,
        before.or(Some(u64::MAX)),
        after,
        min_results
    ).unwrap(); // FIXME

    let entries = history.into_iter().map(|(timestamp, metadata)| HistoryEntry {
        timestamp,
        metadata,
    }).collect::<Vec<_>>();

    let content = serde_json::to_string(&entries).unwrap(); // FIXME

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}

pub async fn handle_state_history_request(mut stream: TcpStream, url: &Url) {
    trace!("Handling state history request: {}", url);

    let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service = query_pairs.get("service").map(|s| s.as_str());
    let before = query_pairs.get("before").and_then(|b| b.parse::<i64>().ok()).map(|ts| DateTime::from_timestamp(ts, 0).unwrap());
    let after = query_pairs.get("after").and_then(|a| a.parse::<i64>().ok()).map(|ts| DateTime::from_timestamp(ts, 0).unwrap());
    let min_results = query_pairs.get("minResults").and_then(|m| m.parse::<usize>().ok()).unwrap_or(10);

    let history = DATABASE.get_state_history(
        service,
        before.or(Some(DateTime::from_timestamp(i64::MAX / 1_000_000_000, 0).unwrap())),
        after,
        min_results
    ).unwrap(); // FIXME

    let entries = history.into_iter().map(|(timestamp, service, state)| {
        let state_str = match state {
            SiteState::Unknown => "unknown",
            SiteState::Down => "down",
            SiteState::Up => "up",
            SiteState::Starting => "starting",
        };
        StateHistoryEntry {
            timestamp,
            service,
            state: state_str.to_string(),
        }
    }).collect::<Vec<_>>();

    let content = serde_json::to_string(&entries).unwrap(); // FIXME

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}

pub async fn handle_metrics_request(mut stream: TcpStream, service_name: &str, url: &Url) {
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
            let status_line = "HTTP/1.1 404 Not Found";
            let content = format!("Service '{}' not found", service_name);
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
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
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = format!("Error fetching metrics: {}", e);
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
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
    let mut total_period_ms = 0;
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
                total_period_ms += duration_ms;
            },
            (SiteState::Down | SiteState::Starting, SiteState::Up) => {
                // Went up
                total_period_ms += duration_ms;

                if state1 == &SiteState::Starting {
                            println!("Duration between {} and {} is {} ms", timestamp1, timestamp2, duration_ms);

                    // Record start duration
                    start_durations_ms.push(duration_ms);
                }
            },
            (SiteState::Up, SiteState::Down | SiteState::Starting) => {
                // Went down
                total_period_ms += duration_ms;
                total_uptime_ms += duration_ms;
                total_hibernations += 1;
            }
            (SiteState::Up, SiteState::Up) => {
                // Stayed up
                total_period_ms += duration_ms;
                total_uptime_ms += duration_ms;
            }
        }
    }

    let uptime_percentage = if total_period_ms > 0 {
        (total_uptime_ms as f64 / total_period_ms as f64) * 100.0
    } else {
        0.0
    };

    // Create histogram with buckets (0-1s, 1-5s, 5-10s, 10-30s, 30s+)
    let histogram = vec![
        start_durations_ms.iter().filter(|&&d| d < 1000).count() as u64,
        start_durations_ms.iter().filter(|&&d| d >= 1000 && d < 5000).count() as u64,
        start_durations_ms.iter().filter(|&&d| d >= 5000 && d < 10000).count() as u64,
        start_durations_ms.iter().filter(|&&d| d >= 10000 && d < 30000).count() as u64,
        start_durations_ms.iter().filter(|&&d| d >= 30000).count() as u64,
    ];

    // Get start duration estimate from database
    let start_duration_estimate_ms = DATABASE
        .get_start_duration_estimate(service_name, controller.config.eta_percentile.0)
        .ok()
        .map(|d| d.as_millis() as u64);

    let metrics = ServiceMetrics {
        uptime_percentage,
        total_hibernations,
        start_times_histogram: histogram,
        start_duration_estimate_ms,
    };

    let content = serde_json::to_string(&metrics).unwrap(); // FIXME

    let status_line = "HTTP/1.1 200 OK";
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}
