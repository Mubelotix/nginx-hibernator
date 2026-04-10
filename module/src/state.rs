use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use crate::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceHealthState {
    Unknown,
    Up,
    Down,
    Starting,
}

impl ServiceHealthState {
    pub fn as_u8(self) -> u8 {
        match self {
            ServiceHealthState::Unknown => 0,
            ServiceHealthState::Up => 1,
            ServiceHealthState::Down => 2,
            ServiceHealthState::Starting => 3,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => ServiceHealthState::Up,
            2 => ServiceHealthState::Down,
            3 => ServiceHealthState::Starting,
            _ => ServiceHealthState::Unknown,
        }
    }
}

pub struct ServiceRuntime {
    pub service_id: String,
    pub last_activity_secs: AtomicU64,
    pub keep_alive_secs: AtomicU64,
    pub started_by_module: AtomicBool,

    pub mode: AtomicU8,
    pub port: AtomicU16,
    pub timeout_ms: AtomicU64,
    pub up_check_interval_ms: AtomicU64,
    pub starting_check_interval_ms: AtomicU64,
    pub down_check_interval_ms: AtomicU64,
    pub endpoint: Mutex<String>,
    pub state: AtomicU8,
    pub state_change_notify: Notify,
    
    pub startup_start_time_ms: AtomicU64,
    pub expected_startup_duration_ms: AtomicU64,

    pub eta_enabled: AtomicBool,
    pub history_file: Mutex<Option<String>>,
    pub history_samples_count: std::sync::atomic::AtomicUsize,
    pub history_percentile: std::sync::atomic::AtomicUsize,
}

impl ServiceRuntime {
    pub fn new(service_id: String) -> Self {
        Self {
            service_id,
            last_activity_secs: AtomicU64::new(now_secs()),
            keep_alive_secs: AtomicU64::new(300),
            started_by_module: AtomicBool::new(false),

            mode: AtomicU8::new(0),
            port: AtomicU16::new(0),
            timeout_ms: AtomicU64::new(100),
            up_check_interval_ms: AtomicU64::new(10_000),
            starting_check_interval_ms: AtomicU64::new(100),
            down_check_interval_ms: AtomicU64::new(60_000),
            endpoint: Mutex::new("/ready".to_owned()),
            state: AtomicU8::new(ServiceHealthState::Unknown.as_u8()),
            state_change_notify: Notify::new(),
            startup_start_time_ms: AtomicU64::new(0),
            expected_startup_duration_ms: AtomicU64::new(0),
            eta_enabled: AtomicBool::new(true),
            history_file: Mutex::new(None),
            history_samples_count: std::sync::atomic::AtomicUsize::new(40),
            history_percentile: std::sync::atomic::AtomicUsize::new(95),
        }
    }

    pub fn state(&self) -> ServiceHealthState {
        ServiceHealthState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state_without_notify(&self, state: ServiceHealthState) {
        self.state.store(state.as_u8(), Ordering::Relaxed);
    }

    pub fn set_state(&self, state: ServiceHealthState) {
        let previous = self.state.swap(state.as_u8(), Ordering::AcqRel);
        if previous != state.as_u8() {
            self.state_change_notify.notify_waiters();
        }
    }

    pub fn history_file(&self) -> Option<String> {
        if !self.eta_enabled.load(Ordering::Relaxed) {
            return None;
        }
        match self.history_file.lock().unwrap().clone() {
            Some(r) => Some(r),
            None => Some(format!("/var/log/nginx/startup-times-{}.txt", self.service_id))
        }
    }

    pub fn fetch_eta(self: &Arc<Self>) {
        let history_file = self.history_file();
        if let Some(history_file) = history_file {
            let pctl = self.history_percentile.load(Ordering::Relaxed);
            let count = self.history_samples_count.load(Ordering::Relaxed);
            let rt = Arc::clone(self);
            spawn_future_on_runtime(async move {
                if let Some(eta) = SERVICE_HISTORY.get_eta(&history_file, pctl, count).await {
                    rt.expected_startup_duration_ms.store(eta as u64, Ordering::Relaxed);
                }
            });
        }
    }
}

pub static SERVICE_RUNTIMES: LazyLock<Mutex<HashMap<String, Arc<ServiceRuntime>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn runtime_for(service_id: &str) -> Arc<ServiceRuntime> {
    let mut map = SERVICE_RUNTIMES.lock().expect("service runtime lock poisoned");
    if let Some(existing) = map.get(service_id) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(ServiceRuntime::new(service_id.to_owned()));
    spawn_idle_monitor(service_id.to_owned(), Arc::clone(&runtime));
    map.insert(service_id.to_owned(), Arc::clone(&runtime));
    runtime
}

pub fn is_service_up(service_id: &str) -> bool {
    get_service_state(service_id) == ServiceHealthState::Up
}

pub fn get_service_state(service_id: &str) -> ServiceHealthState {
    let map = SERVICE_RUNTIMES.lock().expect("service runtime lock poisoned");
    map.get(service_id)
        .map(|runtime| runtime.state())
        .unwrap_or(ServiceHealthState::Unknown)
}

pub fn set_service_state(service_id: &str, state: ServiceHealthState) {
    let runtime = runtime_for(service_id);
    runtime.set_state(state);
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_secs())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_millis() as u64)
}
