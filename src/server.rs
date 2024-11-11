use std::{fs::read_to_string, io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}, thread::spawn};

use crate::{start_server, Config, TopLevelConfig};

pub fn setup_server(config: &'static Config) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.top_level.hibernator_port())).expect("Could not bind to port");

    spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {continue};
            spawn(move || handle_connection(stream, config));
        }
    });
}

fn handle_connection(mut stream: TcpStream, config: &'static Config) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    let host = http_request
        .iter()
        .find(|line| line.starts_with("Host: "))
        .map(|line| &line[6..])
        .map(|host| host.to_string());

    let host = match host {
        Some(host) => host,
        None => {
            let status_line = "HTTP/1.1 400 Bad Request";
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
            let status_line = "HTTP/1.1 404 Not Found";
            let content = "Hibernator doesn't know about the site you're trying to access";
            let length = content.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");
            let _ = stream.write_all(response.as_bytes());
            return;
        }
    };

    let status_line = "HTTP/1.1 503 Service Unavailable";
    let content = include_str!("../static/index.html").replace("KEEP_ALIVE", &site_config.keep_alive.to_string());
    let length = content.len();
    let response = format!(
        "{status_line}\r\nContent-Length: {length}\r\n\r\n{content}"
    );
    let _ = stream.write_all(response.as_bytes());

    let r = start_server(site_config);
    if let Err(e) = r {
        eprintln!("Error while starting site {}: {e}", site_config.name);
    }
}

