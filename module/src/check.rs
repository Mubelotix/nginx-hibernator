use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio::time::timeout;

use crate::config::ServiceCheckMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceHealthState {
    Unknown,
    Up,
    Down,
    Starting,
}

impl ServiceHealthState {
    fn as_u8(self) -> u8 {
        match self {
            ServiceHealthState::Unknown => 0,
            ServiceHealthState::Up => 1,
            ServiceHealthState::Down => 2,
            ServiceHealthState::Starting => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => ServiceHealthState::Up,
            2 => ServiceHealthState::Down,
            3 => ServiceHealthState::Starting,
            _ => ServiceHealthState::Unknown,
        }
    }
}

struct ServiceHealthRuntime {
    mode: AtomicU8,
    port: AtomicU16,
    timeout_ms: AtomicU64,
    up_check_interval_ms: AtomicU64,
    starting_check_interval_ms: AtomicU64,
    down_check_interval_ms: AtomicU64,
    endpoint: Mutex<String>,
    state: AtomicU8,
    state_change_notify: Notify,
}

#[derive(Clone)]
struct ServiceMonitorConfig {
    service_id: String,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: String,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    starting_check_interval_ms: u64,
    down_check_interval_ms: u64,
}

impl ServiceHealthRuntime {
    fn new() -> Self {
        Self {
            mode: AtomicU8::new(mode_to_u8(ServiceCheckMode::Http)),
            port: AtomicU16::new(0),
            timeout_ms: AtomicU64::new(100),
            up_check_interval_ms: AtomicU64::new(10_000),
            starting_check_interval_ms: AtomicU64::new(100),
            down_check_interval_ms: AtomicU64::new(60_000),
            endpoint: Mutex::new("/ready".to_owned()),
            state: AtomicU8::new(ServiceHealthState::Unknown.as_u8()),
            state_change_notify: Notify::new(),
        }
    }

    fn state(&self) -> ServiceHealthState {
        ServiceHealthState::from_u8(self.state.load(Ordering::Relaxed))
    }

    fn set_state_without_notify(&self, state: ServiceHealthState) {
        self.state.store(state.as_u8(), Ordering::Relaxed);
    }

    fn set_state(&self, state: ServiceHealthState) {
        let previous = self.state.swap(state.as_u8(), Ordering::AcqRel);
        if previous != state.as_u8() {
            self.state_change_notify.notify_waiters();
        }
    }
}

static SERVICE_HEALTHS: OnceLock<Mutex<HashMap<String, Arc<ServiceHealthRuntime>>>> = OnceLock::new();
static REGISTERED_MONITORS: OnceLock<Mutex<HashMap<String, ServiceMonitorConfig>>> = OnceLock::new();
static HEALTH_MONITOR_TASK_STARTED: AtomicBool = AtomicBool::new(false);

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
    starting_check_interval_ms: u64,
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
        starting_check_interval_ms,
        down_check_interval_ms,
    );
    start_health_monitor_task_if_needed();
}

fn apply_health_runtime_config(
    runtime: &Arc<ServiceHealthRuntime>,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: &str,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    starting_check_interval_ms: u64,
    down_check_interval_ms: u64,
) {
    runtime.mode.store(mode_to_u8(mode), Ordering::Relaxed);
    runtime.port.store(port, Ordering::Relaxed);
    runtime.timeout_ms.store(timeout_ms.max(1), Ordering::Relaxed);
    runtime
        .up_check_interval_ms
        .store(up_check_interval_ms.max(1), Ordering::Relaxed);
    runtime
        .starting_check_interval_ms
        .store(starting_check_interval_ms.max(1), Ordering::Relaxed);
    runtime
        .down_check_interval_ms
        .store(down_check_interval_ms.max(1), Ordering::Relaxed);

    if let Ok(mut ep) = runtime.endpoint.lock() {
        ep.clear();
        ep.push_str(endpoint);
    }
}

fn start_health_monitor_task_if_needed() {
    // Exit if already started by another thread
    if HEALTH_MONITOR_TASK_STARTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    let runtimes: Vec<Arc<ServiceHealthRuntime>> = {
        let map = healths().lock().expect("service health lock poisoned");
        map.values().cloned().collect()
    };

    for runtime in runtimes {
        crate::runtime::spawn_future_on_runtime(monitor_service_health(runtime));
    }
}

async fn monitor_service_health(runtime: Arc<ServiceHealthRuntime>) {
    loop {
        let interval_ms = match runtime.state() {
            ServiceHealthState::Up => runtime.up_check_interval_ms.load(Ordering::Relaxed),
            ServiceHealthState::Starting => runtime.starting_check_interval_ms.load(Ordering::Relaxed),
            ServiceHealthState::Down => runtime.down_check_interval_ms.load(Ordering::Relaxed),
            ServiceHealthState::Unknown => 0
        };

        let notified = timeout(
            Duration::from_millis(interval_ms),
            runtime.state_change_notify.notified(),
        )
        .await.is_ok();

        if notified {
            continue;
        }

        let is_up = refresh_service_health_async(&runtime).await;
        let next_state = if is_up {
            ServiceHealthState::Up
        } else if runtime.state() == ServiceHealthState::Starting {
            ServiceHealthState::Starting
        } else {
            ServiceHealthState::Down
        };
        runtime.set_state_without_notify(next_state);        
    }
}

pub fn register_service_health_monitor(
    service_id: &str,
    mode: ServiceCheckMode,
    port: u16,
    endpoint: &str,
    timeout_ms: u64,
    up_check_interval_ms: u64,
    starting_check_interval_ms: u64,
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
            starting_check_interval_ms,
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
            cfg.starting_check_interval_ms,
            cfg.down_check_interval_ms,
        );
    }
}

pub fn is_service_up_cached(service_id: &str) -> bool {
    service_health_state_cached(service_id) == ServiceHealthState::Up
}

pub fn service_health_state_cached(service_id: &str) -> ServiceHealthState {
    let map = healths().lock().expect("service health lock poisoned");
    map.get(service_id)
        .map(|runtime| runtime.state())
        .unwrap_or(ServiceHealthState::Unknown)
}

pub fn set_service_state(service_id: &str, state: ServiceHealthState) {
    let runtime = runtime_for(service_id);
    runtime.set_state(state);
}

pub fn try_mark_service_starting(service_id: &str) -> bool {
    let runtime = runtime_for(service_id);
    let target = ServiceHealthState::Starting.as_u8();
    runtime
        .state
        .compare_exchange(
            ServiceHealthState::Unknown.as_u8(),
            target,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_ok()
        || runtime
            .state
            .compare_exchange(
                ServiceHealthState::Down.as_u8(),
                target,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
}

async fn refresh_service_health_async(runtime: &ServiceHealthRuntime) -> bool {
    let mode = mode_from_u8(runtime.mode.load(Ordering::Relaxed));
    let port = runtime.port.load(Ordering::Relaxed);
    let timeout_ms = runtime.timeout_ms.load(Ordering::Relaxed).max(1);
    let endpoint = runtime
        .endpoint
        .lock()
        .map(|ep| ep.clone())
        .unwrap_or_else(|_| "/ready".to_owned());

    is_service_up_async(mode, port, &endpoint, timeout_ms).await
}

fn mode_to_u8(mode: ServiceCheckMode) -> u8 {
    match mode {
        ServiceCheckMode::Http => 0,
        ServiceCheckMode::Tcp => 1,
    }
}

fn mode_from_u8(mode: u8) -> ServiceCheckMode {
    match mode {
        1 => ServiceCheckMode::Tcp,
        _ => ServiceCheckMode::Http,
    }
}

pub fn is_service_up(mode: ServiceCheckMode, port: u16, endpoint: &str, timeout_ms: u64) -> bool {
    match mode {
        ServiceCheckMode::Http => is_service_up_http(port, endpoint, timeout_ms),
        ServiceCheckMode::Tcp => is_service_up_port(port, timeout_ms),
    }
}

async fn is_service_up_async(mode: ServiceCheckMode, port: u16, endpoint: &str, timeout_ms: u64) -> bool {
    match mode {
        ServiceCheckMode::Http => is_service_up_http_async(port, endpoint, timeout_ms).await,
        ServiceCheckMode::Tcp => is_service_up_port_async(port, timeout_ms).await,
    }
}

async fn is_service_up_port_async(port: u16, timeout_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    timeout(
        Duration::from_millis(timeout_ms.max(1)),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    .is_ok_and(|res| res.is_ok())
}

async fn is_service_up_http_async(port: u16, endpoint: &str, timeout_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout_dur = Duration::from_millis(timeout_ms.max(1));

    let Ok(connect_result) = timeout(timeout_dur, tokio::net::TcpStream::connect(addr)).await else {
        return false;
    };
    let Ok(mut stream) = connect_result else {
        return false;
    };

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        endpoint
    );

    if timeout(timeout_dur, stream.write_all(request.as_bytes()))
        .await
        .is_err()
    {
        return false;
    }

    let mut buf = [0_u8; 512];
    let Ok(read_result) = timeout(timeout_dur, stream.read(&mut buf)).await else {
        return false;
    };
    let Ok(n) = read_result else {
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
