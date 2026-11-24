use core::ffi::c_void;

#[cfg(test)]
mod tests;

pub const FFX_OK: i32 = 0;
pub const FFX_ERROR_INVALID_POINTER: i32 = -1;

#[no_mangle]
pub unsafe extern "system" fn ffxCreateContext(
    context: *mut *mut c_void,
    _header: *const c_void,
    _mem_cb: *const c_void,
) -> i32 {
    if context.is_null() {
        return FFX_ERROR_INVALID_POINTER;
    }
    let dummy = Box::into_raw(Box::new(0x46465843u32));
    *context = dummy as *mut c_void;
    FFX_OK
}

#[no_mangle]
pub unsafe extern "system" fn ffxDestroyContext(
    context: *mut *mut c_void,
    _mem_cb: *const c_void,
) -> i32 {
    if context.is_null() {
        return FFX_ERROR_INVALID_POINTER;
    }
    if !(*context).is_null() {
        drop(Box::from_raw(*context as *mut u32));
        *context = core::ptr::null_mut();
    }
    FFX_OK
}

#[no_mangle]
pub unsafe extern "system" fn ffxDispatch(
    _context: *mut c_void,
    _header: *const c_void,
) -> i32 {
    FFX_OK
}

#[no_mangle]
pub unsafe extern "system" fn ffxQuery(
    _context: *mut c_void,
    _header: *const c_void,
) -> i32 {
    FFX_OK
}
