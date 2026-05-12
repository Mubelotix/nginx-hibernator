use ngx::core::Status;
use ngx::ffi::{
    ngx_array_push, ngx_conf_t, ngx_cycle_t, ngx_http_conf_ctx_t, ngx_http_core_main_conf_t,
    ngx_http_core_module, ngx_http_handler_pt, ngx_http_module_t,
    ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_http_request_t, ngx_int_t, ngx_module_t,
    NGX_HTTP_MODULE, NGX_LOG_EMERG,
};
use ngx::http::{self, HttpModule, HttpModuleLocationConf, Request};
use ngx::{ngx_conf_log_error, ngx_log_debug_http};

#[macro_export]
macro_rules! log {
    ($($arg:tt)+) => {
        ngx::ngx_log_error!(
            ngx::ffi::NGX_LOG_NOTICE,
            ngx::log::ngx_cycle_log().as_ptr(),
            $($arg)+
        );
    };
}

#[macro_export]
macro_rules! elog {
    ($($arg:tt)+) => {
        ngx::ngx_log_error!(
            ngx::ffi::NGX_LOG_ERR,
            ngx::log::ngx_cycle_log().as_ptr(),
            $($arg)+
        );
    };
}

mod check;
mod config;
mod hibernate;
mod history;
mod landing;
mod prelude;
mod runtime;
mod service;
mod state;
mod nginx_async;
use check::{
    ensure_service_health_monitor, start_registered_service_health_monitors,
};
use state::{ensure_shared_state_zone, is_service_up};
use config::{ModuleConfig, NGX_HTTP_HIBERNATOR_COMMANDS};
use hibernate::touch_activity;
use landing::{is_landing_prefixed_uri, serve_landing_page, serve_landing_prefixed_asset};
use service::initiate_service_start;

use crate::service::CONTROLLER_TX;

pub(crate) struct Module;

impl http::HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_hibernator_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        if ensure_shared_state_zone(cf) && register_access_handler(cf).is_ok() {
            Status::NGX_OK.into()
        } else {
            Status::NGX_ERROR.into()
        }
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static NGX_HTTP_HIBERNATOR_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: Some(Module::preconfiguration),
    postconfiguration: Some(Module::postconfiguration),
    create_main_conf: None,
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: Some(Module::create_loc_conf),
    merge_loc_conf: Some(Module::merge_loc_conf),
};

#[cfg(feature = "export-modules")]
ngx::ngx_modules!(ngx_http_hibernator_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_hibernator_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_HIBERNATOR_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_HIBERNATOR_COMMANDS[0] },
    type_: NGX_HTTP_MODULE as _,
    init_process: Some(hibernator_init_process),
    ..ngx_module_t::default()
};

unsafe extern "C" fn hibernator_init_process(_cycle: *mut ngx_cycle_t) -> ngx_int_t {
    let _ = &CONTROLLER_TX;
    start_registered_service_health_monitors();
    Status::NGX_OK.into()
}

struct HibernatorRequestHandler;

impl HibernatorRequestHandler {
    fn handler(request: &mut http::Request) -> Status {
        let Some(conf) = Module::location_conf(request) else {
            return Status::NGX_ERROR;
        };

        if !conf.enable {
            return Status::NGX_DECLINED;
        }

        if let Ok(uri) = request.path().to_str() {
            if is_landing_prefixed_uri(uri) {
                return serve_landing_prefixed_asset(request, &conf.landing_dir);
            }
        }

        if let Some(service_name) = conf.service_name.as_deref() {
            touch_activity(service_name, conf.keep_alive_secs);
        }

        let health_service_id = conf.service_name.clone().unwrap_or_else(|| {
            format!("{}:{}", conf.target_port.unwrap_or(0), conf.check_endpoint)
        });

        let Some(target_port) = conf.target_port else {
            ngx_log_debug_http!(
                request,
                "hibernator enabled=1 missing target_port, serving landing"
            );
            return serve_landing_page(
                request,
                &conf.landing_dir,
                &health_service_id,
                conf.keep_alive_secs,
            );
        };

        ensure_service_health_monitor(
            &health_service_id,
            conf.check_mode,
            target_port,
            &conf.check_endpoint,
            conf.check_timeout_ms,
            conf.up_check_interval_ms,
            conf.starting_check_interval_ms,
            conf.down_check_interval_ms,
            if conf.eta_enabled.unwrap_or(true) {
                conf.history_file.as_deref()
            } else {
                None
            },
            conf.history_samples_count,
            conf.history_percentile,
        );

        // Request routing must stay fast and non-blocking: use only cached state here.
        // Do not add synchronous health checks on this path.
        let is_up = is_service_up(&health_service_id);
        ngx_log_debug_http!(
            request,
            "hibernator enabled=1 target_port={} up={}",
            target_port,
            is_up
        );

        if !is_up {
            if let Some(service_name) = conf.service_name.as_deref() {
                initiate_service_start(service_name.to_owned());
                ngx_log_debug_http!(
                    request,
                    "hibernator start scheduled service={}",
                    service_name
                );
            }
            serve_landing_page(
                request,
                &conf.landing_dir,
                &health_service_id,
                conf.keep_alive_secs,
            )
        } else {
            Status::NGX_DECLINED
        }
    }
}

extern "C" fn hibernator_access_handler(r: *mut ngx_http_request_t) -> ngx_int_t {
    let request = unsafe { Request::from_ngx_http_request(r) };
    HibernatorRequestHandler::handler(request).into()
}

unsafe fn register_access_handler(cf: *mut ngx_conf_t) -> Result<(), ()> {
    let cf_ref = unsafe { &mut *cf };
    let conf_ctx = cf_ref.ctx.cast::<ngx_http_conf_ctx_t>();
    if conf_ctx.is_null() {
        return Err(());
    }

    let conf_ctx = unsafe { &mut *conf_ctx };
    let core_module = unsafe { &*::core::ptr::addr_of!(ngx_http_core_module) };
    let cmcf_ptr = unsafe { *conf_ctx.main_conf.add(core_module.ctx_index) }
        .cast::<ngx_http_core_main_conf_t>();
    if cmcf_ptr.is_null() {
        return Err(());
    }

    let cmcf = unsafe { &mut *cmcf_ptr };
    let handlers = &mut cmcf.phases[ngx_http_phases_NGX_HTTP_ACCESS_PHASE as usize].handlers;
    let h = unsafe { ngx_array_push(handlers).cast::<ngx_http_handler_pt>() };
    if h.is_null() {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "failed to register hibernator access handler"
        );
        return Err(());
    }

    unsafe {
        *h = Some(hibernator_access_handler);
    }
    Ok(())
}
