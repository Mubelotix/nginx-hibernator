use core::ptr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use ngx::core::{Buffer, Status};
use ngx::ffi::{
    NGX_HTTP_MODULE, NGX_LOG_EMERG,
    ngx_array_push, ngx_conf_t, ngx_http_conf_ctx_t, ngx_http_core_main_conf_t,
    ngx_http_core_module, ngx_http_handler_pt, ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE,
    ngx_http_request_t, ngx_int_t, ngx_module_t, ngx_chain_t, ngx_cycle_t,
};
use ngx::http::{self, HttpModule, HttpModuleLocationConf, Request};
use ngx::{ngx_conf_log_error, ngx_log_debug_http};

unsafe extern "C" {
    fn ngx_http_finalize_request(r: *mut ngx_http_request_t, rc: ngx_int_t);
}

mod config;
mod check;
mod hibernate;
mod service;
use config::ModuleConfig;

const DEFAULT_LANDING_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Service starting</title><style>body{font-family:system-ui,Segoe UI,sans-serif;margin:40px;color:#222}main{max-width:680px}h1{font-size:1.6rem;margin-bottom:.4rem}p{line-height:1.45}</style></head><body><main><h1>Service is waking up</h1><p>The upstream service is currently hibernated and is being started.</p><p>Please refresh in a few seconds.</p></main></body></html>";
const LANDING_PREFIX: &str = "/hibernator-landing/";

struct Module;

impl http::HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_random_gate_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        if register_access_handler(cf).is_ok() {
            Status::NGX_OK.into()
        } else {
            Status::NGX_ERROR.into()
        }
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static NGX_HTTP_RANDOM_GATE_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
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
ngx::ngx_modules!(ngx_http_random_gate_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_random_gate_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_RANDOM_GATE_MODULE_CTX as _,
    commands: unsafe { &raw mut config::NGX_HTTP_HIBERNATOR_COMMANDS[0] },
    type_: NGX_HTTP_MODULE as _,
    init_process: Some(random_gate_init_process),
    ..ngx_module_t::default()
};

unsafe extern "C" fn random_gate_init_process(_cycle: *mut ngx_cycle_t) -> ngx_int_t {
    service::init_process();
    check::start_registered_service_health_monitors();
    Status::NGX_OK.into()
}

struct RandomGateRequestHandler;

impl RandomGateRequestHandler {
    fn handler(request: &mut http::Request) -> Status {
        let Some(conf) = Module::location_conf(request) else {
            return Status::NGX_ERROR;
        };

        if !conf.enable {
            return Status::NGX_DECLINED;
        }

        if let Ok(uri) = request.path().to_str() {
            if is_landing_prefixed_uri(uri) {
                return serve_landing_prefixed_asset(request, conf.landing_dir.as_deref());
            }
        }

        if let Some(service_name) = conf.service_name.as_deref() {
            hibernate::touch_activity(service_name, conf.keep_alive_secs);
        }

        let Some(target_port) = conf.target_port else {
            ngx_log_debug_http!(request, "hibernator enabled=1 missing target_port, serving landing");
            return serve_landing_page(request, conf.landing_dir.as_deref());
        };

        let health_service_id = conf
            .service_name
            .clone()
            .unwrap_or_else(|| format!("{}:{}", target_port, conf.check_endpoint));

        check::ensure_service_health_monitor(
            &health_service_id,
            conf.check_mode,
            target_port,
            &conf.check_endpoint,
            conf.check_timeout_ms,
            conf.up_check_interval_ms,
            conf.down_check_interval_ms,
        );

        // Request routing must stay fast and non-blocking: use only cached state here.
        // Do not add synchronous health checks on this path.
        let is_up = check::is_service_up_cached(&health_service_id);
        ngx_log_debug_http!(
            request,
            "hibernator enabled=1 target_port={} up={}",
            target_port,
            is_up
        );

        if !is_up {
            if let Some(service_name) = conf.service_name.as_deref() {
                service::start_service_async(
                    service_name,
                    target_port,
                    conf.check_mode,
                    &conf.check_endpoint,
                    conf.check_timeout_ms,
                    conf.start_timeout_ms,
                    conf.start_check_interval_ms,
                );
                ngx_log_debug_http!(
                    request,
                    "hibernator start scheduled service={}",
                    service_name
                );
            }
            serve_landing_page(request, conf.landing_dir.as_deref())
        } else {
            Status::NGX_DECLINED
        }
    }
}

fn serve_landing_page(request: &mut Request, landing_dir: Option<&str>) -> Status {
    if let Some(dir) = landing_dir {
        if let Some((body, content_type)) = read_landing_index(dir) {
            return send_page_response(request, &body, content_type, http::HTTPStatus::SERVICE_UNAVAILABLE);
        }
    }

    send_page_response(
        request,
        DEFAULT_LANDING_HTML.as_bytes(),
        "text/html; charset=utf-8",
        http::HTTPStatus::SERVICE_UNAVAILABLE,
    )
}

fn serve_landing_prefixed_asset(request: &mut Request, landing_dir: Option<&str>) -> Status {
    let Some(dir) = landing_dir else {
        return http::HTTPStatus::NOT_FOUND.into();
    };

    let Ok(uri) = request.path().to_str() else {
        return http::HTTPStatus::NOT_FOUND.into();
    };

    let Some(rel_path) = landing_rel_path_from_uri(uri) else {
        return http::HTTPStatus::NOT_FOUND.into();
    };

    let Some((body, content_type)) = read_landing_asset_by_rel_path(dir, rel_path) else {
        return http::HTTPStatus::NOT_FOUND.into();
    };

    send_page_response(request, &body, content_type, http::HTTPStatus::OK)
}

fn send_page_response(
    request: &mut Request,
    body: &[u8],
    content_type: &str,
    status: http::HTTPStatus,
) -> Status {
    let rc = request.discard_request_body();
    if rc != Status::NGX_OK {
        return rc;
    }

    let pool = request.pool();
    let Some(mut buf) = pool.create_buffer(body.len()) else {
        return Status::NGX_ERROR;
    };

    unsafe {
        let ngx_buf = buf.as_ngx_buf_mut();
        ptr::copy_nonoverlapping(body.as_ptr(), (*ngx_buf).pos, body.len());
        (*ngx_buf).last = (*ngx_buf).pos.add(body.len());
    }

    buf.set_last_buf(true);
    buf.set_last_in_chain(true);

    let chain = pool.calloc_type::<ngx_chain_t>();
    if chain.is_null() {
        return Status::NGX_ERROR;
    }

    unsafe {
        (*chain).buf = buf.as_ngx_buf_mut();
        (*chain).next = ptr::null_mut();
    }

    request.set_status(status);
    let _ = request.add_header_out("Content-Type", content_type);
    request.set_content_length_n(body.len());

    let header_status = request.send_header();
    if header_status != Status::NGX_OK {
        return header_status;
    }
    if request.header_only() {
        return Status::NGX_DONE;
    }

    let body_status = unsafe { request.output_filter(&mut *chain) };
    unsafe {
        let request_ptr: *mut ngx_http_request_t = request.into();
        ngx_http_finalize_request(request_ptr, body_status.0);
    }

    if body_status == Status::NGX_OK {
        Status::NGX_DONE
    } else {
        body_status
    }
}

fn read_landing_index(landing_dir: &str) -> Option<(Vec<u8>, &'static str)> {
    read_landing_asset_by_rel_path(landing_dir, "index.html")
}

fn read_landing_asset_by_rel_path(landing_dir: &str, rel_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let path = resolve_landing_path(landing_dir, rel_path)?;
    let bytes = fs::read(&path).ok()?;
    let content_type = content_type_for_path(&path);
    Some((bytes, content_type))
}

fn is_landing_prefixed_uri(uri: &str) -> bool {
    uri == "/hibernator-landing" || uri.starts_with(LANDING_PREFIX)
}

fn landing_rel_path_from_uri(uri: &str) -> Option<&str> {
    if uri == "/hibernator-landing" {
        return Some("index.html");
    }

    let rel = uri.strip_prefix(LANDING_PREFIX)?;
    if rel.is_empty() {
        Some("index.html")
    } else {
        Some(rel)
    }
}

fn resolve_landing_path(landing_dir: &str, rel_path: &str) -> Option<PathBuf> {
    let base = Path::new(landing_dir);
    if !base.is_dir() {
        return None;
    }

    let mut rel = PathBuf::new();
    for comp in Path::new(rel_path).components() {
        if let Component::Normal(part) = comp {
            rel.push(part);
        }
    }

    if rel.as_os_str().is_empty() {
        return None;
    }

    let candidate = base.join(&rel);
    if candidate.is_file() {
        return Some(candidate);
    }

    None
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

extern "C" fn random_gate_access_handler(r: *mut ngx_http_request_t) -> ngx_int_t {
    let request = unsafe { Request::from_ngx_http_request(r) };
    RandomGateRequestHandler::handler(request).into()
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
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "failed to register random_gate access handler");
        return Err(());
    }

    unsafe {
        *h = Some(random_gate_access_handler);
    }
    Ok(())
}
