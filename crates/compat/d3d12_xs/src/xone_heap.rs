use compat_common::record;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

const PC_CBV_SRV_UAV: u32 = 0;
const PC_NUM_TYPES: u32 = 4;
const XBOX_ONE_EXTRA_CBV_SRV_UAV: u32 = 4;
pub const MAX_SHADER_VISIBLE_CBV_SRV_UAV: u32 = 1_000_000;

pub fn heap_type(t: u32) -> u32 {
    if t == XBOX_ONE_EXTRA_CBV_SRV_UAV {
        PC_CBV_SRV_UAV
    } else {
        t
    }
}

pub fn translate(pdesc: *const c_void) -> Option<[u32; 4]> {
    if pdesc.is_null() {
        return None;
    }
    let d = unsafe { *(pdesc as *const [u32; 4]) };
    let t = heap_type(d[0]);
    let mut num = d[1];
    let flags = d[2];
    if t == PC_CBV_SRV_UAV && (flags & 1 != 0) && num > MAX_SHADER_VISIBLE_CBV_SRV_UAV {
        num = MAX_SHADER_VISIBLE_CBV_SRV_UAV;
    }
    if d[0] == t && d[1] == num {
        return None;
    }
    static LOG: AtomicU32 = AtomicU32::new(0);
    if LOG.fetch_add(1, Ordering::Relaxed) < 8 {
        record("d3d12_xs", "xone.heap", &format!("Xbox descriptor heap type={} num={} flags={:#x} -> PC type={t} num={num}", d[0], d[1], d[2]));
    }
    Some([t, num, flags, d[3]])
}
