use crate::check::{ServiceMonitorConfig, REGISTERED_MONITORS};
use crate::landing::DEFAULT_LANDING_DIR;
use core::ffi::{c_char, c_void};
use ngx::ffi::{
    ngx_command_t, ngx_conf_t, ngx_str_t, ngx_uint_t, NGX_CONF_1MORE, NGX_CONF_TAKE1,
    NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_LOG_EMERG,
};
use ngx::http::{self, MergeConfigError};
use ngx::{ngx_conf_log_error, ngx_string};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceCheckMode {
    #[default]
    Http,
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderValueMatch {
    StartsWith(String),
    Contains(String),
    EndsWith(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointHeaderCondition {
    pub name: String,
    pub value_match: Option<HeaderValueMatch>,
}

#[derive(Debug)]
pub struct ModuleConfig {
    pub enable: bool,
    pub service_name: Option<String>,
    pub target_port: Option<u16>,
    pub keep_alive_secs: u64,
    pub start_timeout_ms: u64,
    pub start_check_interval_ms: u64,
    pub check_mode: ServiceCheckMode,
    pub check_endpoint: String,
    pub check_timeout_ms: u64,
    pub up_check_interval_ms: u64,
    pub starting_check_interval_ms: u64,
    pub down_check_interval_ms: u64,
    pub landing_dir: String,
    pub checkpoint_enabled: Option<bool>,
    pub checkpoint_bypass_rules: Option<Vec<Vec<CheckpointHeaderCondition>>>,
    pub eta_enabled: Option<bool>,
    pub history_file: Option<String>,
    pub history_samples_count: usize,
    pub history_percentile: usize,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            enable: false,
            service_name: None,
            target_port: None,
            keep_alive_secs: 5 * 60,
            start_timeout_ms: 5 * 60 * 1000,
            start_check_interval_ms: 100,
            check_mode: ServiceCheckMode::Http,
            check_endpoint: "/ready".to_owned(),
            check_timeout_ms: 100,
            up_check_interval_ms: 10_000,
            starting_check_interval_ms: 100,
            down_check_interval_ms: 60_000,
            landing_dir: DEFAULT_LANDING_DIR.to_owned(),
            checkpoint_enabled: None,
            checkpoint_bypass_rules: None,
            eta_enabled: None,
            history_file: None,
            history_samples_count: 40,
            history_percentile: 95,
        }
    }
}

impl http::Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        let defaults = ModuleConfig::default();
        let has_local_checkpoint_bypass_rules = self.checkpoint_bypass_rules.is_some();

        if !self.enable {
            self.enable = prev.enable;
        }

        if self.service_name.is_none() {
            self.service_name = prev.service_name.clone();
        }
        if self.target_port.is_none() {
            self.target_port = prev.target_port;
        }

        if self.keep_alive_secs == defaults.keep_alive_secs {
            self.keep_alive_secs = prev.keep_alive_secs;
        }
        if self.start_timeout_ms == defaults.start_timeout_ms {
            self.start_timeout_ms = prev.start_timeout_ms;
        }
        if self.start_check_interval_ms == defaults.start_check_interval_ms {
            self.start_check_interval_ms = prev.start_check_interval_ms;
        }
        if self.check_mode == defaults.check_mode {
            self.check_mode = prev.check_mode;
        }
        if self.check_endpoint == defaults.check_endpoint {
            self.check_endpoint = prev.check_endpoint.clone();
        }
        if self.check_timeout_ms == defaults.check_timeout_ms {
            self.check_timeout_ms = prev.check_timeout_ms;
        }
        if self.up_check_interval_ms == defaults.up_check_interval_ms {
            self.up_check_interval_ms = prev.up_check_interval_ms;
        }
        if self.starting_check_interval_ms == defaults.starting_check_interval_ms {
            self.starting_check_interval_ms = prev.starting_check_interval_ms;
        }
        if self.down_check_interval_ms == defaults.down_check_interval_ms {
            self.down_check_interval_ms = prev.down_check_interval_ms;
        }
        if self.landing_dir == defaults.landing_dir {
            self.landing_dir = prev.landing_dir.clone();
        }
        if self.checkpoint_enabled.is_none() {
            self.checkpoint_enabled = prev.checkpoint_enabled;
        }
        if self.checkpoint_bypass_rules.is_none() {
            self.checkpoint_bypass_rules = prev.checkpoint_bypass_rules.clone();
        }
        if has_local_checkpoint_bypass_rules && self.checkpoint_enabled != Some(true) {
            return Err(MergeConfigError::NoValue);
        }
        if self.eta_enabled.is_none() {
            self.eta_enabled = prev.eta_enabled;
        }
        if self.history_file == defaults.history_file {
            self.history_file = prev.history_file.clone();
        }
        if self.history_samples_count == defaults.history_samples_count {
            self.history_samples_count = prev.history_samples_count;
        }
        if self.history_percentile == defaults.history_percentile {
            self.history_percentile = prev.history_percentile;
        }

        if self.enable {
            let mut map = REGISTERED_MONITORS.lock().expect("registered monitor lock poisoned");
            if let Some(target_port) = self.target_port {
                let health_service_id = self
                    .service_name
                    .clone()
                    .unwrap_or_else(|| format!("{}:{}", target_port, self.check_endpoint));
                map.insert(
                    health_service_id.clone(),
                        ServiceMonitorConfig {
                        service_id: health_service_id,
                        mode: self.check_mode,
                        port: target_port,
                        endpoint: self.check_endpoint.clone(),
                        timeout_ms: self.check_timeout_ms,
                        up_check_interval_ms: self.up_check_interval_ms,
                        starting_check_interval_ms: self.starting_check_interval_ms,
                        down_check_interval_ms: self.down_check_interval_ms,
                        history_file: if self.eta_enabled.unwrap_or(true) {
                            self.history_file.to_owned()
                        } else {
                            None
                        },
                        history_samples_count: self.history_samples_count,
                        history_percentile: self.history_percentile,
                    }
                );
            }
        }

        Ok(())
    }
}

pub static mut NGX_HTTP_HIBERNATOR_COMMANDS: [ngx_command_t; 20] = [
    ngx_command_t {
        name: ngx_string!("hibernator"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_service_name"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_service_name),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_check_port"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_target_port),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_keep_alive"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_keep_alive),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_start_timeout"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_start_timeout),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_start_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_start_check_interval),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_check_mode"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_check_mode),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_check_endpoint"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_check_endpoint),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_check_timeout"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_check_timeout),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_up_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_up_check_interval),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_starting_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_starting_check_interval),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_down_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_down_check_interval),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_landing_dir"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_landing_dir),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_checkpoint"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_checkpoint_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_checkpoint_bypass"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_1MORE) as ngx_uint_t,
        set: Some(set_checkpoint_bypass),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_history_file"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_history_file),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_history_samples_count"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_history_samples_count),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_history_percentile"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_history_percentile),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_eta"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_eta_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

fn parse_on_off(val: &str) -> Option<bool> {
    if val.eq_ignore_ascii_case("on") {
        Some(true)
    } else if val.eq_ignore_ascii_case("off") {
        Some(false)
    } else {
        None
    }
}

fn parse_service_check_mode(val: &str) -> Option<ServiceCheckMode> {
    if val.eq_ignore_ascii_case("http") {
        Some(ServiceCheckMode::Http)
    } else if val.eq_ignore_ascii_case("tcp") {
        Some(ServiceCheckMode::Tcp)
    } else {
        None
    }
}

fn parse_checkpoint_header_condition(value: &str) -> Option<CheckpointHeaderCondition> {
    let (name, value_match) = if let Some((name, value)) = value.split_once(" starts_with=") {
        Some((name, HeaderValueMatch::StartsWith(value.to_owned())))
    } else if let Some((name, value)) = value.split_once(" contains=") {
        Some((name, HeaderValueMatch::Contains(value.to_owned())))
    } else if let Some((name, value)) = value.split_once(" ends_with=") {
        Some((name, HeaderValueMatch::EndsWith(value.to_owned())))
    } else {
        None
    }
    .map_or((value, None), |(name, matcher)| (name, Some(matcher)));

    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
    {
        return None;
    }
    if matches!(
        &value_match,
        Some(
            HeaderValueMatch::StartsWith(value)
                | HeaderValueMatch::Contains(value)
                | HeaderValueMatch::EndsWith(value)
        ) if value.is_empty()
    ) {
        return None;
    }

    Some(CheckpointHeaderCondition {
        name: name.to_owned(),
        value_match,
    })
}

fn parse_duration_ms(mut val: &str) -> Option<u64> {
    let mul = if let Some(stripped) = val.strip_suffix("ms") {
        val = stripped;
        1
    } else if let Some(stripped) = val.strip_suffix('s') {
        val = stripped;
        1_000
    } else if let Some(stripped) = val.strip_suffix('m') {
        val = stripped;
        60_000
    } else if let Some(stripped) = val.strip_suffix('h') {
        val = stripped;
        3_600_000
    } else if let Some(stripped) = val.strip_suffix('d') {
        val = stripped;
        86_400_000
    } else {
        1
    };

    val.parse::<u64>().ok().map(|n| n.saturating_mul(mul))
}

fn parse_duration_secs(mut val: &str) -> Option<u64> {
    let div = if let Some(stripped) = val.strip_suffix("ms") {
        val = stripped;
        1_000
    } else if let Some(stripped) = val.strip_suffix('s') {
        val = stripped;
        1
    } else if let Some(stripped) = val.strip_suffix('m') {
        val = stripped;
        60
    } else if let Some(stripped) = val.strip_suffix('h') {
        val = stripped;
        3_600
    } else if let Some(stripped) = val.strip_suffix('d') {
        val = stripped;
        86_400
    } else {
        1
    };

    val.parse::<u64>().ok().map(|n| {
        if div == 1_000 {
            n / 1_000
        } else {
            n.saturating_mul(div)
        }
    })
}

fn arg1(cf: *mut ngx_conf_t) -> Result<String, *mut c_char> {
    unsafe {
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();
        match args[1].to_str() {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => {
                ngx_conf_log_error!(NGX_LOG_EMERG, cf, "directive argument is not utf-8 encoded");
                Err(ngx::core::NGX_CONF_ERROR)
            }
        }
    }
}

extern "C" fn set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };

    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(enable) = parse_on_off(&val) {
        conf.enable = enable;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid value: use `on` or `off`");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_checkpoint_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };

    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(enabled) = parse_on_off(&val) {
        conf.checkpoint_enabled = Some(enabled);
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid value: use `on` or `off`");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_checkpoint_bypass(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let conditions = unsafe {
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();
        args.iter()
            .skip(1)
            .map(|arg| arg.to_str().ok().and_then(parse_checkpoint_header_condition))
            .collect::<Option<Vec<_>>>()
    };

    let Some(conditions) = conditions else {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "invalid checkpoint bypass condition; use Header, Header starts_with=value, Header contains=value, or Header ends_with=value"
        );
        return ngx::core::NGX_CONF_ERROR;
    };

    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    conf.checkpoint_bypass_rules
        .get_or_insert_with(Vec::new)
        .push(conditions);
    ngx::core::NGX_CONF_OK
}

extern "C" fn set_service_name(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if val.is_empty() {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "service name cannot be empty");
        return ngx::core::NGX_CONF_ERROR;
    }
    conf.service_name = Some(val);
    ngx::core::NGX_CONF_OK
}

extern "C" fn set_target_port(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    match val.parse::<u16>() {
        Ok(v) if v > 0 => {
            conf.target_port = Some(v);
            ngx::core::NGX_CONF_OK
        }
        _ => {
            ngx_conf_log_error!(NGX_LOG_EMERG, cf, "service port must be a valid TCP port");
            ngx::core::NGX_CONF_ERROR
        }
    }
}

extern "C" fn set_keep_alive(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_secs(&val) {
        conf.keep_alive_secs = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_start_timeout(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.start_timeout_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_start_check_interval(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.start_check_interval_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_check_mode(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(mode) = parse_service_check_mode(&val) {
        conf.check_mode = mode;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "invalid service check mode: use `http` or `tcp`"
        );
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_check_endpoint(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if !val.starts_with('/') {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "invalid service ready endpoint: must start with '/'"
        );
        return ngx::core::NGX_CONF_ERROR;
    }
    conf.check_endpoint = val;
    ngx::core::NGX_CONF_OK
}

extern "C" fn set_check_timeout(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.check_timeout_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_up_check_interval(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.up_check_interval_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_starting_check_interval(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.starting_check_interval_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_down_check_interval(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.down_check_interval_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_landing_dir(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if val.is_empty() {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "landing dir cannot be empty");
        return ngx::core::NGX_CONF_ERROR;
    }
    conf.landing_dir = val;
    ngx::core::NGX_CONF_OK
}

extern "C" fn set_history_file(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if val.is_empty() {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "history file cannot be empty");
        return ngx::core::NGX_CONF_ERROR;
    }
    conf.history_file = Some(val);
    ngx::core::NGX_CONF_OK
}

extern "C" fn set_history_samples_count(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    match val.parse::<usize>() {
        Ok(v) if v > 0 => {
            conf.history_samples_count = v;
            ngx::core::NGX_CONF_OK
        }
        _ => {
            ngx_conf_log_error!(
                NGX_LOG_EMERG,
                cf,
                "history samples count must be a valid positive integer"
            );
            ngx::core::NGX_CONF_ERROR
        }
    }
}

extern "C" fn set_history_percentile(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    match val.parse::<usize>() {
        Ok(v) if v <= 100 => {
            conf.history_percentile = v;
            ngx::core::NGX_CONF_OK
        }
        _ => {
            ngx_conf_log_error!(
                NGX_LOG_EMERG,
                cf,
                "history percentile must be an integer between 0 and 100"
            );
            ngx::core::NGX_CONF_ERROR
        }
    }
}

extern "C" fn set_eta_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };

    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(enable) = parse_on_off(&val) {
        conf.eta_enabled = Some(enable);
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid value: use `on` or `off`");
        ngx::core::NGX_CONF_ERROR
    }
}

#[cfg(test)]
mod checkpoint_bypass_tests {
    use super::{parse_checkpoint_header_condition, CheckpointHeaderCondition, HeaderValueMatch};

    #[test]
    fn parses_header_presence_and_value_matchers() {
        assert_eq!(
            parse_checkpoint_header_condition("X-Flag"),
            Some(CheckpointHeaderCondition {
                name: "X-Flag".to_owned(),
                value_match: None,
            })
        );
        assert_eq!(
            parse_checkpoint_header_condition("X-Token starts_with=trusted-"),
            Some(CheckpointHeaderCondition {
                name: "X-Token".to_owned(),
                value_match: Some(HeaderValueMatch::StartsWith("trusted-".to_owned())),
            })
        );
        assert_eq!(
            parse_checkpoint_header_condition("X-Trace contains=internal value"),
            Some(CheckpointHeaderCondition {
                name: "X-Trace".to_owned(),
                value_match: Some(HeaderValueMatch::Contains("internal value".to_owned())),
            })
        );
        assert_eq!(
            parse_checkpoint_header_condition("X-Client ends_with=-agent")
                .and_then(|condition| condition.value_match),
            Some(HeaderValueMatch::EndsWith("-agent".to_owned()))
        );
        assert!(parse_checkpoint_header_condition("bad header").is_none());
        assert!(parse_checkpoint_header_condition("X-Token starts_with=").is_none());
    }
}
