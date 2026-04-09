use tokio::sync::{mpsc::{self, UnboundedSender as Sender}, oneshot::{Sender as OneShotSender, Receiver as OneShotReceiver, channel as oneshot_channel}};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;
use dbus_tokio::connection;
use dbus::nonblock::{self, Proxy, SyncConnection};

use crate::check;
use crate::runtime::spawn_future_on_runtime;
use dbus::blocking::Connection;
static CONTROLLER_TX: OnceLock<Sender<ControllerCommand>> = OnceLock::new();

enum ControllerAction {
    Start,
    Stop,
}

struct ControllerCommand {
    action: ControllerAction,
    service_name: String,
    reply_tx: OneShotSender<bool>,
}

pub fn init_process() {
    let _ = controller_tx();
}

fn controller_tx() -> &'static Sender<ControllerCommand> {
    log!("A");
    let r = CONTROLLER_TX.get_or_init(|| {
        let (resource, conn) = connection::new_system_sync()
            .expect("hibernator: failed to connect to D-Bus system bus");
        let _handle = spawn_future_on_runtime(async move {
            let err = resource.await;
            panic!("Lost connection to D-Bus: {}", err);
        });

        let (tx, mut rx) = mpsc::unbounded_channel::<ControllerCommand>();
        spawn_future_on_runtime(async move {
            crate::log!("hibernator: internal controller thread started");

            while let Some(cmd) = rx.recv().await {
                let conn2 = Arc::clone(&conn);
                let ok = match cmd.action {
                    ControllerAction::Start => run_service_action(ControllerAction::Start, &cmd.service_name, conn2).await,
                    ControllerAction::Stop => run_service_action(ControllerAction::Stop, &cmd.service_name, conn2).await,
                };
                let _ = cmd.reply_tx.send(ok);
            }
        });
        tx
    });
    log!("B");
    r
}

async fn request_service_action(action: ControllerAction, service_name: &str) -> bool {
    let (reply_tx, reply_rx) = oneshot_channel();
    let cmd = ControllerCommand {
        action,
        service_name: service_name.to_owned(),
        reply_tx,
    };

    if controller_tx().send(cmd).is_err() {
        crate::elog!("hibernator: failed to send command to internal controller");
        return false;
    }

    reply_rx.await.unwrap_or(false)
}

async fn run_service_action(action: ControllerAction, service_name: &str, conn: Arc<SyncConnection>) -> bool {
    let unit_name = if service_name.ends_with('.') || service_name.contains('.') {
        service_name.to_owned()
    } else {
        format!("{service_name}.service")
    };

    let proxy = Proxy::new(
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        Duration::from_secs(30),
        conn
    );

    let result = match action {
        ControllerAction::Start => {
            proxy.method_call::<(dbus::Path<'static>,), _, _, _>(
                "org.freedesktop.systemd1.Manager",
                "StartUnit",
                (unit_name.as_str(), "replace"),
            ).await
        }
        ControllerAction::Stop => {
            proxy.method_call::<(dbus::Path<'static>,), _, _, _>(
                "org.freedesktop.systemd1.Manager",
                "StopUnit",
                (unit_name.as_str(), "replace"),
            ).await
        }
    };

    if let Err(e) = result {
        let op = match action {
            ControllerAction::Start => "start",
            ControllerAction::Stop => "stop",
        };
        crate::elog!("hibernator: failed to {} service {} via dbus: {}", op, service_name, e);
        return false;
    }

    true
}

pub fn initiate_service_stop(service_name: String) {
    spawn_future_on_runtime(async move {
        let stopped = request_service_action(ControllerAction::Stop, &service_name).await;
        if !stopped {
            elog!("hibernator: failed to stop service {}", service_name);
        }
    });
}

pub fn initiate_service_start(service_name: String) {
    if !check::try_mark_service_starting(&service_name) {
        return;
    }

    spawn_future_on_runtime(async move {
        let started = request_service_action(ControllerAction::Start, &service_name).await;
        if !started {
            elog!("hibernator: failed to start service {}", service_name);
        }
    });
}
