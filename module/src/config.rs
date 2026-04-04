use core::ffi::{c_char, c_void};

use ngx::ffi::{
    NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_LOG_EMERG, ngx_command_t,
    ngx_conf_t, ngx_str_t, ngx_uint_t,
};
use ngx::http::{self, MergeConfigError};
use ngx::{ngx_conf_log_error, ngx_string};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Always,
    WhenReady,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCheckMode {
    Http,
    Port,
}

impl Default for ServiceCheckMode {
    fn default() -> Self {
        Self::Http
    }
}

impl Default for ProxyMode {
    fn default() -> Self {
        Self::Always
    }
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
    pub down_check_interval_ms: u64,
    pub proxy_mode: ProxyMode,
    pub browser_proxy_mode: ProxyMode,
    pub proxy_timeout_ms: u64,
    pub proxy_check_interval_ms: u64,
    pub landing_dir: Option<String>,
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
            down_check_interval_ms: 60_000,
            proxy_mode: ProxyMode::Always,
            browser_proxy_mode: ProxyMode::WhenReady,
            proxy_timeout_ms: 28_000,
            proxy_check_interval_ms: 500,
            landing_dir: None,
        }
    }
}

impl http::Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        let defaults = ModuleConfig::default();

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
        if self.down_check_interval_ms == defaults.down_check_interval_ms {
            self.down_check_interval_ms = prev.down_check_interval_ms;
        }
        if self.proxy_mode == defaults.proxy_mode {
            self.proxy_mode = prev.proxy_mode;
        }
        if self.browser_proxy_mode == defaults.browser_proxy_mode {
            self.browser_proxy_mode = prev.browser_proxy_mode;
        }
        if self.proxy_timeout_ms == defaults.proxy_timeout_ms {
            self.proxy_timeout_ms = prev.proxy_timeout_ms;
        }
        if self.proxy_check_interval_ms == defaults.proxy_check_interval_ms {
            self.proxy_check_interval_ms = prev.proxy_check_interval_ms;
        }
        if self.landing_dir.is_none() {
            self.landing_dir = prev.landing_dir.clone();
        }

        Ok(())
    }
}

pub static mut NGX_HTTP_HIBERNATOR_COMMANDS: [ngx_command_t; 17] = [
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
        name: ngx_string!("hibernator_down_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_down_check_interval),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_proxy_mode"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_proxy_mode),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_browser_proxy_mode"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_browser_proxy_mode),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_proxy_timeout"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_proxy_timeout),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("hibernator_proxy_check_interval"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(set_proxy_check_interval),
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

fn parse_proxy_mode(val: &str) -> Option<ProxyMode> {
    if val.eq_ignore_ascii_case("always") {
        Some(ProxyMode::Always)
    } else if val.eq_ignore_ascii_case("when_ready") || val.eq_ignore_ascii_case("when-ready") || val.eq_ignore_ascii_case("ready") {
        Some(ProxyMode::WhenReady)
    } else if val.eq_ignore_ascii_case("never") {
        Some(ProxyMode::Never)
    } else {
        None
    }
}

fn parse_service_check_mode(val: &str) -> Option<ServiceCheckMode> {
    if val.eq_ignore_ascii_case("http") {
        Some(ServiceCheckMode::Http)
    } else if val.eq_ignore_ascii_case("port") {
        Some(ServiceCheckMode::Port)
    } else {
        None
    }
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

    val.parse::<u64>().ok().map(|n| if div == 1_000 { n / 1_000 } else { n.saturating_mul(div) })
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

extern "C" fn set_enable(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_service_name(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_target_port(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_keep_alive(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_start_timeout(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_start_check_interval(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_check_mode(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(mode) = parse_service_check_mode(&val) {
        conf.check_mode = mode;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid service check mode: use `http` or `port`");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_check_endpoint(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_check_timeout(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_up_check_interval(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_down_check_interval(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
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

extern "C" fn set_proxy_mode(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(mode) = parse_proxy_mode(&val) {
        conf.proxy_mode = mode;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid proxy mode");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_browser_proxy_mode(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    set_proxy_mode(cf, _cmd, conf)
}

extern "C" fn set_proxy_timeout(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.proxy_timeout_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_proxy_check_interval(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if let Some(v) = parse_duration_ms(&val) {
        conf.proxy_check_interval_ms = v;
        ngx::core::NGX_CONF_OK
    } else {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid duration value");
        ngx::core::NGX_CONF_ERROR
    }
}

extern "C" fn set_landing_dir(cf: *mut ngx_conf_t, _cmd: *mut ngx_command_t, conf: *mut c_void) -> *mut c_char {
    let Ok(val) = arg1(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    let conf = unsafe { &mut *(conf as *mut ModuleConfig) };
    if val.is_empty() {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "landing dir cannot be empty");
        return ngx::core::NGX_CONF_ERROR;
    }
    conf.landing_dir = Some(val);
    ngx::core::NGX_CONF_OK
}

