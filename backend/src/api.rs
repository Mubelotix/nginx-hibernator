use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use url::Url;
use crate::{controller::{SiteState, SITE_CONTROLLERS}, database::DATABASE, server::ConnectionMetadata};
use log::*;

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
