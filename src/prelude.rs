//! A prelude for the nginx-hibernator module.
//!
//! This module re-exports common types and functions to avoid fully qualified syntax.

use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use crate::config::ServiceCheckMode;
pub(crate) use crate::hibernate::spawn_idle_monitor;
pub(crate) use crate::history::SERVICE_HISTORY;
pub(crate) use crate::runtime::spawn_future_on_runtime;
pub(crate) use crate::service::{request_service_action, ControllerAction};
pub(crate) use crate::state::{
    runtime_for, ServiceHealthState, ServiceRuntime,
};
pub(crate) use ngx::http::{self, Request};

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

