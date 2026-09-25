use crate::landing::{read_landing_asset_by_rel_path_async, send_page_response};
use crate::nginx_async::perform_async;
use crate::prelude::*;
use crate::Module;
use ngx::core::Status;
use ngx::http::{HTTPStatus, HttpModule, Method};

pub fn should_serve_checkpoint(enabled: bool, state: ServiceHealthState, method: Method) -> bool {
    enabled && state != ServiceHealthState::Starting && method != Method::POST
}

pub fn serve_checkpoint_page(request: &mut Request, landing_dir: &str) -> Status {
    let dir = landing_dir.to_owned();
    let result = perform_async(request, Module::module(), || async move {
        read_landing_asset_by_rel_path_async(&dir, "checkpoint.html").await
    });

    let Some(result) = result else {
        return Status::NGX_AGAIN;
    };

    let (body, content_type) =
        result.unwrap_or_else(|| (b"Site is unavailable.\n".to_vec(), "text/plain"));
    send_page_response(
        request,
        &body,
        content_type,
        HTTPStatus::SERVICE_UNAVAILABLE,
    )
}
