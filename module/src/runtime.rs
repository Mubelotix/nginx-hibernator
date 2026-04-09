use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::task::JoinHandle;

static ASYNC_RUNTIME_STARTED: AtomicBool = AtomicBool::new(false);
static ASYNC_RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();

pub fn spawn_future_on_runtime<F>(future: F) -> Option<JoinHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime_handle().map(|handle| handle.spawn(future))
}

fn runtime_handle() -> Option<&'static Handle> {
    start_async_runtime_if_needed();
    ASYNC_RUNTIME_HANDLE.get()
}

fn start_async_runtime_if_needed() {
    if ASYNC_RUNTIME_STARTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        let (handle_tx, handle_rx) = mpsc::channel::<Handle>();
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build hibernator async runtime");

            let handle = runtime.handle().clone();
            let _ = handle_tx.send(handle);

            runtime.block_on(async {
                std::future::pending::<()>().await;
            });
        });

        if let Ok(handle) = handle_rx.recv() {
            let _ = ASYNC_RUNTIME_HANDLE.set(handle);
        }
    } else {
        for _ in 0..20 {
            if ASYNC_RUNTIME_HANDLE.get().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}