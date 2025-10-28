use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use url::Url;
use crate::{controller::SITE_CONTROLLERS, database::DATABASE, server::ConnectionMetadata};
use log::*;

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    #[serde(flatten)]
    pub metadata: ConnectionMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub state: String,
    pub last_changed: u64,
}

pub async fn handle_services_request(mut stream: TcpStream) {
    trace!("Handling services request");

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
                last_changed,
            }
        }).collect()
    };

    let content = serde_json::to_string(&services).unwrap(); // FIXME

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

    let history = if let Some(after) = after {
        DATABASE.get_history_after(service, after, min_results).unwrap() // FIXME
    } else {
        let before = before.unwrap_or(u64::MAX);
        DATABASE.get_history(service, before, min_results).unwrap() // FIXME
    };

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
