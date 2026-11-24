use core::ffi::c_void;

#[cfg(test)]
mod tests;

#[repr(C)]
pub struct Bink {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub frame_num: u32,
    pub last_frame_num: u32,
    pub frame_rate: u32,
    pub frame_rate_div: u32,
    pub read_error: u32,
    pub open_flags: u32,
    pub bink_type: u32,
    pub size: u32,
    pub frame_size: u32,
    pub s_width: u32,
    pub s_height: u32,
    pub paused: u32,
}

#[no_mangle]
pub unsafe extern "system" fn BinkOpen(_name: *const c_void, _flags: u32) -> *mut Bink {
    Box::into_raw(Box::new(Bink {
        width: 1920,
        height: 1080,
        frames: 100,
        frame_num: 0,
        last_frame_num: 0,
        frame_rate: 30,
        frame_rate_div: 1,
        read_error: 0,
        open_flags: 0,
        bink_type: 0,
        size: 0,
        frame_size: 0,
        s_width: 1920,
        s_height: 1080,
        paused: 0,
    }))
}

#[no_mangle]
pub unsafe extern "system" fn BinkClose(bink: *mut Bink) {
    if !bink.is_null() {
        drop(Box::from_raw(bink));
    }
}

#[no_mangle]
pub unsafe extern "system" fn BinkDoFramePlane(_bink: *mut Bink, _plane: u32) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkFreeGlobals() {}

#[no_mangle]
pub unsafe extern "system" fn BinkGetFrameBuffersInfo(_bink: *mut Bink, _info: *mut c_void) {}

#[no_mangle]
pub unsafe extern "system" fn BinkGetKeyFrame(_bink: *mut Bink, frame: u32, _flags: u32) -> u32 {
    frame
}

#[no_mangle]
pub unsafe extern "system" fn BinkGetRealtime(_bink: *mut Bink, _run: *mut u32, _frames: *mut u32) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkGetTrackID(_bink: *mut Bink, track_index: u32) -> u32 {
    track_index
}

#[no_mangle]
pub unsafe extern "system" fn BinkGoto(bink: *mut Bink, frame: u32, _flags: u32) {
    if !bink.is_null() {
        (*bink).frame_num = frame;
    }
}

#[no_mangle]
pub unsafe extern "system" fn BinkNextFrame(bink: *mut Bink) {
    if !bink.is_null() {
        (*bink).frame_num = (*bink).frame_num.saturating_add(1);
    }
}

#[no_mangle]
pub unsafe extern "system" fn BinkPause(bink: *mut Bink, pause: i32) -> i32 {
    if !bink.is_null() {
        let prev = (*bink).paused;
        (*bink).paused = pause as u32;
        prev as i32
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn BinkRegisterFrameBuffers(_bink: *mut Bink, _info: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkSetFileOffset(_offset: u64) {}

#[no_mangle]
pub unsafe extern "system" fn BinkSetFrameRate(_rate: u32, _scale: u32) {}

#[no_mangle]
pub unsafe extern "system" fn BinkSetMemory(_alloc_fn: *const c_void, _free_fn: *const c_void) {}

#[no_mangle]
pub unsafe extern "system" fn BinkSetOSFileCallbacks(_open: *const c_void, _read: *const c_void, _close: *const c_void) {}

#[no_mangle]
pub unsafe extern "system" fn BinkSetSoundSystem(_init_fn: *const c_void, _param: *const c_void) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkSetSoundTrack(_total: u32, _ids: *const u32) {}

#[no_mangle]
pub unsafe extern "system" fn BinkSetSpeakerVolumes(_bink: *mut Bink, _track: u32, _vols: *const f32, _total_vols: u32) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkSetVolume(_bink: *mut Bink, _track: u32, _volume: i32) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkSetWillLoop(_bink: *mut Bink, _loop_flag: i32) {}

#[no_mangle]
pub unsafe extern "system" fn BinkShouldSkip(_bink: *mut Bink) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "system" fn BinkWait(_bink: *mut Bink) -> i32 {
    0
}
