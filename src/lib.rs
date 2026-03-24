#![allow(non_snake_case)]

mod dbc;
mod lua;
mod mpq;
mod offsets;
mod scripts;

use minhook::MinHook;
use std::sync::OnceLock;

const DLL_PROCESS_ATTACH: u32 = 1;

type LoadScriptFunctionsT = unsafe extern "stdcall" fn();

static ORIG_PLAYER_LOAD: OnceLock<LoadScriptFunctionsT> = OnceLock::new();
static ORIG_GLUE_LOAD:   OnceLock<LoadScriptFunctionsT> = OnceLock::new();

unsafe fn register_rq_functions() {
    // populated in Task 9
}

unsafe extern "stdcall" fn player_load_hook() {
    if let Some(orig) = ORIG_PLAYER_LOAD.get() { orig(); }
    register_rq_functions();
}

unsafe extern "stdcall" fn glue_load_hook() {
    if let Some(orig) = ORIG_GLUE_LOAD.get() { orig(); }
    register_rq_functions();
}

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

#[no_mangle]
pub extern "C" fn Load() -> u32 {
    unsafe { let _ = MinHook::enable_all_hooks(); }
    0
}
