use dbus::nonblock::{Proxy, SyncConnection};
use dbus_tokio::connection;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::{
    mpsc::{self, UnboundedSender as Sender},
    oneshot::{channel as oneshot_channel, Sender as OneShotSender},
};
use crate::prelude::*;

pub static CONTROLLER_TX: LazyLock<Sender<ControllerCommand>> = LazyLock::new(|| {
    let (resource, conn) = connection::new_system_sync()
        .expect("hibernator: failed to connect to D-Bus system bus");
    let _handle = spawn_future_on_runtime(async move {
        let err = resource.await;
        panic!("Lost connection to D-Bus: {}", err);
    });

    let (tx, mut rx) = mpsc::unbounded_channel::<ControllerCommand>();
    spawn_future_on_runtime(async move {
        log!("hibernator: internal controller thread started");

        while let Some(cmd) = rx.recv().await {
            let conn2 = Arc::clone(&conn);
            let ok = match cmd.action {
                ControllerAction::Start => {
                    run_service_action(ControllerAction::Start, &cmd.service_name, conn2).await
                }
                ControllerAction::Stop => {
                    run_service_action(ControllerAction::Stop, &cmd.service_name, conn2).await
                }
            };
            let _ = cmd.reply_tx.send(ok);
        }
    });
    tx
});

pub(crate) enum ControllerAction {
    Start,
    Stop,
}

pub struct ControllerCommand {
    action: ControllerAction,
    service_name: String,
    reply_tx: OneShotSender<bool>,
}

pub(crate) async fn request_service_action(action: ControllerAction, service_name: &str) -> bool {
    let (reply_tx, reply_rx) = oneshot_channel();
    let cmd = ControllerCommand {
        action,
        service_name: service_name.to_owned(),
        reply_tx,
    };

    if CONTROLLER_TX.send(cmd).is_err() {
        elog!("hibernator: failed to send command to internal controller");
        return false;
    }

    reply_rx.await.unwrap_or(false)
}

async fn run_service_action(
    action: ControllerAction,
    service_name: &str,
    conn: Arc<SyncConnection>,
) -> bool {
    let unit_name = if service_name.ends_with('.') || service_name.contains('.') {
        service_name.to_owned()
    } else {
        format!("{service_name}.service")
    };

    let proxy = Proxy::new(
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        Duration::from_secs(30),
        conn,
    );

    let result = match action {
        ControllerAction::Start => {
            proxy
                .method_call::<(dbus::Path<'static>,), _, _, _>(
                    "org.freedesktop.systemd1.Manager",
                    "StartUnit",
                    (unit_name.as_str(), "replace"),
                )
                .await
        }
        ControllerAction::Stop => {
            proxy
                .method_call::<(dbus::Path<'static>,), _, _, _>(
                    "org.freedesktop.systemd1.Manager",
                    "StopUnit",
                    (unit_name.as_str(), "replace"),
                )
                .await
        }
    };

    if let Err(e) = result {
        let op = match action {
            ControllerAction::Start => "start",
            ControllerAction::Stop => "stop",
        };
        elog!(
            "hibernator: failed to {} service {} via dbus: {}",
            op,
            service_name,
            e
        );
        return false;
    }

    true
}

pub fn initiate_service_start(service_name: String) {
    let runtime = runtime_for(&service_name);
    let target = ServiceHealthState::Starting.as_u8();
    let success = runtime
        .state
        .compare_exchange(
            ServiceHealthState::Unknown.as_u8(),
            target,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_ok()
        || runtime
            .state
            .compare_exchange(
                ServiceHealthState::Down.as_u8(),
                target,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok();

    if !success {
        return;
    }

    let now = now_ms();
    runtime.startup_start_time_ms.store(now, Ordering::Relaxed);
    runtime.state_change_notify.notify_waiters();

    spawn_future_on_runtime(async move {
        let started = request_service_action(ControllerAction::Start, &service_name).await;
        if !started {
            elog!("hibernator: failed to start service {}", service_name);
        }
    });
}
