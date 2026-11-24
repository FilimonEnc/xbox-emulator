use super::*;

#[test]
fn test_fidelityfx_lifecycle() {
    unsafe {
        let mut ctx: *mut c_void = core::ptr::null_mut();
        assert_eq!(ffxCreateContext(&mut ctx, core::ptr::null(), core::ptr::null()), FFX_OK);
        assert!(!ctx.is_null());
        assert_eq!(ffxDispatch(ctx, core::ptr::null()), FFX_OK);
        assert_eq!(ffxQuery(ctx, core::ptr::null()), FFX_OK);
        assert_eq!(ffxDestroyContext(&mut ctx, core::ptr::null()), FFX_OK);
        assert!(ctx.is_null());
    }
}

#[test]
fn test_null_pointers() {
    unsafe {
        assert_eq!(ffxCreateContext(core::ptr::null_mut(), core::ptr::null(), core::ptr::null()), FFX_ERROR_INVALID_POINTER);
        assert_eq!(ffxDestroyContext(core::ptr::null_mut(), core::ptr::null()), FFX_ERROR_INVALID_POINTER);
    }
}
