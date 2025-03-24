use std::time::Duration;
use crate::{get_controller, is_healthy, Config, ProxyMode, SiteConfig};
use log::*;
use anyhow::anyhow;
use tokio::{io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::{TcpListener, TcpStream}, spawn, time::{sleep, timeout}};
use tokio_stream::{wrappers::LinesStream, StreamExt};

pub async fn setup_server(config: &'static Config) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.top_level.hibernator_port())).await.expect("Could not bind to port");

    spawn(async move {
        loop {
            if let Ok((stream, _addr)) = listener.accept().await {
                spawn(handle_connection(stream));
            }
        }
    });
}

fn should_be_processed(site_config: &'static SiteConfig, path: &str, real_ip: Option<&str>) -> bool {
    if let Some(blacklist_paths) = &site_config.path_blacklist {
        for blacklist_path in blacklist_paths {
            if blacklist_path.is_match(path) {
                return false;
            }
        }
    }

    if let Some(blacklist_ips) = &site_config.ip_blacklist {
        let real_ip = real_ip.unwrap();
        for blacklist_ip in blacklist_ips {
            if real_ip.starts_with(blacklist_ip) {
                return false;
            }
        }
    }

    if let Some(whitelist_ips) = &site_config.ip_whitelist {
        let real_ip = real_ip.unwrap();
        for whitelist_ip in whitelist_ips {
            if real_ip.starts_with(whitelist_ip) {
                return true;
            }
        }
        return false;
    }

    true
}

async fn try_proxy(port: u16, head: Vec<String>, body: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let mut upstream = TcpStream::connect(format!("127.0.0.1:{port}")).await?;

    upstream.write_all(head.join("\r\n").as_bytes()).await?;
    upstream.write_all(b"\r\n\r\n").await?;
    upstream.write_all(&body).await?;

    let mut response = Vec::new();
    upstream.read_to_end(&mut response).await?;

    if response.is_empty() {
        return Err(anyhow!("Empty response"));
    }

    Ok(response)
}

// It's ok to panic in this function, as it's only called in its own thread
async fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = LinesStream::new(buf_reader.lines())
        .map(|result| result.expect("Could not read request lines"))
        .take_while(|line| !line.is_empty())
        .collect()
        .await;
    debug!("Request: {http_request:?}");

    let host = http_request
        .iter()
        .find(|line| line.to_lowercase().starts_with("host: "))
        .map(|line| &line[6..])
        .map(|host| host.to_lowercase());

    let host = match host {
        Some(host) => host,
        None => {
            debug!("Client didn't provide a Host header");
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = "Hibernator requires a Host header";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
            return;
        }
    };

    let controller = get_controller(&host);
    let controller = match controller {
        Some(controller) => controller,
        None => {
            debug!("Client requested a site that doesn't exist (host: {host})");
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = format!("Hibernator doesn't know about the site you're trying to access (host: {host})");
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
            return;
        }
    };

    // Make sure the request should be treated
    let first_line = http_request.first().expect("Request is empty");
    let path = first_line.split_whitespace().nth(1).expect("Request line is empty");
    let real_ip = http_request
        .iter()
        .find(|line| line.to_lowercase().starts_with("x-real-ip: "))
        .map(|line| &line[11..]);
    if !should_be_processed(controller.config, path, real_ip) {
        debug!("Client shall not be served");
        let status_line = "HTTP/1.1 503 Service Unavailable";
        let content = "Server is unavailable";
        let length = content.len();
        let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }

    // Determine if we should attempt to proxy the request
    let is_browser = http_request.iter().any(|line| line.to_lowercase() == "sec-fetch-mode: navigate");
    let proxy_mode = match is_browser {
        true => &controller.config.browser_proxy_mode,
        false => &controller.config.proxy_mode,
    };
    let should_proxy = match proxy_mode {
        ProxyMode::Always => true,
        ProxyMode::WhenReady => is_healthy(controller.config.port).await,
        ProxyMode::Never => false,
    };
    debug!("Is browser: {is_browser}, Proxy mode: {proxy_mode:?}, Should proxy: {should_proxy}");

    if !should_proxy {
        debug!("Returning 503 right away");
        let status_line = "HTTP/1.1 503 Service Unavailable";
        let content = include_str!("../static/index.html").replace("KEEP_ALIVE", &controller.config.keep_alive.to_string());
        let length = content.len();
        let response = format!(
            "{status_line}\r\nContent-Length: {length}\r\n\r\n{content}"
        );
        let _ = stream.write_all(response.as_bytes()).await;

        controller.trigger_start().await;
        return;
    }

    let content_lenght = http_request
        .iter()
        .find(|line| line.to_lowercase().starts_with("content-length: "))
        .map(|line| line[16..].parse::<usize>().expect("Could not parse content length"))
        .unwrap_or(0);
    let mut body = vec![0; content_lenght];
    stream.read_exact(&mut body).await.expect("Could not read request body");

    let timeout_duration = Duration::from_millis(controller.config.proxy_timeout_ms.0);
    let r = timeout(timeout_duration, async move {
        controller.waiting_trigger_start().await;
        debug!("Site started, waiting for upstream");
        loop {
            if let Ok(response) = try_proxy(controller.config.port, http_request.clone(), body.clone()).await {
                debug!("Site {} is ready, got response", controller.config.name);
                return Ok::<Vec<u8>, anyhow::Error>(response);
            }
            sleep(Duration::from_millis(controller.config.proxy_check_interval_ms.0)).await;
        }
    }).await;

    match r {
        Ok(Ok(response)) => {
            debug!("Returning response from upstream");
            let _ = stream.write_all(&response).await;
        },
        Ok(Err(e)) => {
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = format!("Error while starting site: {e}");
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
        },
        Err(_) => {
            debug!("Site {} took too long to start", controller.config.name);

            let status_line = "HTTP/1.1 504 Gateway Timeout";
            let content = "Site is booting up. Try again.";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes()).await;
        },
    }
}
