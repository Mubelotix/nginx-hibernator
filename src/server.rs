use std::{io::{BufRead, BufReader, Read, Write}, net::{TcpListener, TcpStream}, sync::mpsc::channel, thread::{sleep, spawn}, time::{Duration, Instant}};
use crate::{is_port_open, start_server, Config, ProxyMode};
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

// It's ok to panic in this function, as it's only called in its own thread
fn handle_connection(mut stream: TcpStream, config: &'static Config) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.expect("Could not read request lines"))
        .take_while(|line| !line.is_empty())
        .collect();

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

    // Fail rightaway if the client is a browser, so that it quickly displays a waiting page to the user
    let is_browser_navigate = http_request.iter().any(|line| line.starts_with("Sec-Fetch-Mode: navigate"));
    let should_proxy = site_config.proxy_mode == ProxyMode::None || (site_config.proxy_mode == ProxyMode::NonBrowser && is_browser_navigate);
    if !should_proxy {
        debug!("Returning 503 right away");
        let status_line = "HTTP/1.1 503 Service Unavailable";
        let content = include_str!("../static/index.html").replace("KEEP_ALIVE", &site_config.keep_alive.to_string());
        let length = content.len();
        let response = format!(
            "{status_line}\r\nContent-Length: {length}\r\n\r\n{content}"
        );
        let _ = stream.write_all(response.as_bytes());

        let r = start_server(site_config);
        if let Err(e) = &r {
            eprintln!("Error while starting site {}: {e}", site_config.name);
        }

        return;
    }

    let (sender, receiver) = channel::<anyhow::Result<()>>();
    let start_instant = Instant::now();
    let timeout_duration = Duration::from_millis(site_config.proxy_timeout_ms.0);
    spawn(move || {
        let r = start_server(site_config);
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
            let mut inner_stream = match TcpStream::connect(format!("127.0.0.1:{}", site_config.port)) {
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
            inner_stream.write_all(http_request.join("\r\n").as_bytes()).expect("Could not write request to inner stream");
            inner_stream.write_all(b"\r\n\r\n").expect("Could not write request end to inner stream");

            let mut stream2 = stream.try_clone().expect("Could not clone stream");
            let mut inner_stream2 = inner_stream.try_clone().expect("Could not clone inner stream");
            spawn(move || {
                let mut buf = [0; 8000];
                loop {
                    let Ok(bytes_read) = stream2.read(&mut buf) else {break};
                    if bytes_read == 0 {
                        break;
                    }
                    inner_stream2.write_all(&buf[..bytes_read]).expect("Could not write to inner stream");
                }
            });

            let mut buf = [0; 8000];
            loop {
                let Ok(bytes_read) = inner_stream.read(&mut buf) else {break};
                if bytes_read == 0 {
                    break;
                }
                stream.write_all(&buf[..bytes_read]).expect("Could not write to stream");
            }

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
