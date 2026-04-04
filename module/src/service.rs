use std::sync::mpsc::{self, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::check;
use crate::hibernate;
use dbus::blocking::Connection;
use ngx::ffi::{NGX_LOG_ERR, NGX_LOG_NOTICE};
use ngx::ngx_log_error;

use crate::config::ServiceCheckMode;

macro_rules! log {
    ($($arg:tt)+) => {
        ngx_log_error!(NGX_LOG_NOTICE, ngx::log::ngx_cycle_log().as_ptr(), $($arg)+);
    };
}

macro_rules! elog {
    ($($arg:tt)+) => {
        ngx_log_error!(NGX_LOG_ERR, ngx::log::ngx_cycle_log().as_ptr(), $($arg)+);
    };
}

struct ServiceStartRuntime {
    starting: AtomicBool,
}

impl ServiceStartRuntime {
    fn new() -> Self {
        Self {
            starting: AtomicBool::new(false),
        }
    }
}

static SERVICE_START_RUNTIMES: OnceLock<Mutex<std::collections::HashMap<String, Arc<ServiceStartRuntime>>>> = OnceLock::new();
static CONTROLLER_TX: OnceLock<Sender<ControllerCommand>> = OnceLock::new();

enum ControllerAction {
    Start,
    Stop,
}

struct ControllerCommand {
    action: ControllerAction,
    service_name: String,
    reply_tx: Sender<bool>,
}

fn start_runtimes() -> &'static Mutex<std::collections::HashMap<String, Arc<ServiceStartRuntime>>> {
    SERVICE_START_RUNTIMES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub fn init_process() {
    let _ = controller_tx();
}

fn controller_tx() -> &'static Sender<ControllerCommand> {
    CONTROLLER_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ControllerCommand>();
        thread::spawn(move || {
            log!("hibernator: internal controller thread started");
            while let Ok(cmd) = rx.recv() {
                let ok = match cmd.action {
                    ControllerAction::Start => run_service_action(ControllerAction::Start, &cmd.service_name),
                    ControllerAction::Stop => run_service_action(ControllerAction::Stop, &cmd.service_name),
                };
                let _ = cmd.reply_tx.send(ok);
            }
        });
        tx
    })
}

fn request_service_action(action: ControllerAction, service_name: &str) -> bool {
    let (reply_tx, reply_rx) = mpsc::channel::<bool>();
    let cmd = ControllerCommand {
        action,
        service_name: service_name.to_owned(),
        reply_tx,
    };

    if controller_tx().send(cmd).is_err() {
        elog!("hibernator: failed to send command to internal controller");
        return false;
    }

    reply_rx.recv().unwrap_or(false)
}

fn start_runtime_for(service_name: &str) -> Arc<ServiceStartRuntime> {
    let mut map = start_runtimes().lock().expect("service start runtime lock poisoned");
    if let Some(existing) = map.get(service_name) {
        return Arc::clone(existing);
    }

    let runtime = Arc::new(ServiceStartRuntime::new());
    map.insert(service_name.to_owned(), Arc::clone(&runtime));
    runtime
}

fn run_service_action(action: ControllerAction, service_name: &str) -> bool {
    let unit_name = if service_name.ends_with('.') || service_name.contains('.') {
        service_name.to_owned()
    } else {
        format!("{service_name}.service")
    };

    let Ok(conn) = Connection::new_system() else {
        elog!("hibernator: failed to connect to system bus");
        return false;
    };

    let proxy = conn.with_proxy(
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        Duration::from_secs(5),
    );

    let result = match action {
        ControllerAction::Start => {
            proxy.method_call::<(dbus::Path<'static>,), _, _, _>(
                "org.freedesktop.systemd1.Manager",
                "StartUnit",
                (unit_name.as_str(), "replace"),
            )
        }
        ControllerAction::Stop => {
            proxy.method_call::<(dbus::Path<'static>,), _, _, _>(
                "org.freedesktop.systemd1.Manager",
                "StopUnit",
                (unit_name.as_str(), "replace"),
            )
        }
    };

    if let Err(e) = result {
        let op = match action {
            ControllerAction::Start => "start",
            ControllerAction::Stop => "stop",
        };
        elog!("hibernator: failed to {} service {} via dbus: {}", op, service_name, e);
        return false;
    }

    true
}

pub fn stop_service(service_name: &str) -> bool {
    request_service_action(ControllerAction::Stop, service_name)
}

pub fn start_service_async(
    service_name: &str,
    target_port: u16,
    check_mode: ServiceCheckMode,
    ready_endpoint: &str,
    ready_timeout_ms: u64,
    timeout_ms: u64,
    check_interval_ms: u64,
) {
    let service_name_owned = service_name.to_owned();
    let ready_endpoint_owned = ready_endpoint.to_owned();
    let rt = start_runtime_for(&service_name_owned);

    if rt
        .starting
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    thread::spawn(move || {
        log!(
            "hibernator: scheduling async start for service {}",
            service_name_owned
        );
        let started = start_service_and_wait_ready_inner(
            &service_name_owned,
            target_port,
            check_mode,
            &ready_endpoint_owned,
            ready_timeout_ms,
            timeout_ms,
            check_interval_ms,
        );

        if started {
            check::set_service_up(&service_name_owned, true);
            hibernate::mark_service_started(&service_name_owned);
        }

        let runtime = start_runtime_for(&service_name_owned);
        runtime.starting.store(false, Ordering::Release);
    });
}

fn start_service_and_wait_ready_inner(
    service_name: &str,
    target_port: u16,
    check_mode: ServiceCheckMode,
    ready_endpoint: &str,
    ready_timeout_ms: u64,
    timeout_ms: u64,
    check_interval_ms: u64,
) -> bool {
    log!("hibernator: starting service {}", service_name);
    if !request_service_action(ControllerAction::Start, service_name) {
        elog!("hibernator: failed to start service {}", service_name);
        return false;
    }

    let interval = check_interval_ms.max(1);
    let max_checks = timeout_ms.saturating_div(interval).max(1);

    for _ in 0..max_checks {
        if check::is_service_up(check_mode, target_port, ready_endpoint, ready_timeout_ms) {
            hibernate::mark_service_started(service_name);
            log!("hibernator: service {} is ready", service_name);
            return true;
        }
        thread::sleep(Duration::from_millis(interval));
    }

    elog!("hibernator: service {} did not become ready in time", service_name);
    false
}
