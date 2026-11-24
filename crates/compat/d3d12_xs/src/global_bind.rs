use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use std::sync::Mutex;
use compat_common::record;
use tombox_xshader::global_abi::{
    window_cb, Window, TB_GLOBAL_CBV, TB_GLOBAL_CB_DWORDS, TB_GLOBAL_FAULT_UAV, TB_GLOBAL_SPACE,
    TB_GLOBAL_WINDOWS,
};

use crate::device_wrap::Guid;

const PARAM_SIZE: usize = 32;
const VERSION_1_1: u32 = 2;
const ROOT_FLAG_LOCAL: u32 = 0x80;
const VISIBILITY_ALL: u32 = 0;
const TOTAL_DWORDS: u32 = TB_GLOBAL_CB_DWORDS as u32 + 2 * (TB_GLOBAL_WINDOWS as u32 + 1);

const DEV_CREATE_COMMITTED: usize = 27;
const RES_GET_GPU_VA: usize = 11;
const STATE_UAV: u32 = 0x8;
const FAULT_BYTES: u64 = 256;

const PC_SET_COMPUTE_ROOT_CONSTANTS: usize = 35;
const PC_SET_GRAPHICS_ROOT_CONSTANTS: usize = 36;
const PC_SET_COMPUTE_ROOT_UAV: usize = 41;
const PC_SET_GRAPHICS_ROOT_UAV: usize = 42;

const XBOX_SLOT_DRAW: usize = 12;
const XBOX_SLOT_DRAW_INDEXED: usize = 13;
const XBOX_SLOT_DISPATCH: usize = 14;
const XBOX_SLOT_SET_COMPUTE_RS: usize = 33;
const XBOX_SLOT_SET_GRAPHICS_RS: usize = 34;
const XBOX_SLOT_SET_COMPUTE_CONST: usize = 37;
const XBOX_SLOT_SET_GRAPHICS_CONST: usize = 38;
const XBOX_SLOT_SET_COMPUTE_CONSTS: usize = 39;
const XBOX_SLOT_SET_GRAPHICS_CONSTS: usize = 40;
const XBOX_SLOT_SET_COMPUTE_CBV: usize = 41;
const XBOX_SLOT_SET_GRAPHICS_CBV: usize = 42;
const XBOX_SLOT_SET_COMPUTE_SRV: usize = 43;
const XBOX_SLOT_SET_GRAPHICS_SRV: usize = 44;
const XBOX_SLOT_SET_COMPUTE_UAV: usize = 45;
const XBOX_SLOT_SET_GRAPHICS_UAV: usize = 46;

const IID_DESERIALIZER: Guid =
    Guid { d1: 0x7f91_ce67, d2: 0x090c, d3: 0x4bb7, d4: [0xb7, 0x8e, 0xed, 0x8f, 0xf2, 0xe3, 0x1d, 0xa0] };
const IID_RESOURCE: Guid =
    Guid { d1: 0x6964_42be, d2: 0xa72e, d3: 0x4059, d4: [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad] };
const IID_DEVICE: Guid =
    Guid { d1: 0x1898_19f1, d2: 0x1db6, d3: 0x4b57, d4: [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7] };

#[repr(C)]
struct Mbi {
    base: usize,
    alloc_base: usize,
    alloc_prot: u32,
    part_id: u16,
    _pad: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    ty: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn VirtualQuery(addr: *const c_void, info: *mut Mbi, len: usize) -> usize;
}

struct PipeState {
    rs: usize,
    vas: [u64; 64],
    consts: HashMap<u32, Vec<u32>>,
}

impl Default for PipeState {
    fn default() -> Self {
        Self { rs: 0, vas: [0; 64], consts: HashMap::new() }
    }
}

#[derive(Default)]
struct ListState {
    compute: PipeState,
    graphics: PipeState,
}

static ROOTS: Mutex<Option<HashMap<usize, u32>>> = Mutex::new(None);
static LISTS: Mutex<Option<HashMap<usize, ListState>>> = Mutex::new(None);
static FAULT_BUF: Mutex<Option<(usize, u64)>> = Mutex::new(None);
static LOG: AtomicUsize = AtomicUsize::new(0);

static PREV_DRAW: AtomicUsize = AtomicUsize::new(0);
static PREV_DRAW_IDX: AtomicUsize = AtomicUsize::new(0);
static PREV_DISPATCH: AtomicUsize = AtomicUsize::new(0);
static PREV_CROOT: AtomicUsize = AtomicUsize::new(0);
static PREV_GROOT: AtomicUsize = AtomicUsize::new(0);
static PREV_CCONST: AtomicUsize = AtomicUsize::new(0);
static PREV_GCONST: AtomicUsize = AtomicUsize::new(0);
static PREV_CCONSTS: AtomicUsize = AtomicUsize::new(0);
static PREV_GCONSTS: AtomicUsize = AtomicUsize::new(0);
static PREV_CCBV: AtomicUsize = AtomicUsize::new(0);
static PREV_GCBV: AtomicUsize = AtomicUsize::new(0);
static PREV_CSRV: AtomicUsize = AtomicUsize::new(0);
static PREV_GSRV: AtomicUsize = AtomicUsize::new(0);
static PREV_CUAV: AtomicUsize = AtomicUsize::new(0);
static PREV_GUAV: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn refusal(_needs_global: bool) -> Option<String> {
    None
}

fn once(n: usize) -> bool {
    LOG.fetch_add(1, Ordering::Relaxed) < n
}

unsafe fn vslot(obj: *mut c_void, i: usize) -> usize {
    unsafe { *(*(obj as *const *const usize)).add(i) }
}

fn u32_at(p: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(p[o..o + 4].try_into().unwrap())
}

fn cbv_param() -> [u8; PARAM_SIZE] {
    let mut p = [0u8; PARAM_SIZE];
    p[0..4].copy_from_slice(&crate::gds::PARAM_CONSTANTS.to_le_bytes());
    p[8..12].copy_from_slice(&TB_GLOBAL_CBV.to_le_bytes());
    p[12..16].copy_from_slice(&TB_GLOBAL_SPACE.to_le_bytes());
    p[16..20].copy_from_slice(&(TB_GLOBAL_CB_DWORDS as u32).to_le_bytes());
    p[24..28].copy_from_slice(&VISIBILITY_ALL.to_le_bytes());
    p
}

fn uav_param(reg: u32) -> [u8; PARAM_SIZE] {
    let mut p = [0u8; PARAM_SIZE];
    p[0..4].copy_from_slice(&crate::gds::PARAM_UAV.to_le_bytes());
    p[8..12].copy_from_slice(&reg.to_le_bytes());
    p[12..16].copy_from_slice(&TB_GLOBAL_SPACE.to_le_bytes());
    p[24..28].copy_from_slice(&VISIBILITY_ALL.to_le_bytes());
    p
}

pub fn append_plan(params: &[u8], flags: u32) -> Option<(Vec<u8>, u32)> {
    if flags & ROOT_FLAG_LOCAL != 0 {
        return None;
    }
    if crate::gds::root_cost(params) + TOTAL_DWORDS > crate::gds::ROOT_DWORD_LIMIT {
        return None;
    }
    let idx = (params.len() / PARAM_SIZE) as u32;
    let mut v = params.to_vec();
    v.extend_from_slice(&cbv_param());
    for i in 0..TB_GLOBAL_WINDOWS as u32 {
        v.extend_from_slice(&uav_param(i));
    }
    v.extend_from_slice(&uav_param(TB_GLOBAL_FAULT_UAV));
    Some((v, idx))
}

unsafe fn release(o: *mut c_void) {
    if !o.is_null() {
        let r: unsafe extern "system" fn(*mut c_void) -> u32 = unsafe { core::mem::transmute(vslot(o, 2)) };
        unsafe { r(o) };
    }
}

pub unsafe fn patch_root_signature(blob: *const u8, len: usize) -> Option<(Vec<u8>, u32)> {
    if blob.is_null() || len == 0 {
        return None;
    }
    let mk = crate::host_proc(b"D3D12CreateVersionedRootSignatureDeserializer\0");
    let ser = crate::host_proc(b"D3D12SerializeVersionedRootSignature\0");
    if mk == 0 || ser == 0 {
        return None;
    }
    let mk: unsafe extern "system" fn(*const u8, usize, *const Guid, *mut *mut c_void) -> i32 =
        unsafe { core::mem::transmute(mk) };
    let ser: unsafe extern "system" fn(*const u8, *mut *mut c_void, *mut *mut c_void) -> i32 =
        unsafe { core::mem::transmute(ser) };
    let mut d: *mut c_void = core::ptr::null_mut();
    if unsafe { mk(blob, len, &IID_DESERIALIZER, &mut d) } < 0 || d.is_null() {
        return None;
    }
    let get: unsafe extern "system" fn(*mut c_void, u32, *mut *const u8) -> i32 =
        unsafe { core::mem::transmute(vslot(d, 3)) };
    let mut desc: *const u8 = core::ptr::null();
    let mut out = None;
    if unsafe { get(d, VERSION_1_1, &mut desc) } >= 0 && !desc.is_null() {
        let vd = unsafe { core::slice::from_raw_parts(desc, 48) }.to_vec();
        let n = u32_at(&vd, 8) as usize;
        let pp = u64::from_le_bytes(vd[16..24].try_into().unwrap()) as *const u8;
        let params = if n == 0 || pp.is_null() {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(pp, n * PARAM_SIZE) }.to_vec()
        };
        match append_plan(&params, u32_at(&vd, 40)) {
            Some((np, idx)) => {
                let mut nd = vd.clone();
                let added = 2 + TB_GLOBAL_WINDOWS;
                nd[8..12].copy_from_slice(&((n + added) as u32).to_le_bytes());
                nd[16..24].copy_from_slice(&(np.as_ptr() as u64).to_le_bytes());
                let (mut b, mut e): (*mut c_void, *mut c_void) = (core::ptr::null_mut(), core::ptr::null_mut());
                let hr = unsafe { ser(nd.as_ptr(), &mut b, &mut e) };
                if hr >= 0 && !b.is_null() {
                    let ptr: unsafe extern "system" fn(*mut c_void) -> *const u8 =
                        unsafe { core::mem::transmute(vslot(b, 3)) };
                    let size: unsafe extern "system" fn(*mut c_void) -> usize =
                        unsafe { core::mem::transmute(vslot(b, 4)) };
                    out = Some((unsafe { core::slice::from_raw_parts(ptr(b), size(b)) }.to_vec(), idx));
                } else if once(8) {
                    record("d3d12_xs", "global_bind", &format!("re-serializing root signature with TbGlobal failed hr={hr:#010x}"));
                }
                unsafe {
                    release(b);
                    release(e);
                }
            }
            None => {
                if once(8) {
                    record("d3d12_xs", "global_bind", &format!("root signature uses {} DWORDs (or is local); no room for TbGlobal", crate::gds::root_cost(&params)));
                }
            }
        }
    }
    unsafe { release(d) };
    out
}

pub fn note_root_signature(rs: *mut c_void, idx: u32) {
    if !rs.is_null() {
        ROOTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(rs as usize, idx);
    }
}

pub fn forget(rs: *mut c_void) {
    if let Some(m) = ROOTS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(&(rs as usize));
    }
}

fn root_index(rs: usize) -> Option<u32> {
    if rs == 0 {
        return None;
    }
    ROOTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&rs).copied())
}

unsafe fn fault_buffer(list: *mut c_void) -> Option<(usize, u64)> {
    let mut g = FAULT_BUF.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(b) = *g {
        return Some(b);
    }
    let get: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
        unsafe { core::mem::transmute(vslot(list, 7)) };
    let mut dev: *mut c_void = core::ptr::null_mut();
    if unsafe { get(list, &IID_DEVICE, &mut dev) } < 0 || dev.is_null() {
        return None;
    }
    let heap: [u32; 5] = [1, 0, 0, 1, 1];
    let mut desc = [0u8; 56];
    desc[0..4].copy_from_slice(&1u32.to_le_bytes());
    desc[0x10..0x18].copy_from_slice(&FAULT_BYTES.to_le_bytes());
    desc[0x18..0x1c].copy_from_slice(&1u32.to_le_bytes());
    desc[0x1c..0x1e].copy_from_slice(&1u16.to_le_bytes());
    desc[0x1e..0x20].copy_from_slice(&1u16.to_le_bytes());
    desc[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
    desc[0x2c..0x30].copy_from_slice(&1u32.to_le_bytes());
    desc[0x30..0x34].copy_from_slice(&4u32.to_le_bytes());
    let create: unsafe extern "system" fn(*mut c_void, *const u32, u32, *const u8, u32, *const c_void, *const Guid, *mut *mut c_void) -> i32 =
        unsafe { core::mem::transmute(vslot(dev, DEV_CREATE_COMMITTED)) };
    let mut res: *mut c_void = core::ptr::null_mut();
    let hr = unsafe { create(dev, heap.as_ptr(), 0, desc.as_ptr(), STATE_UAV, core::ptr::null(), &IID_RESOURCE, &mut res) };
    unsafe { release(dev) };
    if hr < 0 || res.is_null() {
        record("d3d12_xs", "global_bind", &format!("TbGlobal fault buffer create FAILED hr={hr:#010x}"));
        return None;
    }
    let va: unsafe extern "system" fn(*mut c_void) -> u64 = unsafe { core::mem::transmute(vslot(res, RES_GET_GPU_VA)) };
    let b = (res as usize, unsafe { va(res) });
    *g = Some(b);
    Some(b)
}

fn push_va(out: &mut Vec<(Window, u64)>, va: u64) {
    if va == 0 || out.len() >= TB_GLOBAL_WINDOWS {
        return;
    }
    let Some((e, _)) = crate::gpu_va::lookup_buffer(va) else { return };
    if e.res == 0 {
        return;
    }
    let pc_va: u64 = unsafe {
        let f: unsafe extern "system" fn(*mut c_void) -> u64 = core::mem::transmute(vslot(e.res as *mut c_void, RES_GET_GPU_VA));
        f(e.res as *mut c_void)
    };
    if pc_va == 0 || out.iter().any(|&(_, p)| p == pc_va) {
        return;
    }
    out.push((Window { base: e.start, size: e.len.min(u32::MAX as u64) as u32 }, pc_va));
}

fn scan_cpu_mem(out: &mut Vec<(Window, u64)>, addr: u64) {
    if addr < 0x10000 || addr > 0x7fff_ffff_0000 || out.len() >= TB_GLOBAL_WINDOWS {
        return;
    }
    let mut mbi = core::mem::MaybeUninit::<Mbi>::zeroed();
    let got = unsafe { VirtualQuery(addr as *const c_void, mbi.as_mut_ptr(), core::mem::size_of::<Mbi>()) };
    if got == 0 {
        return;
    }
    let mbi = unsafe { mbi.assume_init() };
    if mbi.state != 0x1000 || (mbi.protect & 0x66) == 0 || (mbi.protect & 0x101) != 0 {
        return;
    }
    let avail = (mbi.base + mbi.region_size).saturating_sub(addr as usize);
    let n = (avail / 8).min(64);
    for i in 0..n {
        let v = unsafe { core::ptr::read_unaligned((addr as *const u64).add(i)) };
        push_va(out, v);
        if out.len() >= TB_GLOBAL_WINDOWS {
            break;
        }
    }
}

fn collect_windows(pipe: &PipeState) -> Vec<(Window, u64)> {
    let mut out = Vec::with_capacity(TB_GLOBAL_WINDOWS);
    for &va in &pipe.vas {
        push_va(&mut out, va);
        if out.len() >= TB_GLOBAL_WINDOWS {
            return out;
        }
    }
    for words in pipe.consts.values() {
        for pair in words.chunks_exact(2) {
            let va = (pair[0] as u64) | ((pair[1] as u64) << 32);
            push_va(&mut out, va);
            if out.len() >= TB_GLOBAL_WINDOWS {
                return out;
            }
        }
    }
    for &va in &pipe.vas {
        scan_cpu_mem(&mut out, va);
        if out.len() >= TB_GLOBAL_WINDOWS {
            return out;
        }
    }
    out
}

unsafe fn bind_global(list: *mut c_void, compute: bool) {
    let (rs, wins) = {
        let g = LISTS.lock().unwrap_or_else(|e| e.into_inner());
        let Some(m) = g.as_ref() else { return };
        let Some(ls) = m.get(&(list as usize)) else { return };
        let pipe = if compute { &ls.compute } else { &ls.graphics };
        let Some(base_slot) = root_index(pipe.rs) else { return };
        (base_slot, collect_windows(pipe))
    };
    let Some((_, fault_va)) = (unsafe { fault_buffer(list) }) else { return };
    let mut win_desc = [Window::default(); TB_GLOBAL_WINDOWS];
    let mut win_vas = [fault_va; TB_GLOBAL_WINDOWS];
    for (i, &(w, pc_va)) in wins.iter().take(TB_GLOBAL_WINDOWS).enumerate() {
        win_desc[i] = w;
        win_vas[i] = pc_va;
    }
    let cb = window_cb(&win_desc, [0, 0]);
    let set_consts: unsafe extern "system" fn(*mut c_void, u32, u32, *const u32, u32) = unsafe {
        core::mem::transmute(crate::cmdlist_wrap::pc_method(
            list,
            if compute { PC_SET_COMPUTE_ROOT_CONSTANTS } else { PC_SET_GRAPHICS_ROOT_CONSTANTS },
        ))
    };
    let set_uav: unsafe extern "system" fn(*mut c_void, u32, u64) = unsafe {
        core::mem::transmute(crate::cmdlist_wrap::pc_method(
            list,
            if compute { PC_SET_COMPUTE_ROOT_UAV } else { PC_SET_GRAPHICS_ROOT_UAV },
        ))
    };
    unsafe {
        set_consts(list, rs, TB_GLOBAL_CB_DWORDS as u32, cb.as_ptr(), 0);
        for (i, &va) in win_vas.iter().enumerate() {
            set_uav(list, rs + 1 + i as u32, va);
        }
        set_uav(list, rs + 1 + TB_GLOBAL_WINDOWS as u32, fault_va);
    }
}

fn update_pipe(list: *mut c_void, compute: bool, f: impl FnOnce(&mut PipeState)) {
    let mut g = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    let ls = g.get_or_insert_with(HashMap::new).entry(list as usize).or_default();
    f(if compute { &mut ls.compute } else { &mut ls.graphics });
}

unsafe extern "system" fn h_draw(this: *mut c_void, a: u32, b: u32, c: u32, d: u32) {
    unsafe { bind_global(this, false) };
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, u32, u32) =
        unsafe { core::mem::transmute(PREV_DRAW.load(Ordering::Relaxed)) };
    unsafe { f(this, a, b, c, d) };
}

unsafe extern "system" fn h_draw_idx(this: *mut c_void, a: u32, b: u32, c: u32, d: i32, e: u32) {
    unsafe { bind_global(this, false) };
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, u32, i32, u32) =
        unsafe { core::mem::transmute(PREV_DRAW_IDX.load(Ordering::Relaxed)) };
    unsafe { f(this, a, b, c, d, e) };
}

unsafe extern "system" fn h_dispatch(this: *mut c_void, x: u32, y: u32, z: u32) {
    unsafe { bind_global(this, true) };
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, u32) =
        unsafe { core::mem::transmute(PREV_DISPATCH.load(Ordering::Relaxed)) };
    unsafe { f(this, x, y, z) };
}

unsafe extern "system" fn h_croot(this: *mut c_void, rs: *mut c_void) {
    update_pipe(this, true, |p| {
        p.rs = rs as usize;
        p.vas = [0; 64];
        p.consts.clear();
    });
    let f: unsafe extern "system" fn(*mut c_void, *mut c_void) =
        unsafe { core::mem::transmute(PREV_CROOT.load(Ordering::Relaxed)) };
    unsafe { f(this, rs) };
}

unsafe extern "system" fn h_groot(this: *mut c_void, rs: *mut c_void) {
    update_pipe(this, false, |p| {
        p.rs = rs as usize;
        p.vas = [0; 64];
        p.consts.clear();
    });
    let f: unsafe extern "system" fn(*mut c_void, *mut c_void) =
        unsafe { core::mem::transmute(PREV_GROOT.load(Ordering::Relaxed)) };
    unsafe { f(this, rs) };
}

unsafe extern "system" fn h_cconst(this: *mut c_void, idx: u32, v: u32, off: u32) {
    update_pipe(this, true, |p| {
        let c = p.consts.entry(idx).or_default();
        if c.len() <= off as usize {
            c.resize(off as usize + 1, 0);
        }
        c[off as usize] = v;
    });
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, u32) =
        unsafe { core::mem::transmute(PREV_CCONST.load(Ordering::Relaxed)) };
    unsafe { f(this, idx, v, off) };
}

unsafe extern "system" fn h_gconst(this: *mut c_void, idx: u32, v: u32, off: u32) {
    update_pipe(this, false, |p| {
        let c = p.consts.entry(idx).or_default();
        if c.len() <= off as usize {
            c.resize(off as usize + 1, 0);
        }
        c[off as usize] = v;
    });
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, u32) =
        unsafe { core::mem::transmute(PREV_GCONST.load(Ordering::Relaxed)) };
    unsafe { f(this, idx, v, off) };
}

unsafe extern "system" fn h_cconsts(this: *mut c_void, idx: u32, n: u32, pdata: *const u32, off: u32) {
    if !pdata.is_null() && n > 0 {
        let slice = unsafe { core::slice::from_raw_parts(pdata, n as usize) };
        update_pipe(this, true, |p| {
            let c = p.consts.entry(idx).or_default();
            let end = off as usize + n as usize;
            if c.len() < end {
                c.resize(end, 0);
            }
            c[off as usize..end].copy_from_slice(slice);
        });
    }
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, *const u32, u32) =
        unsafe { core::mem::transmute(PREV_CCONSTS.load(Ordering::Relaxed)) };
    unsafe { f(this, idx, n, pdata, off) };
}

unsafe extern "system" fn h_gconsts(this: *mut c_void, idx: u32, n: u32, pdata: *const u32, off: u32) {
    if !pdata.is_null() && n > 0 {
        let slice = unsafe { core::slice::from_raw_parts(pdata, n as usize) };
        update_pipe(this, false, |p| {
            let c = p.consts.entry(idx).or_default();
            let end = off as usize + n as usize;
            if c.len() < end {
                c.resize(end, 0);
            }
            c[off as usize..end].copy_from_slice(slice);
        });
    }
    let f: unsafe extern "system" fn(*mut c_void, u32, u32, *const u32, u32) =
        unsafe { core::mem::transmute(PREV_GCONSTS.load(Ordering::Relaxed)) };
    unsafe { f(this, idx, n, pdata, off) };
}

macro_rules! hook_va {
    ($name:ident, $compute:expr, $prev:ident) => {
        unsafe extern "system" fn $name(this: *mut c_void, idx: u32, va: u64) {
            update_pipe(this, $compute, |p| {
                if (idx as usize) < p.vas.len() {
                    p.vas[idx as usize] = va;
                }
            });
            let f: unsafe extern "system" fn(*mut c_void, u32, u64) =
                unsafe { core::mem::transmute($prev.load(Ordering::Relaxed)) };
            unsafe { f(this, idx, va) };
        }
    };
}

hook_va!(h_ccbv, true, PREV_CCBV);
hook_va!(h_gcbv, false, PREV_GCBV);
hook_va!(h_csrv, true, PREV_CSRV);
hook_va!(h_gsrv, false, PREV_GSRV);
hook_va!(h_cuav, true, PREV_CUAV);
hook_va!(h_guav, false, PREV_GUAV);

fn hook_slot(vt: &[AtomicUsize], slot: usize, prev: &AtomicUsize, f: usize) {
    if slot < vt.len() {
        prev.store(vt[slot].load(Ordering::Relaxed), Ordering::Relaxed);
        vt[slot].store(f, Ordering::Relaxed);
    }
}

pub fn install(vt: &[AtomicUsize]) {
    hook_slot(vt, XBOX_SLOT_DRAW, &PREV_DRAW, h_draw as usize);
    hook_slot(vt, XBOX_SLOT_DRAW_INDEXED, &PREV_DRAW_IDX, h_draw_idx as usize);
    hook_slot(vt, XBOX_SLOT_DISPATCH, &PREV_DISPATCH, h_dispatch as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_RS, &PREV_CROOT, h_croot as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_RS, &PREV_GROOT, h_groot as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_CONST, &PREV_CCONST, h_cconst as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_CONST, &PREV_GCONST, h_gconst as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_CONSTS, &PREV_CCONSTS, h_cconsts as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_CONSTS, &PREV_GCONSTS, h_gconsts as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_CBV, &PREV_CCBV, h_ccbv as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_CBV, &PREV_GCBV, h_gcbv as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_SRV, &PREV_CSRV, h_csrv as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_SRV, &PREV_GSRV, h_gsrv as usize);
    hook_slot(vt, XBOX_SLOT_SET_COMPUTE_UAV, &PREV_CUAV, h_cuav as usize);
    hook_slot(vt, XBOX_SLOT_SET_GRAPHICS_UAV, &PREV_GUAV, h_guav as usize);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_plan_adds_cbv_and_uavs_for_tb_global() {
        let params = vec![0u8; PARAM_SIZE];
        let (out, idx) = append_plan(&params, 0).expect("plan");
        assert_eq!(idx, 1);
        assert_eq!(out.len(), PARAM_SIZE * 5);
        assert_eq!(u32_at(&out, PARAM_SIZE), crate::gds::PARAM_CONSTANTS);
        assert_eq!(u32_at(&out, PARAM_SIZE + 12), TB_GLOBAL_SPACE);
        assert_eq!(u32_at(&out, PARAM_SIZE + 16), TB_GLOBAL_CB_DWORDS as u32);
        assert_eq!(u32_at(&out, PARAM_SIZE * 2), crate::gds::PARAM_UAV);
        assert_eq!(u32_at(&out, PARAM_SIZE * 4 + 8), TB_GLOBAL_FAULT_UAV);
    }

    #[test]
    fn global_memory_shader_is_no_longer_refused() {
        assert_eq!(super::refusal(false), None);
        assert_eq!(super::refusal(true), None);
    }
}
