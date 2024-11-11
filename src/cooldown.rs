
use std::{collections::HashMap, sync::{Arc, LazyLock, Mutex}};
use chrono::Utc;
use crate::SiteConfig;

static LAST_STOPPED: LazyLock<Arc<Mutex<HashMap<&'static str, u64>>>> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));
static LAST_STARTED: LazyLock<Arc<Mutex<HashMap<&'static str, u64>>>> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

pub fn get_last_stopped(site_name: &'static str) -> Option<u64> {
    let last_stopped_table = LAST_STOPPED.lock().unwrap();
    last_stopped_table.get(site_name).copied()
}

pub fn mark_stopped(site_name: &'static str) {
    let now = Utc::now().timestamp() as u64;
    let mut last_stopped_table = LAST_STOPPED.lock().unwrap();
    last_stopped_table.insert(site_name, now);
}


pub fn get_last_started(site_name: &'static str) -> Option<u64> {
    let last_started_table = LAST_STARTED.lock().unwrap();
    last_started_table.get(site_name).copied()
}

pub fn try_mark_started(site_config: &'static SiteConfig) -> bool {
    let now = Utc::now().timestamp() as u64;
    let mut last_started_table = LAST_STARTED.lock().unwrap();
    let last_started = last_started_table.get(site_config.name.as_str());
    if let Some(last_started) = last_started {
        if now.saturating_sub(*last_started) < site_config.keep_alive {
            return false;
        }
    }

    last_started_table.insert(site_config.name.as_str(), now);

    true
}
