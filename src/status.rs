use ngx::core::Status;
use ngx::http::HTTPStatus;
use crate::prelude::*;

pub fn serve_status(request: &mut Request, service_id: &str, eta_enabled: bool) -> Status {
    let runtime = runtime_for(service_id);
    let state = runtime.state();
    let status = match state {
        ServiceHealthState::Unknown => "unknown",
        ServiceHealthState::Up => "up",
        ServiceHealthState::Down => "down",
        ServiceHealthState::Starting => "starting",
    };
    let start = runtime.shared.load_startup_start_time_ms();
    let expected = runtime.shared.load_expected_startup_duration_ms();
    let eta = match state {
        ServiceHealthState::Up => "0".to_owned(),
        ServiceHealthState::Starting if eta_enabled && start > 0 && expected > 0 => {
            expected.saturating_sub(now_ms().saturating_sub(start)).div_ceil(1000).to_string()
        }
        _ => "null".to_owned(),
    };
    let body = format!("{{\"status\":\"{status}\",\"eta_seconds\":{eta}}}\n");
    let _ = request.add_header_out("Cache-Control", "no-store");
    send_page_response(request, body.as_bytes(), "application/json", HTTPStatus::OK)
}
