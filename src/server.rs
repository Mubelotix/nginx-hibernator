use std::{io::{BufRead, BufReader, Read, Write}, net::{TcpListener, TcpStream}, sync::mpsc::channel, thread::{sleep, spawn}, time::{Duration, Instant}};
use crate::{is_port_open, start_server, Config, ProxyMode, SiteConfig};
use log::*;

pub fn setup_server(config: &'static Config) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.top_level.hibernator_port())).expect("Could not bind to port");

    spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {continue};
            spawn(move || handle_connection(stream, config));
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

// It's ok to panic in this function, as it's only called in its own thread
fn handle_connection(mut stream: TcpStream, config: &'static Config) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.expect("Could not read request lines"))
        .take_while(|line| !line.is_empty())
        .collect();
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
            let _ = stream.write_all(response.as_bytes());
            return;
        }
    };

    let site_config = config.sites.iter().find(|site| site.hosts.contains(&host));
    let site_config = match site_config {
        Some(site_config) => site_config,
        None => {
            debug!("Client requested a site that doesn't exist");
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = "Hibernator doesn't know about the site you're trying to access";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes());
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
    if !should_be_processed(site_config, path, real_ip) {
        debug!("Client shall not be served");
        let status_line = "HTTP/1.1 503 Service Unavailable";
        let content = "Server is unavailable";
        let length = content.len();
        let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    // Determine if we should attempt to proxy the request
    let is_browser = http_request.iter().any(|line| line.to_lowercase() == "sec-fetch-mode: navigate");
    let is_up = is_port_open(site_config.port);
    let proxy_mode = match is_browser {
        true => &site_config.browser_proxy_mode,
        false => &site_config.proxy_mode,
    };
    let should_proxy = match proxy_mode {
        ProxyMode::Always => true,
        ProxyMode::WhenReady => is_up,
        ProxyMode::Never => false,
    };
    debug!("Is browser: {is_browser}, Is up: {is_up}, Should proxy: {should_proxy}");

    if !should_proxy {
        debug!("Returning 503 right away");
        let status_line = "HTTP/1.1 503 Service Unavailable";
        let content = include_str!("../static/index.html").replace("KEEP_ALIVE", &site_config.keep_alive.to_string());
        let length = content.len();
        let response = format!(
            "{status_line}\r\nContent-Length: {length}\r\n\r\n{content}"
        );
        let _ = stream.write_all(response.as_bytes());

        let r = start_server(site_config, is_up);
        if let Err(e) = &r {
            eprintln!("Error while starting site {}: {e}", site_config.name);
        }

        return;
    }

    let content_lenght = http_request
        .iter()
        .find(|line| line.to_lowercase().starts_with("content-length: "))
        .map(|line| line[16..].parse::<usize>().expect("Could not parse content length"))
        .unwrap_or(0);
    let mut body = vec![0; content_lenght];
    stream.read_exact(&mut body).expect("Could not read request body");

    let (sender, receiver) = channel::<anyhow::Result<()>>();
    let start_instant = Instant::now();
    let timeout_duration = Duration::from_millis(site_config.proxy_timeout_ms.0);
    spawn(move || {
        let r = start_server(site_config, is_up);
        if let Err(e) = &r {
            eprintln!("Error while starting site {}: {e}", site_config.name);
            let _ = sender.send(r);
            return;
        }
        debug!("Site started, waiting for port to open");
        loop {
            if start_instant.elapsed() >= timeout_duration {
                warn!("Site {} took too long to start", site_config.name);
                break;
            }
            if is_port_open(site_config.port) {
                debug!("Site {} is ready", site_config.name);
                let _ = sender.send(Ok(()));
                break;
            }
            sleep(Duration::from_millis(site_config.proxy_check_interval_ms.0));
        }
    });

    let r = receiver.recv_timeout(timeout_duration);
    match r {
        Ok(Ok(())) => {
            debug!("Connecting to target site {}", site_config.name);
            let mut upstream = match TcpStream::connect(format!("127.0.0.1:{}", site_config.port)) {
                Ok(stream) => stream,
                Err(_) => {
                    warn!("Error while connecting to target site {}", site_config.name);

                    let status_line = "HTTP/1.1 502 Bad Gateway";
                    let content = "Error while connecting to site";
                    let length = content.len();
                    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
                    let _ = stream.write_all(response.as_bytes());
                    return;
                }
            };

            debug!("Proxying request to site {}", site_config.name);
            upstream.write_all(http_request.join("\r\n").as_bytes()).expect("Could not write request to inner stream");
            upstream.write_all(b"\r\n\r\n").expect("Could not write request end to inner stream");
            upstream.write_all(&body).expect("Could not write request body to inner stream");

            std::io::copy(&mut upstream, &mut stream).expect("Could not forward from downstream to upstream");

            debug!("Request to site {} completed", site_config.name);
        },
        Ok(Err(e)) => {
            let status_line = "HTTP/1.1 500 Internal Server Error";
            let content = "Error while starting site";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes());
        },
        Err(_) => {
            debug!("Site {} took too long to start", site_config.name);

            let status_line = "HTTP/1.1 504 Gateway Timeout";
            let content = "Site is booting up. Try again.";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes());
        },
    }
}
