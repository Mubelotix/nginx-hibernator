use core::ptr;
use std::path::{Component, Path, PathBuf};
use ngx::core::{Buffer, Status};
use ngx::http::HttpModule;
use ngx::ffi::{ngx_chain_t, ngx_http_request_t, ngx_int_t};
use crate::prelude::*;
use crate::Module;
use crate::nginx_async::perform_async;
use tokio::fs::read;
use ngx::http::HTTPStatus;

unsafe extern "C" {
    fn ngx_http_finalize_request(r: *mut ngx_http_request_t, rc: ngx_int_t);
}

const LANDING_PREFIX: &str = "/hibernator-landing/";
pub const DEFAULT_LANDING_DIR: &str = "/usr/share/nginx-hibernator/landing";

pub fn serve_landing_page(
    request: &mut Request,
    landing_dir: &str,
    service_id: &str,
    keep_alive_secs: u64,
) -> Status {
    let runtime = runtime_for(service_id);
    let start = runtime.shared.load_startup_start_time_ms();
    let expected = runtime.shared.load_expected_startup_duration_ms();
    let now = now_ms();

    let elapsed = if start > 0 { now.saturating_sub(start) } else { 0 };

    let dir = landing_dir.to_owned();
    let result = perform_async(request, Module::module(), || async move {
        read_landing_asset_by_rel_path_async(&dir, "index.html").await
    });

    let Some(result) = result else {
        return Status::NGX_AGAIN;
    };

    let (body, content_type) = result.unwrap_or_else(|| {
        (b"Service is starting, please retry in a moment.\n".to_vec(), "text/plain")
    });

    let body = apply_eta(body, content_type, elapsed, expected, keep_alive_secs);
    send_page_response(request, &body, content_type, HTTPStatus::SERVICE_UNAVAILABLE)
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

pub fn serve_landing_prefixed_asset(request: &mut Request, landing_dir: &str) -> Status {
    let Ok(uri) = request.path().to_str() else {
        return HTTPStatus::NOT_FOUND.into();
    };

    let Some(rel_path) = landing_rel_path_from_uri(uri) else {
        return HTTPStatus::NOT_FOUND.into();
    };

    let dir = landing_dir.to_owned();
    let rel_path = rel_path.to_owned();
    let result = perform_async(request, Module::module(), || async move {
        read_landing_asset_by_rel_path_async(&dir, &rel_path).await
    });

    let Some(result) = result else {
        return Status::NGX_AGAIN;
    };

    if let Some((body, content_type)) = result {
        send_page_response(request, &body, content_type, HTTPStatus::OK)
    } else {
        HTTPStatus::NOT_FOUND.into()
    }
}

pub fn is_landing_prefixed_uri(uri: &str) -> bool {
    uri == "/hibernator-landing" || uri.starts_with(LANDING_PREFIX)
}

fn send_page_response(
    request: &mut Request,
    body: &[u8],
    content_type: &str,
    status: HTTPStatus,
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

async fn read_landing_asset_by_rel_path_async(
    landing_dir: &str,
    rel_path: &str,
) -> Option<(Vec<u8>, &'static str)> {
    let path = resolve_landing_path(landing_dir, rel_path)?;
    let bytes = read(&path).await.ok()?;
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
