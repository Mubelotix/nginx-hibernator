use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use crate::prelude::*;

pub fn touch_activity(service_name: &str, keep_alive_secs: u64) {
    let rt = runtime_for(service_name);
    rt.keep_alive_secs.store(keep_alive_secs, Ordering::Relaxed);
    rt.shared.store_last_activity_secs(now_secs());
}

pub(crate) fn spawn_idle_monitor(service_name: String, runtime: Arc<ServiceRuntime>) {
    let service_name_for_error = service_name.clone();
    let spawned = spawn_future_on_runtime(async move {
        loop {
            sleep(Duration::from_secs(1)).await;

            let keep_alive = runtime.keep_alive_secs.load(Ordering::Relaxed);
            if keep_alive == 0 {
                continue;
            }

            let now = now_secs();
            let last = runtime.shared.load_last_activity_secs();
            let idle = now.saturating_sub(last);

            if idle >= keep_alive && runtime.state() == ServiceHealthState::Up {
                log!(
                    "hibernator: stopping service {} after {}s of idle time",
                    service_name,
                    idle
                );
                let stopped = request_service_action(ControllerAction::Stop, &service_name).await;
                if !stopped {
                    elog!("hibernator: failed to stop service {}", service_name);
                }
            }
        }
    });

    if spawned.is_none() {
        elog!(
            "hibernator: failed to spawn idle monitor for {} on async runtime",
            service_name_for_error
        );
    }
}
