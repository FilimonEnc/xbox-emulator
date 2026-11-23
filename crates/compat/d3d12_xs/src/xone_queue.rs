use compat_common::record;

const PC_PRIORITY_NORMAL: u32 = 0;
const PC_PRIORITY_HIGH: u32 = 100;
const PC_PRIORITY_GLOBAL_REALTIME: u32 = 10000;
const PC_TYPE_COPY: u32 = 3;
const XBOX_TYPE_DMA: u32 = 0x10;

fn get(d: &[u8; 16], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn put(d: &mut [u8; 16], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

pub fn translate(d: &mut [u8; 16]) {
    let (ty, prio, flags, node) = (get(d, 0), get(d, 4), get(d, 8), get(d, 12));
    let pc_ty = if ty == XBOX_TYPE_DMA { PC_TYPE_COPY } else { ty };
    let pc_prio = match prio {
        PC_PRIORITY_NORMAL | PC_PRIORITY_HIGH | PC_PRIORITY_GLOBAL_REALTIME => prio,
        1 => PC_PRIORITY_HIGH,
        _ => PC_PRIORITY_HIGH,
    };
    let pc_flags = flags & 1;
    let pc_node = if node == 0 { 0 } else { 1 };
    put(d, 0, pc_ty);
    put(d, 4, pc_prio);
    put(d, 8, pc_flags);
    put(d, 12, pc_node);
    if (pc_ty, pc_prio, pc_flags, pc_node) != (ty, prio, flags, node) {
        record("d3d12_xs", "xone.queue", &format!(
            "Xbox queue desc type={ty:#x} priority={prio} flags={flags:#x} node={node:#x} -> PC type={pc_ty} priority={pc_prio} flags={pc_flags:#x} node={pc_node}"
        ));
    }
}

pub fn list_type(t: u32) -> u32 {
    if t == XBOX_TYPE_DMA { PC_TYPE_COPY } else { t }
}

pub unsafe fn create_allocator(real: *mut core::ffi::c_void, t: u32, riid: *const core::ffi::c_void, ppv: *mut *mut core::ffi::c_void) -> i32 {
    let pc = list_type(t);
    let f: unsafe extern "system" fn(*mut core::ffi::c_void, u32, *const core::ffi::c_void, *mut *mut core::ffi::c_void) -> i32 =
        unsafe { core::mem::transmute(*(*(real as *const *const usize)).add(9)) };
    let hr = unsafe { f(real, pc, riid, ppv) };
    if pc != t || hr != 0 {
        record("d3d12_xs", "xone.CreateCommandAllocator", &format!("Xbox type={t:#x} -> PC type={pc} hr={hr:#010x}"));
    }
    hr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_masks_console_queue_flags() {
        let mut d = [0u8; 16];
        put(&mut d, 0, 2);
        put(&mut d, 4, 0);
        put(&mut d, 8, 2);
        put(&mut d, 12, 0);
        translate(&mut d);
        assert_eq!(get(&d, 0), 2);
        assert_eq!(get(&d, 4), 0);
        assert_eq!(get(&d, 8), 0);
        assert_eq!(get(&d, 12), 0);
    }

    #[test]
    fn translate_preserves_disable_gpu_timeout_flag() {
        let mut d = [0u8; 16];
        put(&mut d, 0, 2);
        put(&mut d, 4, 100);
        put(&mut d, 8, 3);
        put(&mut d, 12, 0);
        translate(&mut d);
        assert_eq!(get(&d, 0), 2);
        assert_eq!(get(&d, 4), 100);
        assert_eq!(get(&d, 8), 1);
        assert_eq!(get(&d, 12), 0);
    }
}
