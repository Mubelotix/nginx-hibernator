use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::sleep;



pub fn touch_activity(service_name: &str, keep_alive_secs: u64) {
    let rt = crate::state::runtime_for(service_name);
    rt.keep_alive_secs.store(keep_alive_secs, Ordering::Relaxed);
    rt.last_activity_secs.store(crate::state::now_secs(), Ordering::Relaxed);
}

pub fn mark_service_started(service_name: &str) {
    let rt = crate::state::runtime_for(service_name);
    rt.started_by_module.store(true, Ordering::Relaxed);
    rt.last_activity_secs.store(crate::state::now_secs(), Ordering::Relaxed);
}

pub(crate) fn spawn_idle_monitor(service_name: String, runtime: Arc<crate::state::ServiceRuntime>) {
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

            let now = crate::state::now_secs();
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


