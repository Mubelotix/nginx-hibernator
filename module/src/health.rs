use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::ServiceCheckMode;

pub fn is_service_up(mode: ServiceCheckMode, port: u16, endpoint: &str, timeout_ms: u64) -> bool {
    match mode {
        ServiceCheckMode::Http => is_service_up_http(port, endpoint, timeout_ms),
        ServiceCheckMode::Port => is_service_up_port(port, timeout_ms),
    }
}

fn is_service_up_port(port: u16, timeout_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms.max(1))).is_ok()
}

fn is_service_up_http(port: u16, endpoint: &str, timeout_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout = Duration::from_millis(timeout_ms.max(1));

    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        return false;
    };

    if stream.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    if stream.set_write_timeout(Some(timeout)).is_err() {
        return false;
    }

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        endpoint
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut buf = [0_u8; 512];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    if n == 0 {
        return false;
    }

    let Ok(head) = std::str::from_utf8(&buf[..n]) else {
        return false;
    };
    let Some(first_line) = head.lines().next() else {
        return false;
    };

    is_valid_http_status_line(first_line)
}

fn is_valid_http_status_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(http_version) = parts.next() else {
        return false;
    };
    let Some(status) = parts.next() else {
        return false;
    };

    (http_version.starts_with("HTTP/1.") || http_version == "HTTP/2")
        && status.len() == 3
        && status.chars().all(|c| c.is_ascii_digit())
}
