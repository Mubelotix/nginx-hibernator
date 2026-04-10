//! A prelude for the nginx-hibernator module.
//!
//! This module re-exports common types and functions to avoid fully qualified syntax.

pub(crate) use crate::check::{
    ensure_service_health_monitor, is_service_up_cached, register_service_health_monitor,
    service_health_state_cached, try_mark_service_starting, ServiceHealthState,
};
pub(crate) use crate::config::{ModuleConfig, ServiceCheckMode};
pub(crate) use crate::hibernate::{spawn_idle_monitor, touch_activity};
pub(crate) use crate::history::SERVICE_HISTORY;
pub(crate) use crate::runtime::spawn_future_on_runtime;
pub(crate) use crate::service::{initiate_service_start, request_service_action, ControllerAction};
pub(crate) use crate::state::{now_ms, now_secs, runtime_for, runtimes, ServiceRuntime};
pub(crate) use crate::{elog, log};
pub(crate) use ngx::core::Status;
pub(crate) use ngx::http::{self, Request};
