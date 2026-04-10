use std::future::Future;
use std::sync::mpsc;
use std::sync::LazyLock;
use std::thread;

use tokio::runtime::{Builder, Handle};
use tokio::task::JoinHandle;

static ASYNC_RUNTIME_HANDLE: LazyLock<Handle> = LazyLock::new(|| {
    let (handle_tx, handle_rx) = mpsc::channel::<Handle>();
    thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build hibernator async runtime");

        let handle = runtime.handle().clone();
        let _ = handle_tx.send(handle);

        runtime.block_on(async {
            std::future::pending::<()>().await;
        });
    });

    handle_rx
        .recv()
        .expect("failed to receive handle from hibernator async runtime")
});

pub fn spawn_future_on_runtime<F>(future: F) -> Option<JoinHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Some(ASYNC_RUNTIME_HANDLE.spawn(future))
}