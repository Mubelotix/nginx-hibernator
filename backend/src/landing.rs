use std::{path::Path, time::Duration};
use log::*;
use tokio::{fs, io::AsyncWriteExt, net::TcpStream};

/// Serves the landing page (index.html) with replaced template variables
pub async fn serve_landing_page(
    mut stream: TcpStream,
    landing_folder: &str,
    done: Duration,
    duration: Duration,
    keep_alive: u64,
) -> bool {
    let index_path = Path::new(landing_folder).join("index.html");

    // Read the index.html file
    let content = match fs::read_to_string(&index_path).await {
        Ok(content) => content,
        Err(e) => {
            warn!("Could not read index.html from {:?}: {e}", index_path);
            send_error(&mut stream, 500, "Landing page not found").await;
            return false;
        }
    };

    // Replace template variables
    let content = content
        .replace("DONE_MS", &done.as_millis().to_string())
        .replace("DURATION_MS", &duration.as_millis().to_string())
        .replace("KEEP_ALIVE", &keep_alive.to_string());

    // Send response
    let status_line = "HTTP/1.1 503 Service Unavailable";
    let retry_after = duration.checked_sub(done)
        .and_then(|remaining| {
            let remaining_secs = remaining.as_secs();
            if remaining_secs > 0 {
                Some(format!("Retry-After: {remaining_secs}\r\n"))
            } else {
                None
            }
        })
        .unwrap_or_default();
    
    let length = content.len();
    let response = format!(
        "{status_line}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {length}\r\n{retry_after}\r\n{content}"
    );

    if let Err(e) = stream.write_all(response.as_bytes()).await {
        warn!("Could not write landing page response: {e}");
        return false;
    }

    true
}

async fn send_error(stream: &mut TcpStream, code: u16, message: &str) {
    let status_line = format!("HTTP/1.1 {code} {message}");
    let content = message;
    let length = content.len();
    let response = format!("{status_line}\r\nContent-Type: text/plain\r\nContent-Length: {length}\r\n\r\n{content}");
    let _ = stream.write_all(response.as_bytes()).await;
}
