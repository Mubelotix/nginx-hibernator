use core::ptr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use ngx::core::{Buffer, Status};
use ngx::ffi::{ngx_chain_t, ngx_http_request_t, ngx_int_t};
use ngx::http::{self, Request};
use crate::state::{now_ms, runtime_for};

unsafe extern "C" {
    fn ngx_http_finalize_request(r: *mut ngx_http_request_t, rc: ngx_int_t);
}

const DEFAULT_LANDING_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Service starting</title><style>body{font-family:system-ui,Segoe UI,sans-serif;margin:40px;color:#222}main{max-width:680px}h1{font-size:1.6rem;margin-bottom:.4rem}p{line-height:1.45}</style></head><body><main><h1>Service is waking up</h1><p>The upstream service is currently hibernated and is being started.</p><p>Please refresh in a few seconds.</p><p id=\"eta\"></p></main><script>var eta=parseInt('{{ETA_SECONDS}}',10);if(eta>0){var el=document.getElementById('eta');var update=function(){el.innerText='Estimated time left: '+eta+'s';if(eta<=0){location.reload();}eta--;};update();setInterval(update,1000);}</script></body></html>";
const LANDING_PREFIX: &str = "/hibernator-landing/";

pub fn serve_landing_page(
    request: &mut Request,
    landing_dir: Option<&str>,
    service_id: &str,
    keep_alive_secs: u64,
) -> Status {
    let runtime = runtime_for(service_id);
    let start = runtime.startup_start_time_ms.load(std::sync::atomic::Ordering::Relaxed);
    let expected = runtime.expected_startup_duration_ms.load(std::sync::atomic::Ordering::Relaxed);
    let now = now_ms();

    let elapsed = if start > 0 { now.saturating_sub(start) } else { 0 };

    if let Some(dir) = landing_dir {
        if let Some((body, content_type)) = read_landing_index(dir) {
            let body = apply_eta(body, content_type, elapsed, expected, keep_alive_secs);
            return send_page_response(request, &body, content_type, http::HTTPStatus::SERVICE_UNAVAILABLE);
        }
    }

    let body = apply_eta(DEFAULT_LANDING_HTML.as_bytes().to_vec(), "text/html; charset=utf-8", elapsed, expected, keep_alive_secs);
    send_page_response(
        request,
        &body,
        "text/html; charset=utf-8",
        http::HTTPStatus::SERVICE_UNAVAILABLE,
    )
}

fn apply_eta(
    body: Vec<u8>,
    content_type: &str,
    elapsed_ms: u64,
    expected_ms: u64,
    keep_alive_secs: u64,
) -> Vec<u8> {
    if content_type.starts_with("text/html") {
        if let Ok(mut body_str) = String::from_utf8(body.clone()) {
            // Support modern Lottie-based template tags
            body_str = body_str.replace("KEEP_ALIVE", &keep_alive_secs.to_string());
            body_str = body_str.replace("DONE_MS", &elapsed_ms.to_string());
            body_str = body_str.replace("DURATION_MS", &expected_ms.to_string());

            // Support legacy placeholder {{ETA_SECONDS}}
            let remaining_secs = expected_ms.saturating_sub(elapsed_ms) / 1000;
            body_str = body_str.replace("{{ETA_SECONDS}}", &remaining_secs.to_string());

            return body_str.into_bytes();
        }
    }
    body
}

pub fn serve_landing_prefixed_asset(request: &mut Request, landing_dir: Option<&str>) -> Status {
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

pub fn is_landing_prefixed_uri(uri: &str) -> bool {
    uri == "/hibernator-landing" || uri.starts_with(LANDING_PREFIX)
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
