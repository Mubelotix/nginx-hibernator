use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use url::Url;
use crate::{database::DATABASE, server::ConnectionMetadata};
use log::*;

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    #[serde(flatten)]
    pub metadata: ConnectionMetadata,
}

pub async fn handle_history_request(mut stream: TcpStream, url: &Url) {
    trace!("Handling history request: {}", url);

    let query_pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service = query_pairs.get("service").map(|s| s.as_str());
    let before = query_pairs.get("before").and_then(|b| b.parse::<u64>().ok()).unwrap_or(u64::MAX);
    let min_results = query_pairs.get("minResults").and_then(|m| m.parse::<usize>().ok()).unwrap_or(10);

    let history = DATABASE.get_history(service, before, min_results).unwrap(); // FIXME

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
