#![allow(non_snake_case)]

mod dbc;
mod mpq;
#[cfg(windows)]
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
    use scripts::*;

    dbc::init_schema_registry();
    dbc::init_mpq_list();

    lua::register_lua_function("RQ_GetVersion",            script_rq_get_version            as *mut usize);
    lua::register_lua_function("RQ_GetRow",                script_rq_get_row                as *mut usize);
    lua::register_lua_function("RQ_GetAreaTable",          script_rq_get_area_table         as *mut usize);
    lua::register_lua_function("RQ_GetAreaTrigger",        script_rq_get_area_trigger       as *mut usize);
    lua::register_lua_function("RQ_GetCharStartOutfit",    script_rq_get_char_start_outfit  as *mut usize);
    lua::register_lua_function("RQ_GetChrClasses",         script_rq_get_chr_classes        as *mut usize);
    lua::register_lua_function("RQ_GetChrRaces",           script_rq_get_chr_races          as *mut usize);
    lua::register_lua_function("RQ_GetCreatureFamily",     script_rq_get_creature_family    as *mut usize);
    lua::register_lua_function("RQ_GetCreatureType",       script_rq_get_creature_type      as *mut usize);
    lua::register_lua_function("RQ_GetFaction",            script_rq_get_faction            as *mut usize);
    lua::register_lua_function("RQ_GetFactionTemplate",    script_rq_get_faction_template   as *mut usize);
    lua::register_lua_function("RQ_GetItemBagFamily",      script_rq_get_item_bag_family    as *mut usize);
    lua::register_lua_function("RQ_GetItemClass",          script_rq_get_item_class         as *mut usize);
    lua::register_lua_function("RQ_GetItemDisplayInfo",    script_rq_get_item_display_info  as *mut usize);
    lua::register_lua_function("RQ_GetItemRandomProperties", script_rq_get_item_random_properties as *mut usize);
    lua::register_lua_function("RQ_GetItemSet",            script_rq_get_item_set           as *mut usize);
    lua::register_lua_function("RQ_GetItemSubClass",       script_rq_get_item_sub_class     as *mut usize);
    lua::register_lua_function("RQ_GetLFGDungeons",        script_rq_get_lfg_dungeons       as *mut usize);
    lua::register_lua_function("RQ_GetLock",               script_rq_get_lock               as *mut usize);
    lua::register_lua_function("RQ_GetLockType",           script_rq_get_lock_type          as *mut usize);
    lua::register_lua_function("RQ_GetMailTemplate",       script_rq_get_mail_template      as *mut usize);
    lua::register_lua_function("RQ_GetMap",                script_rq_get_map                as *mut usize);
    lua::register_lua_function("RQ_GetQuestInfo",          script_rq_get_quest_info         as *mut usize);
    lua::register_lua_function("RQ_GetQuestSort",          script_rq_get_quest_sort         as *mut usize);
    lua::register_lua_function("RQ_GetSkillLine",          script_rq_get_skill_line         as *mut usize);
    lua::register_lua_function("RQ_GetSkillLineAbility",   script_rq_get_skill_line_ability as *mut usize);
    lua::register_lua_function("RQ_GetSkillLineCategory",  script_rq_get_skill_line_category as *mut usize);
    lua::register_lua_function("RQ_GetSpell",              script_rq_get_spell              as *mut usize);
    lua::register_lua_function("RQ_GetSpellCastTimes",     script_rq_get_spell_cast_times   as *mut usize);
    lua::register_lua_function("RQ_GetSpellCategory",      script_rq_get_spell_category     as *mut usize);
    lua::register_lua_function("RQ_GetSpellDispelType",    script_rq_get_spell_dispel_type  as *mut usize);
    lua::register_lua_function("RQ_GetSpellDuration",      script_rq_get_spell_duration     as *mut usize);
    lua::register_lua_function("RQ_GetSpellIcon",          script_rq_get_spell_icon         as *mut usize);
    lua::register_lua_function("RQ_GetSpellItemEnchantment", script_rq_get_spell_item_enchantment as *mut usize);
    lua::register_lua_function("RQ_GetSpellMechanic",      script_rq_get_spell_mechanic     as *mut usize);
    lua::register_lua_function("RQ_GetSpellRadius",        script_rq_get_spell_radius       as *mut usize);
    lua::register_lua_function("RQ_GetSpellRange",         script_rq_get_spell_range        as *mut usize);
    lua::register_lua_function("RQ_GetSpellShapeshiftForm", script_rq_get_spell_shapeshift_form as *mut usize);
    lua::register_lua_function("RQ_GetTalent",             script_rq_get_talent             as *mut usize);
    lua::register_lua_function("RQ_GetTalentTab",          script_rq_get_talent_tab         as *mut usize);
    lua::register_lua_function("RQ_GetTaxiNodes",          script_rq_get_taxi_nodes         as *mut usize);
    lua::register_lua_function("RQ_GetTaxiPath",           script_rq_get_taxi_path          as *mut usize);
    lua::register_lua_function("RQ_GetTaxiPathNode",       script_rq_get_taxi_path_node     as *mut usize);
    lua::register_lua_function("RQ_GetWorldMapArea",       script_rq_get_world_map_area     as *mut usize);
    lua::register_lua_function("RQ_GetWorldSafeLocs",      script_rq_get_world_safe_locs    as *mut usize);
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
