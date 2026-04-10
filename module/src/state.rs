use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;

use crate::check::ServiceHealthState;
use crate::hibernate::spawn_idle_monitor;

pub struct ServiceRuntime {
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
}

impl ServiceRuntime {
    pub fn new() -> Self {
        Self {
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
}

static SERVICE_RUNTIMES: OnceLock<Mutex<HashMap<String, Arc<ServiceRuntime>>>> = OnceLock::new();

pub fn runtimes() -> &'static Mutex<HashMap<String, Arc<ServiceRuntime>>> {
    SERVICE_RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn runtime_for(service_id: &str) -> Arc<ServiceRuntime> {
    let mut map = runtimes().lock().expect("service runtime lock poisoned");
    if let Some(existing) = map.get(service_id) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(ServiceRuntime::new());
    spawn_idle_monitor(service_id.to_owned(), Arc::clone(&runtime));
    map.insert(service_id.to_owned(), Arc::clone(&runtime));
    runtime
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_secs())
}
