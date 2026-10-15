use std::collections::HashMap;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use lyric_core::json_store::JsonStore;

static NEXT: AtomicU64 = AtomicU64::new(1);
static STORES: OnceLock<Mutex<HashMap<u64, Arc<JsonStore>>>> = OnceLock::new();
fn stores() -> &'static Mutex<HashMap<u64, Arc<JsonStore>>> { STORES.get_or_init(|| Mutex::new(HashMap::new())) }
fn find(id: u64) -> Option<Arc<JsonStore>> { stores().lock().ok()?.get(&id).cloned() }

/// # Safety
/// `path` must be a valid UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn li_json_cache_create(path: *const c_char) -> u64 {
    if path.is_null() { return 0; }
    catch_unwind(AssertUnwindSafe(|| {
        let path = super::from_c_str(path); if path.is_empty() { return 0; }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut stores) = stores().lock() { stores.insert(id, Arc::new(JsonStore::new(path))); id } else { 0 }
    })).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn li_json_cache_destroy(id: u64) { if let Ok(mut stores) = stores().lock() { stores.remove(&id); } }
/// # Safety
/// `key` must be a valid UTF-8 C string, `status` writable. Returned strings use li_free_string.
#[no_mangle]
pub unsafe extern "C" fn li_json_cache_get(id: u64, key: *const c_char, status: *mut i32) -> *mut c_char {
    if key.is_null() || status.is_null() { return std::ptr::null_mut(); }
    status.write(-1);
    catch_unwind(AssertUnwindSafe(|| {
        let Some(cache) = find(id) else { return std::ptr::null_mut() };
        match cache.get(&super::from_c_str(key)) {
            Some((payload, acquisition)) => { status.write(acquisition); super::to_c_string(payload) }
            None => { status.write(0); std::ptr::null_mut() }
        }
    })).unwrap_or(std::ptr::null_mut())
}
/// # Safety
/// `key` and `payload` must be valid UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn li_json_cache_store(id: u64, key: *const c_char, payload: *const c_char) -> i32 {
    if key.is_null() || payload.is_null() { return 0; }
    catch_unwind(AssertUnwindSafe(|| {
        let Some(cache) = find(id) else { return 0 };
        match cache.store(&super::from_c_str(key), &super::from_c_str(payload)) { Ok(true) => 1, Ok(false) => 2, Err(_) => 0 }
    })).unwrap_or(0)
}
/// # Safety
/// `key` must be a valid UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn li_json_cache_remove(id: u64, key: *const c_char) -> i32 {
    if key.is_null() { return 0; }
    catch_unwind(AssertUnwindSafe(|| find(id).map(|cache| if cache.remove(&super::from_c_str(key)) { 1 } else { 2 }).unwrap_or(0))).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn li_json_cache_clear(id: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| find(id).map(|cache| if cache.clear() { 1 } else { 2 }).unwrap_or(0))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*; use std::ffi::{CString, CStr};
    #[test]
    fn handles_and_string_ownership_roundtrip() {
        let path = std::env::temp_dir().join(format!("li-ffi-cache-{}.json", NEXT.fetch_add(1, Ordering::Relaxed)));
        let path_c = CString::new(path.to_str().unwrap()).unwrap(); let key = CString::new("Title").unwrap();
        let payload = CString::new(r#"{"Value":"歌词"}"#).unwrap();
        unsafe {
            let id = li_json_cache_create(path_c.as_ptr()); assert_ne!(id, 0);
            assert_eq!(li_json_cache_store(id, key.as_ptr(), payload.as_ptr()), 1);
            let mut status = -1; let value = li_json_cache_get(id, key.as_ptr(), &mut status);
            assert_eq!(status, 1); assert_eq!(CStr::from_ptr(value).to_str().unwrap(), payload.to_str().unwrap());
            super::super::li_free_string(value);
            assert_eq!(li_json_cache_clear(id), 1); li_json_cache_destroy(id);
            assert!(li_json_cache_get(id, key.as_ptr(), &mut status).is_null()); assert_eq!(status, -1);
        }
    }
}
