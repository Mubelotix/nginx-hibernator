use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub fn is_service_up(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
}
