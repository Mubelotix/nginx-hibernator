use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::sleep;

struct HibernateRuntime {
    last_activity_secs: AtomicU64,
    keep_alive_secs: AtomicU64,
    started_by_module: AtomicBool,
}

impl HibernateRuntime {
    fn new() -> Self {
        Self {
            last_activity_secs: AtomicU64::new(now_secs()),
            keep_alive_secs: AtomicU64::new(300),
            started_by_module: AtomicBool::new(false),
        }
    }
}

static HIBERNATE_RUNTIMES: OnceLock<Mutex<HashMap<String, Arc<HibernateRuntime>>>> = OnceLock::new();

fn runtimes() -> &'static Mutex<HashMap<String, Arc<HibernateRuntime>>> {
    HIBERNATE_RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_for(service_name: &str) -> Arc<HibernateRuntime> {
    let mut map = runtimes().lock().expect("hibernate runtime lock poisoned");
    if let Some(existing) = map.get(service_name) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(HibernateRuntime::new());
    spawn_idle_monitor(service_name.to_owned(), Arc::clone(&runtime));
    map.insert(service_name.to_owned(), Arc::clone(&runtime));
    runtime
}

pub fn touch_activity(service_name: &str, keep_alive_secs: u64) {
    let rt = runtime_for(service_name);
    rt.keep_alive_secs.store(keep_alive_secs, Ordering::Relaxed);
    rt.last_activity_secs.store(now_secs(), Ordering::Relaxed);
}

pub fn mark_service_started(service_name: &str) {
    let rt = runtime_for(service_name);
    rt.started_by_module.store(true, Ordering::Relaxed);
    rt.last_activity_secs.store(now_secs(), Ordering::Relaxed);
}

fn spawn_idle_monitor(service_name: String, runtime: Arc<HibernateRuntime>) {
    let service_name_for_error = service_name.clone();
    let spawned = crate::runtime::spawn_future_on_runtime(async move {
        loop {
            sleep(Duration::from_secs(1)).await;

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
                crate::log!(
                    "hibernator: stopping service {} after {}s of idle time",
                    service_name,
                    idle
                );
                crate::service::initiate_service_stop(service_name.clone());
                // TODO await
            }
        }
    });

    if spawned.is_none() {
        crate::elog!(
            "hibernator: failed to spawn idle monitor for {} on async runtime",
            service_name_for_error
        );
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_secs())
}
