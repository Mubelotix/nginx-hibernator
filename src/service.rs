use crate::prelude::*;
use dbus::nonblock::{Proxy, SyncConnection};
use dbus_tokio::connection;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::Mutex;

static SYSTEM_BUS_CONNECTION: LazyLock<Mutex<Option<Arc<SyncConnection>>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) enum ControllerAction {
    Start,
    Stop,
}

pub(crate) async fn request_service_action(action: ControllerAction, service_name: &str) -> bool {
    let Some(conn) = system_bus_connection().await else {
        return false;
    };

    run_service_action(action, service_name, conn).await
}

async fn system_bus_connection() -> Option<Arc<SyncConnection>> {
    let mut cached = SYSTEM_BUS_CONNECTION.lock().await;
    if let Some(conn) = cached.as_ref() {
        return Some(Arc::clone(conn));
    }

    let (resource, conn) = match connection::new_system_sync() {
        Ok(connection) => connection,
        Err(error) => {
            elog!("hibernator: failed to connect to D-Bus system bus: {error}");
            return None;
        }
    };

    let resource_conn = Arc::clone(&conn);
    *cached = Some(Arc::clone(&conn));
    if spawn_future_on_runtime(async move {
        let error = resource.await;
        elog!("hibernator: lost D-Bus connection: {error}");
        let mut cached = SYSTEM_BUS_CONNECTION.lock().await;
        if cached
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &resource_conn))
        {
            *cached = None;
        }
    })
    .is_none()
    {
        *cached = None;
        elog!("hibernator: failed to monitor D-Bus system bus connection");
        return None;
    }

    Some(conn)
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
    if !runtime.try_mark_starting() {
        return;
    }

    let now = now_ms();
    runtime.shared.store_startup_start_time_ms(now);
    runtime.state_change_notify.notify_waiters();

    spawn_future_on_runtime(async move {
        let started = request_service_action(ControllerAction::Start, &service_name).await;
        if !started {
            elog!("hibernator: failed to start service {}", service_name);
            runtime.set_state(ServiceHealthState::Down);
        }
    });
}
