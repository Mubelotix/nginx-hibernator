use core::ffi::{c_char, c_void};
use core::ptr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ngx::core::{Buffer, Status};
use ngx::ffi::{
    NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_HTTP_MODULE, NGX_LOG_EMERG,
    ngx_array_push, ngx_command_t, ngx_conf_t, ngx_http_conf_ctx_t, ngx_http_core_main_conf_t,
    ngx_http_core_module, ngx_http_handler_pt, ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE,
    ngx_http_request_t, ngx_int_t, ngx_module_t, ngx_str_t, ngx_uint_t, ngx_chain_t,
};
use ngx::http::{self, HttpModule, HttpModuleLocationConf, MergeConfigError, Request};
use ngx::{ngx_conf_log_error, ngx_log_debug_http, ngx_string};

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

#[derive(Debug, Default)]
struct ModuleConfig {
    enable: bool,
    landing_dir: Option<String>,
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static mut NGX_HTTP_RANDOM_GATE_COMMANDS: [ngx_command_t; 3] = [
    ngx_command_t {
        name: ngx_string!("random_gate"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_random_gate_commands_set_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("random_gate_landing_dir"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_random_gate_commands_set_landing_dir),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

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
    commands: unsafe { &raw mut NGX_HTTP_RANDOM_GATE_COMMANDS[0] },
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

impl http::Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if prev.enable {
            self.enable = true;
        }
        if self.landing_dir.is_none() {
            self.landing_dir = prev.landing_dir.clone();
        }
        Ok(())
    }
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

        let fail = should_fail_now();
        ngx_log_debug_http!(request, "random_gate enabled=1 fail_window={}", fail);

        if fail {
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

fn should_fail_now() -> bool {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u64, |d| d.as_secs());
    let window = (now_secs / 10) % 2;
    window == 0
}

extern "C" fn ngx_http_random_gate_commands_set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();

        let val = match args[1].to_str() {
            Ok(s) => s,
            Err(_) => {
                ngx_conf_log_error!(
                    NGX_LOG_EMERG,
                    cf,
                    "`random_gate` argument is not utf-8 encoded"
                );
                return ngx::core::NGX_CONF_ERROR;
            }
        };

        conf.enable = false;

        if val.len() == 2 && val.eq_ignore_ascii_case("on") {
            conf.enable = true;
        } else if val.len() == 3 && val.eq_ignore_ascii_case("off") {
            conf.enable = false;
        } else {
            ngx_conf_log_error!(
                NGX_LOG_EMERG,
                cf,
                "invalid value for `random_gate`: use `on` or `off`"
            );
            return ngx::core::NGX_CONF_ERROR;
        }
    }

    ngx::core::NGX_CONF_OK
}

extern "C" fn ngx_http_random_gate_commands_set_landing_dir(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();

        let val = match args[1].to_str() {
            Ok(s) => s,
            Err(_) => {
                ngx_conf_log_error!(
                    NGX_LOG_EMERG,
                    cf,
                    "`random_gate_landing_dir` argument is not utf-8 encoded"
                );
                return ngx::core::NGX_CONF_ERROR;
            }
        };

        if val.is_empty() {
            ngx_conf_log_error!(
                NGX_LOG_EMERG,
                cf,
                "invalid value for `random_gate_landing_dir`: path cannot be empty"
            );
            return ngx::core::NGX_CONF_ERROR;
        }

        conf.landing_dir = Some(val.to_owned());
    }

    ngx::core::NGX_CONF_OK
}
