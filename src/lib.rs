#![allow(non_snake_case)]

mod dbc;
mod mpq;
mod scripts;

#[cfg(windows)]
mod lua;
#[cfg(windows)]
mod offsets;

#[cfg(windows)]
use minhook::MinHook;
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
const DLL_PROCESS_ATTACH: u32 = 1;

#[cfg(windows)]
type LoadScriptFunctionsT = unsafe extern "stdcall" fn();

#[cfg(windows)]
static ORIG_PLAYER_LOAD: OnceLock<LoadScriptFunctionsT> = OnceLock::new();
#[cfg(windows)]
static ORIG_GLUE_LOAD:   OnceLock<LoadScriptFunctionsT> = OnceLock::new();

#[cfg(windows)]
unsafe fn register_rq_functions() {
    // populated in Task 9
}

#[cfg(windows)]
unsafe extern "stdcall" fn player_load_hook() {
    if let Some(orig) = ORIG_PLAYER_LOAD.get() { orig(); }
    register_rq_functions();
}

#[cfg(windows)]
unsafe extern "stdcall" fn glue_load_hook() {
    if let Some(orig) = ORIG_GLUE_LOAD.get() { orig(); }
    register_rq_functions();
}

#[cfg(windows)]
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    _hinstance: *mut std::ffi::c_void,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        if let Ok(orig) = MinHook::create_hook(
            offsets::PLAYER_LOAD_SCRIPT_FUNCTIONS as *mut _,
            player_load_hook as *mut _,
        ) {
            let _ = ORIG_PLAYER_LOAD.set(std::mem::transmute(orig));
        }
        if let Ok(orig) = MinHook::create_hook(
            offsets::GLUE_LOAD_SCRIPT_FUNCTIONS as *mut _,
            glue_load_hook as *mut _,
        ) {
            let _ = ORIG_GLUE_LOAD.set(std::mem::transmute(orig));
        }
    }
    1
}

#[cfg(windows)]
#[no_mangle]
pub extern "C" fn Load() -> u32 {
    unsafe { let _ = MinHook::enable_all_hooks(); }
    0
}
