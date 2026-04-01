use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::health;
use ngx::ffi::{NGX_LOG_ERR, NGX_LOG_NOTICE};
use ngx::ngx_log_error;

macro_rules! log {
    ($($arg:tt)+) => {
        ngx_log_error!(NGX_LOG_NOTICE, ngx::log::ngx_cycle_log().as_ptr(), $($arg)+);
    };
}

macro_rules! elog {
    ($($arg:tt)+) => {
        ngx_log_error!(NGX_LOG_ERR, ngx::log::ngx_cycle_log().as_ptr(), $($arg)+);
    };
}

struct ServiceRuntime {
    last_activity_secs: AtomicU64,
    keep_alive_secs: AtomicU64,
    started_by_module: AtomicBool,
    starting: AtomicBool,
}

impl ServiceRuntime {
    fn new() -> Self {
        Self {
            last_activity_secs: AtomicU64::new(now_secs()),
            keep_alive_secs: AtomicU64::new(300),
            started_by_module: AtomicBool::new(false),
            starting: AtomicBool::new(false),
        }
    }
}

static SERVICE_RUNTIMES: OnceLock<Mutex<HashMap<String, Arc<ServiceRuntime>>>> = OnceLock::new();

fn runtimes() -> &'static Mutex<HashMap<String, Arc<ServiceRuntime>>> {
    SERVICE_RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_for(service_name: &str) -> Arc<ServiceRuntime> {
    let mut map = runtimes().lock().expect("service runtime lock poisoned");
    if let Some(existing) = map.get(service_name) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(ServiceRuntime::new());
    spawn_idle_monitor(service_name.to_owned(), Arc::clone(&runtime));
    map.insert(service_name.to_owned(), Arc::clone(&runtime));
    runtime
}

fn spawn_idle_monitor(service_name: String, runtime: Arc<ServiceRuntime>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));

            if !runtime.started_by_module.load(Ordering::Relaxed) {
                continue;
            }

            let keep_alive = runtime.keep_alive_secs.load(Ordering::Relaxed);
            if keep_alive == 0 {
                continue;
            }

            let now = now_secs();
            let last = runtime.last_activity_secs.load(Ordering::Relaxed);
            let idle = now.saturating_sub(last);

            if idle >= keep_alive {
                log!(
                    "hibernator: stopping service {} after {}s of idle time",
                    service_name,
                    idle
                );
                if run_systemctl(&["stop", &service_name]) {
                    runtime.started_by_module.store(false, Ordering::Relaxed);
                } else {
                    elog!("hibernator: failed to stop service {}", service_name);
                }
            }
        }
    });
}

fn run_systemctl(args: &[&str]) -> bool {
    let direct = Command::new("systemctl")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if direct {
        return true;
    }

    let sudo_ok = Command::new("sudo")
        .args(["-n", "systemctl"])
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !sudo_ok {
        elog!("hibernator: systemctl {:?} failed", args);
    }

    sudo_ok
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_secs())
}

pub fn touch_activity(service_name: &str, keep_alive_secs: u64) {
    let rt = runtime_for(service_name);
    rt.keep_alive_secs.store(keep_alive_secs, Ordering::Relaxed);
    rt.last_activity_secs.store(now_secs(), Ordering::Relaxed);
}

pub fn start_service_and_wait_ready(
    service_name: &str,
    target_port: u16,
    timeout_ms: u64,
    check_interval_ms: u64,
) -> bool {
    log!("hibernator: starting service {}", service_name);
    if !run_systemctl(&["start", service_name]) {
        elog!("hibernator: failed to start service {}", service_name);
        return false;
    }

    let interval = check_interval_ms.max(1);
    let max_checks = timeout_ms.saturating_div(interval).max(1);

    for _ in 0..max_checks {
        if health::is_service_up(target_port) {
            let rt = runtime_for(service_name);
            rt.started_by_module.store(true, Ordering::Relaxed);
            rt.last_activity_secs.store(now_secs(), Ordering::Relaxed);
            log!("hibernator: service {} is ready", service_name);
            return true;
        }
        thread::sleep(Duration::from_millis(interval));
    }

    elog!("hibernator: service {} did not become ready in time", service_name);
    false
}

pub fn start_service_async(service_name: &str, target_port: u16, timeout_ms: u64, check_interval_ms: u64) {
    let service_name_owned = service_name.to_owned();
    let rt = runtime_for(&service_name_owned);

    if rt
        .starting
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    thread::spawn(move || {
        log!(
            "hibernator: scheduling async start for service {}",
            service_name_owned
        );
        let started = start_service_and_wait_ready(
            &service_name_owned,
            target_port,
            timeout_ms,
            check_interval_ms,
        );

        if started {
            let runtime = runtime_for(&service_name_owned);
            runtime.started_by_module.store(true, Ordering::Relaxed);
            runtime.last_activity_secs.store(now_secs(), Ordering::Relaxed);
        }

        let runtime = runtime_for(&service_name_owned);
        runtime.starting.store(false, Ordering::Release);
    });
}
