use std::{io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}, thread::spawn};

use crate::{start_server, Config, TopLevelConfig};

pub fn setup_server(config: &'static Config) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.top_level.hibernator_port())).unwrap();

    for stream in listener.incoming() {
        let Ok(stream) = stream else {continue};
        spawn(move || handle_connection(stream, config));
    }
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
            let contents = "Hibernator requires a Host header";
            let length = contents.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
            stream.write_all(response.as_bytes()).unwrap();
            return;
        }
    };

    let site_index = config.sites.iter().position(|site| site.hosts.contains(&host));

    let site_index = match site_index {
        Some(site_index) => site_index,
        None => {
            let status_line = "HTTP/1.1 404 Not Found";
            let contents = "Hibernator doesn't know about the site you're trying to access";
            let length = contents.len();
            let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
            stream.write_all(response.as_bytes()).unwrap();
            return;
        }
    };

    let status_line = "HTTP/1.1 503 Service Unavailable";
    let contents = include_str!("../static/index.html");
    let length = contents.len();
    let response = format!(
        "{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}"
    );
    stream.write_all(response.as_bytes()).unwrap();

    start_server(&config.sites[site_index], site_index).unwrap();
}

