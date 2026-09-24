use core::ptr::{self, NonNull};
use ngx::collections::RbTreeMap;
use ngx::core::{NgxString, SlabPool, Status};
use ngx::ffi::{ngx_conf_t, ngx_int_t, ngx_shm_zone_t, ngx_shared_memory_add};
use ngx::{allocator::allocate, ngx_string};
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicPtr, Ordering};
use ngx::sync::RwLock;

use crate::prelude::now_secs;
 
 #[repr(C)]
 pub struct SharedState {
     pub state: AtomicU8,
     pub expected_startup_duration_ms: AtomicU64,
     pub last_check_ms: AtomicU64,
     pub last_activity_secs: AtomicU64,
     pub startup_start_time_ms: AtomicU64,
 }
 
 impl SharedState {
     pub fn new() -> Self {
         Self {
             state: AtomicU8::new(super::ServiceHealthState::Unknown.as_u8()),
             expected_startup_duration_ms: AtomicU64::new(0),
             last_check_ms: AtomicU64::new(0),
             last_activity_secs: AtomicU64::new(now_secs()),
             startup_start_time_ms: AtomicU64::new(0),
         }
     }
 }

pub type SharedStateMap = RwLock<RbTreeMap<NgxString<SlabPool>, SharedState, SlabPool>>;

#[derive(Clone, Copy)]
pub struct SharedStateRef(NonNull<SharedState>);

unsafe impl Send for SharedStateRef {}
unsafe impl Sync for SharedStateRef {}

impl SharedStateRef {
    pub fn load_state(self) -> u8 {
        unsafe { self.0.as_ref().state.load(Ordering::Acquire) }
    }

    pub fn swap_state(self, value: u8) -> u8 {
        unsafe { self.0.as_ref().state.swap(value, Ordering::AcqRel) }
    }

    pub fn compare_exchange_state(self, current: u8, new: u8) -> bool {
        unsafe {
            self.0
                .as_ref()
                .state
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        }
    }

    pub fn load_expected_startup_duration_ms(self) -> u64 {
        unsafe { self.0.as_ref().expected_startup_duration_ms.load(Ordering::Acquire) }
    }

    pub fn store_expected_startup_duration_ms(self, value: u64) {
        unsafe { self.0.as_ref().expected_startup_duration_ms.store(value, Ordering::Release) }
    }

    pub fn load_last_check_ms(self) -> u64 {
        unsafe { self.0.as_ref().last_check_ms.load(Ordering::Acquire) }
    }

    pub fn compare_exchange_last_check_ms(self, current: u64, new: u64) -> bool {
        unsafe {
            self.0
                .as_ref()
                .last_check_ms
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        }
    }

    pub fn load_last_activity_secs(self) -> u64 {
        unsafe { self.0.as_ref().last_activity_secs.load(Ordering::Acquire) }
    }

    pub fn store_last_activity_secs(self, value: u64) {
        unsafe { self.0.as_ref().last_activity_secs.store(value, Ordering::Release) }
    }

    pub fn load_startup_start_time_ms(self) -> u64 {
        unsafe { self.0.as_ref().startup_start_time_ms.load(Ordering::Acquire) }
    }

    pub fn store_startup_start_time_ms(self, value: u64) {
        unsafe { self.0.as_ref().startup_start_time_ms.store(value, Ordering::Release) }
    }
}

static SHARED_STATE_ZONE: AtomicPtr<ngx_shm_zone_t> = AtomicPtr::new(ptr::null_mut());

pub fn ensure_shared_state_zone(cf: *mut ngx_conf_t) -> bool {
    let mut name = ngx_string!("hibernator_shared_state");
    let shm_zone = unsafe {
        ngx_shared_memory_add(
            cf,
            &raw mut name,
            128 * 1024,
            &raw const SHARED_STATE_ZONE as *const _ as *mut core::ffi::c_void,
        )
    };

    let Some(mut shm_zone) = NonNull::new(shm_zone) else {
        return false;
    };

    unsafe {
        shm_zone.as_mut().init = Some(shared_state_zone_init);
    }
    SHARED_STATE_ZONE.store(shm_zone.as_ptr(), Ordering::Release);
    true
}

pub fn shared_state_for(service_id: &str) -> Option<SharedStateRef> {
    let shm_zone = SHARED_STATE_ZONE.load(Ordering::Acquire);
    let mut shm_zone = NonNull::new(shm_zone)?;
    let shared = shared_state_map(unsafe { shm_zone.as_mut() }).ok()?;

    {
        let reader = shared.read();
        if let Some(state) = reader.get(service_id.as_bytes()) {
            return Some(SharedStateRef(NonNull::from(state)));
        }
    }

    let mut writer = shared.write();
    if let Some(state) = writer.get(service_id.as_bytes()) {
        return Some(SharedStateRef(NonNull::from(state)));
    }

    let key = NgxString::try_from_bytes_in(service_id.as_bytes(), writer.allocator().clone()).ok()?;
    if writer.try_insert(key, SharedState::new()).is_err() {
        return None;
    }

    writer
        .get(service_id.as_bytes())
        .map(|state| SharedStateRef(NonNull::from(state)))
}

pub fn shared_state_map(shm_zone: &mut ngx_shm_zone_t) -> Result<&'static SharedStateMap, Status> {
    let mut alloc = unsafe { SlabPool::from_shm_zone(shm_zone) }.ok_or(Status::NGX_ERROR)?;

    if alloc.as_mut().data.is_null() {
        let map: RbTreeMap<NgxString<SlabPool>, SharedState, SlabPool> =
            RbTreeMap::try_new_in(alloc.clone()).map_err(|_| Status::NGX_ERROR)?;
        let shared = RwLock::new(map);
        alloc.as_mut().data = allocate(shared, &alloc)
            .map_err(|_| Status::NGX_ERROR)?
            .as_ptr()
            .cast();
    }

    unsafe {
        alloc
            .as_ref()
            .data
            .cast::<SharedStateMap>()
            .as_ref()
            .ok_or(Status::NGX_ERROR)
    }
}

extern "C" fn shared_state_zone_init(
    shm_zone: *mut ngx_shm_zone_t,
    _data: *mut core::ffi::c_void,
) -> ngx_int_t {
    let shm_zone = unsafe { &mut *shm_zone };
    match shared_state_map(shm_zone) {
        Ok(_) => Status::NGX_OK.into(),
        Err(_) => Status::NGX_ERROR.into(),
    }
}
