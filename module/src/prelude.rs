//! A prelude for the nginx-hibernator module.
//!
//! This module re-exports common types and functions to avoid fully qualified syntax.

pub use crate::check::{
    ensure_service_health_monitor, is_service_up_cached, register_service_health_monitor,
    service_health_state_cached, try_mark_service_starting, ServiceHealthState,
};
pub use crate::config::{ModuleConfig, ServiceCheckMode};
pub use crate::hibernate::{spawn_idle_monitor, touch_activity};
pub use crate::history::SERVICE_HISTORY;
pub use crate::runtime::spawn_future_on_runtime;
pub use crate::service::{initiate_service_start, request_service_action, ControllerAction};
pub use crate::state::{now_ms, now_secs, runtime_for, runtimes, ServiceRuntime};
pub use crate::{elog, log};
pub use ngx::core::Status;
pub use ngx::http::{self, Request};
