use crate::prelude::*;
use std::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use core::mem;
use ngx::ffi::{
    ngx_add_timer, ngx_connection_t, ngx_del_timer, ngx_delete_posted_event, ngx_event_t,
    ngx_module_t, ngx_post_event, ngx_posted_events, ngx_posted_next_events,
};
use tokio::task::JoinHandle;

#[repr(C)]
pub struct AsyncCTX<T> {
    pub done: Arc<AtomicBool>,
    pub event: ngx_event_t,
    pub task: Option<JoinHandle<()>>,
    pub result: Arc<Mutex<Option<T>>>,
}

impl<T> Default for AsyncCTX<T> {
    fn default() -> Self {
        Self {
            done: Arc::new(AtomicBool::new(false)),
            event: unsafe { mem::zeroed() },
            task: None,
            result: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T> Drop for AsyncCTX<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.task.take() {
            handle.abort();
        }

        if self.event.posted() != 0 {
            unsafe { ngx_delete_posted_event(&raw mut self.event) };
        }

        if self.event.timer_set() != 0 {
            unsafe { ngx_del_timer(&raw mut self.event) };
        }
    }
}

/// Polling handler for asynchronous work.
pub unsafe extern "C" fn check_async_work_done<T>(event: *mut ngx_event_t) {
    let ctx_ptr = ngx::ngx_container_of!(event, AsyncCTX<T>, event);
    let ctx = unsafe { &*ctx_ptr };
    let c: *mut ngx_connection_t = unsafe { (*event).data.cast() };

    if ctx.done.load(Ordering::Relaxed) {
        // Work is completed! Trigger the Nginx request handler by posting to the write event.
        unsafe { ngx_post_event((*c).write, &raw mut ngx_posted_events) };
    } else {
        // Still waiting for completion. 
        // We use a small timer (10ms) to poll without hogging the CPU.
        // Real-time wakeup is currently not possible without ngx_notify (requires Nginx --with-threads)
        // or a custom self-pipe registered via low-level ngx_add_event (not exposed in current bindings).
        unsafe { ngx_add_timer(event, 10) };
    }
}

pub fn perform_async<T, F>(
    request: &mut Request,
    module: &'static ngx_module_t,
    future_factory: impl FnOnce() -> F + Send + 'static,
) -> Option<T>
where
    T: Send + 'static,
    F: std::future::Future<Output = T> + Send + 'static,
{
    if let Some(ctx) = request.get_module_ctx::<AsyncCTX<T>>(module) {
        if !ctx.done.load(Ordering::Relaxed) {
            return None;
        }

        return ctx.result.lock().unwrap().take();
    }

    let ctx = request.pool().allocate(AsyncCTX::<T>::default());
    if ctx.is_null() {
        return None;
    }
    request.set_module_ctx(ctx.cast(), module);

    let ctx = unsafe { &mut *ctx };
    ctx.event.handler = Some(check_async_work_done::<T>);
    ctx.event.data = request.connection().cast();
    ctx.event.log = unsafe { (*request.connection()).log };
    
    // First check is scheduled immediately in the main thread.
    unsafe { ngx_post_event(&raw mut ctx.event, &raw mut ngx_posted_next_events) };

    let done_flag = ctx.done.clone();
    let result_store = ctx.result.clone();

    ctx.task = crate::runtime::spawn_future_on_runtime(async move {
        let res = future_factory().await;
        *result_store.lock().unwrap() = Some(res);
        done_flag.store(true, Ordering::Release);
    });

    None
}
