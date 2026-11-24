//! Fixes the Xbox↔PC return-convention mismatch for the descriptor-handle getters
//! **without wrapping the heap object** (a wrapper would leak into real D3D12 calls
//! like `SetDescriptorHeaps`, crashing the driver).
//!
//! Instead we patch the real `ID3D12DescriptorHeap` vtable in place: slots 9/10
//! (`GetCPU/GPUDescriptorHandleForHeapStart`) are redirected to small bridges.
//! All descriptor heaps share one vtable, so patching once (on the first heap)
//! fixes them all, and the heap objects stay fully real.
//!
//! Xbox callers expect the 8-byte handle in RAX. PC `D3D12Core` implements these
//! as `fn(this, Handle* retval) -> Handle*` (writes *retval). Each bridge calls the
//! saved original with a local and returns the value in RAX.

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use compat_common::record;

extern "system" {
    fn VirtualProtect(addr: *mut c_void, size: usize, new_protect: u32, old_protect: *mut u32) -> i32;
}
const PAGE_READWRITE: u32 = 0x04;

// All descriptor-heap vtables share the same GetCPU/GPU impl in D3D12Core, so one
// saved original serves every patched vtable. Different heap TYPES can have
// different vtable objects, though, so we patch each distinct one.
static ORIG_CPU: AtomicUsize = AtomicUsize::new(0);
static ORIG_GPU: AtomicUsize = AtomicUsize::new(0);
static PATCHED_VTABLES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static PLACED_HEAPS: Mutex<Vec<(usize, u64, u64)>> = Mutex::new(Vec::new());

pub fn register_placed(heap: *mut c_void, placed_va: u64) {
    if !heap.is_null() && placed_va != 0 {
        let orig: unsafe extern "system" fn(*mut c_void, *mut u64) -> *mut u64 =
            unsafe { core::mem::transmute(ORIG_CPU.load(Ordering::Relaxed)) };
        let mut real_base: u64 = 0;
        if orig as usize != 0 {
            unsafe { orig(heap, &mut real_base) };
        }
        PLACED_HEAPS.lock().unwrap().push((heap as usize, placed_va, real_base));
        record("d3d12_xs", "heap.placed", &format!("heap={heap:p} placed_va={placed_va:#x} real_base={real_base:#x}"));
    }
}

pub fn map_cpu_handle(handle: usize) -> usize {
    let g = PLACED_HEAPS.lock().unwrap();
    for &(_, placed_base, real_base) in g.iter() {
        let placed_base = placed_base as usize;
        let real_base = real_base as usize;
        if handle >= placed_base && handle < placed_base + 0x4000_0000 {
            return real_base.wrapping_add(handle.wrapping_sub(placed_base));
        }
    }
    handle
}

unsafe extern "system" fn bridge_cpu(heap: *mut c_void) -> u64 {
    let orig: unsafe extern "system" fn(*mut c_void, *mut u64) -> *mut u64 =
        unsafe { core::mem::transmute(ORIG_CPU.load(Ordering::Relaxed)) };
    let mut real_base: u64 = 0;
    unsafe { orig(heap, &mut real_base) };
    let placed_base = PLACED_HEAPS.lock().unwrap().iter().find(|e| e.0 == heap as usize).map(|e| e.1);
    let out = placed_base.unwrap_or(real_base);
    static N: AtomicUsize = AtomicUsize::new(0);
    if N.fetch_add(1, Ordering::Relaxed) < 6 {
        record("d3d12_xs", "heap.GetCPUHandleBase", &format!("heap={heap:p} base={out:#x} (real={real_base:#x})"));
    }
    out
}

unsafe extern "system" fn bridge_gpu(heap: *mut c_void) -> u64 {
    let orig: unsafe extern "system" fn(*mut c_void, *mut u64) -> *mut u64 =
        unsafe { core::mem::transmute(ORIG_GPU.load(Ordering::Relaxed)) };
    let mut out: u64 = 0;
    unsafe { orig(heap, &mut out) };
    static N: AtomicUsize = AtomicUsize::new(0);
    if N.fetch_add(1, Ordering::Relaxed) < 6 {
        record("d3d12_xs", "heap.GetGPUHandleBase", &format!("heap={heap:p} base={out:#x}"));
    }
    out
}

/// Patch this heap's descriptor-heap vtable (slots 9/10) if not already done.
/// Called for every created heap so each distinct heap-type vtable gets patched.
pub fn ensure_patched(real_heap: *mut c_void) {
    if real_heap.is_null() {
        return;
    }
    let vtbl = unsafe { *(real_heap as *const *mut usize) };
    let vtbl_key = vtbl as usize;
    let mut patched = PATCHED_VTABLES.lock().unwrap();
    if patched.contains(&vtbl_key) {
        return;
    }
    unsafe {
        let cur_cpu = *vtbl.add(9);
        let cur_gpu = *vtbl.add(10);
        // Don't re-save our own bridges if somehow already pointing at them.
        if cur_cpu != bridge_cpu as usize {
            ORIG_CPU.store(cur_cpu, Ordering::Relaxed);
        }
        if cur_gpu != bridge_gpu as usize {
            ORIG_GPU.store(cur_gpu, Ordering::Relaxed);
        }
        let addr = vtbl.add(9) as *mut c_void;
        let mut old: u32 = 0;
        if VirtualProtect(addr, 16, PAGE_READWRITE, &mut old) != 0 {
            *vtbl.add(9) = bridge_cpu as usize;
            *vtbl.add(10) = bridge_gpu as usize;
            let mut tmp: u32 = 0;
            VirtualProtect(addr, 16, old, &mut tmp);
            patched.push(vtbl_key);
            record("d3d12_xs", "heap.patch", &format!("vtable {vtbl:p} slots 9/10 bridged ({} total)", patched.len()));
        } else {
            record("d3d12_xs", "heap.patch", "VirtualProtect failed");
        }
    }
}
