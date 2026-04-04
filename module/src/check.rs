use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::config::ServiceCheckMode;

struct ServiceHealthRuntime {
    mode: AtomicU8,
    port: AtomicU16,
    timeout_ms: AtomicU64,
    up_check_interval_ms: AtomicU64,
    down_check_interval_ms: AtomicU64,
    endpoint: Mutex<String>,
    is_up: AtomicBool,
    monitor_started: AtomicBool,
}

#[derive(Clone)]
struct ServiceMonitorConfig {
    service_id: String,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: String,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    down_check_interval_ms: u64,
}

impl ServiceHealthRuntime {
    fn new() -> Self {
        Self {
            mode: AtomicU8::new(mode_to_u8(ServiceCheckMode::Http)),
            port: AtomicU16::new(0),
            timeout_ms: AtomicU64::new(100),
            up_check_interval_ms: AtomicU64::new(10_000),
            down_check_interval_ms: AtomicU64::new(60_000),
            endpoint: Mutex::new("/ready".to_owned()),
            is_up: AtomicBool::new(false),
            monitor_started: AtomicBool::new(false),
        }
    }
}

static SERVICE_HEALTHS: OnceLock<Mutex<HashMap<String, Arc<ServiceHealthRuntime>>>> = OnceLock::new();
static REGISTERED_MONITORS: OnceLock<Mutex<HashMap<String, ServiceMonitorConfig>>> = OnceLock::new();

fn healths() -> &'static Mutex<HashMap<String, Arc<ServiceHealthRuntime>>> {
    SERVICE_HEALTHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registered_monitors() -> &'static Mutex<HashMap<String, ServiceMonitorConfig>> {
    REGISTERED_MONITORS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_for(service_id: &str) -> Arc<ServiceHealthRuntime> {
    let mut map = healths().lock().expect("service health lock poisoned");
    if let Some(existing) = map.get(service_id) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(ServiceHealthRuntime::new());
    map.insert(service_id.to_owned(), Arc::clone(&runtime));
    runtime
}

pub fn ensure_service_health_monitor(
    service_id: &str,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: &str,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    down_check_interval_ms: u64,
) {
    let runtime = runtime_for(service_id);
    apply_health_runtime_config(
        &runtime,
        mode,
        port,
        endpoint,
        timeout_ms,
        up_check_interval_ms,
        down_check_interval_ms,
    );
    start_health_monitor_if_needed(&runtime);
}

fn apply_health_runtime_config(
    runtime: &Arc<ServiceHealthRuntime>,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: &str,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    down_check_interval_ms: u64,
) {
    runtime.mode.store(mode_to_u8(mode), Ordering::Relaxed);
    runtime.port.store(port, Ordering::Relaxed);
    runtime.timeout_ms.store(timeout_ms.max(1), Ordering::Relaxed);
    runtime
        .up_check_interval_ms
        .store(up_check_interval_ms.max(1), Ordering::Relaxed);
    runtime
        .down_check_interval_ms
        .store(down_check_interval_ms.max(1), Ordering::Relaxed);

    if let Ok(mut ep) = runtime.endpoint.lock() {
        ep.clear();
        ep.push_str(endpoint);
    }
}

fn start_health_monitor_if_needed(runtime: &Arc<ServiceHealthRuntime>) {
    if runtime
        .monitor_started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        let runtime = Arc::clone(runtime);
        thread::spawn(move || loop {
            refresh_service_health(&runtime);

            let is_up = runtime.is_up.load(Ordering::Relaxed);
            let interval_ms = if is_up {
                runtime.up_check_interval_ms.load(Ordering::Relaxed)
            } else {
                runtime.down_check_interval_ms.load(Ordering::Relaxed)
            }
            .max(1);

            thread::sleep(Duration::from_millis(interval_ms));
        });
    }
}

pub fn register_service_health_monitor(
    service_id: &str,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: &str,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    down_check_interval_ms: u64,
) {
    let mut map = registered_monitors()
        .lock()
        .expect("registered monitor lock poisoned");
    map.insert(
        service_id.to_owned(),
        ServiceMonitorConfig {
            service_id: service_id.to_owned(),
            mode,
            port,
            endpoint: endpoint.to_owned(),
            timeout_ms,
            up_check_interval_ms,
            down_check_interval_ms,
        },
    );
}

pub fn start_registered_service_health_monitors() {
    let configs: Vec<ServiceMonitorConfig> = {
        let map = registered_monitors()
            .lock()
            .expect("registered monitor lock poisoned");
        map.values().cloned().collect()
    };

    for cfg in configs {
        ensure_service_health_monitor(
            &cfg.service_id,
            cfg.mode,
            cfg.port,
            &cfg.endpoint,
            cfg.timeout_ms,
            cfg.up_check_interval_ms,
            cfg.down_check_interval_ms,
        );
    }
}

pub fn is_service_up_cached(service_id: &str) -> bool {
    let map = healths().lock().expect("service health lock poisoned");
    map.get(service_id)
        .map(|runtime| runtime.is_up.load(Ordering::Relaxed))
        .unwrap_or(false)
}

pub fn set_service_up(service_id: &str, is_up: bool) {
    let runtime = runtime_for(service_id);
    runtime.is_up.store(is_up, Ordering::Relaxed);
}

fn refresh_service_health(runtime: &ServiceHealthRuntime) {
    let mode = mode_from_u8(runtime.mode.load(Ordering::Relaxed));
    let port = runtime.port.load(Ordering::Relaxed);
    let timeout_ms = runtime.timeout_ms.load(Ordering::Relaxed).max(1);
    let endpoint = runtime
        .endpoint
        .lock()
        .map(|ep| ep.clone())
        .unwrap_or_else(|_| "/ready".to_owned());

    let is_up = is_service_up(mode, port, &endpoint, timeout_ms);
    runtime.is_up.store(is_up, Ordering::Relaxed);
}

fn mode_to_u8(mode: ServiceCheckMode) -> u8 {
    match mode {
        ServiceCheckMode::Http => 0,
        ServiceCheckMode::Port => 1,
    }
}

fn mode_from_u8(mode: u8) -> ServiceCheckMode {
    match mode {
        1 => ServiceCheckMode::Port,
        _ => ServiceCheckMode::Http,
    }
}

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
