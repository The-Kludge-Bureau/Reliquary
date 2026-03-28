use crate::dbc::{self, FieldValue};
use crate::lua::{self, LuaState};

// SAFETY: lua_error (called indirectly via lua::lua_error) triggers a longjmp
// back to the Lua protected-call boundary. This unwinds Rust stack frames
// without running destructors. All calls to lua::lua_error in this file must
// occur before any heap-allocated locals (String, Vec, MutexGuard, etc.) are
// created in the same scope. Violating this causes silent memory leaks or
// use-after-free in release builds.

const VERSION_MAJOR: u32 = 0;
const VERSION_MINOR: u32 = 2;
const VERSION_PATCH: u32 = 0;

const LOCALES: &[&str] = &[
    "enUS", "koKR", "frFR", "deDE", "zhCN", "ruRU", "esES", "ptPT",
];

/// Pushes a FieldValue onto the Lua stack.
unsafe fn push_field(l: LuaState, value: &FieldValue) {
    match value {
        FieldValue::Int32(n)   => lua::lua_pushnumber(l, *n as f64),
        FieldValue::UInt32(n)  => lua::lua_pushnumber(l, *n as f64),
        FieldValue::Float32(f) => lua::lua_pushnumber(l, *f as f64),
        FieldValue::String(s)  => lua::lua_pushstring(l, s),
    }
}

/// Returns the field name to expose for a given schema field, applying locale filtering.
/// locale: None = enUS default (strip suffix), Some("all") = keep full name,
/// Some(loc) = only include that locale, strip suffix.
///
/// Returns None if the field should be skipped (wrong locale variant).
fn localized_key<'a>(
    field_name: &'a str,
    locale: Option<&str>,
) -> Option<&'a str> {
    // Check if this field is a localized string variant
    for loc in LOCALES {
        if let Some(base) = field_name.strip_suffix(&format!("_{}", loc)) {
            return match locale {
                None => {
                    if *loc == "enUS" { Some(base) } else { None }
                }
                Some("all") => Some(field_name),
                Some(wanted) => {
                    if *loc == wanted { Some(base) } else { None }
                }
            };
        }
    }
    // Non-localized field: always include
    Some(field_name)
}

/// Builds a Lua table from a row, applying locale filtering.
/// The table has both named keys and integer (positional) keys.
unsafe fn push_row_table(
    l: LuaState,
    schema: crate::dbc::Schema,
    fields: &[FieldValue],
    locale: Option<&str>,
) {
    lua::lua_newtable(l);
    let mut pos: i32 = 1;
    for (i, (field_name, _)) in schema.iter().enumerate() {
        let key = match localized_key(field_name, locale) {
            Some(k) => k,
            None => continue,
        };
        // Named key
        lua::lua_pushstring(l, key);
        push_field(l, &fields[i]);
        lua::lua_settable(l, -3);
        // Positional key
        push_field(l, &fields[i]);
        lua::lua_rawseti(l, -2, pos);
        pos += 1;
    }
}

pub unsafe extern "fastcall" fn script_rq_get_version(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    lua::lua_pushnumber(l, VERSION_MAJOR as f64);
    lua::lua_pushnumber(l, VERSION_MINOR as f64);
    lua::lua_pushnumber(l, VERSION_PATCH as f64);
    3
}

pub unsafe extern "fastcall" fn script_rq_get_row(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    let argc = lua::lua_gettop(l);

    if argc < 2 || !lua::lua_isstring(l, 1) || !lua::lua_isnumber(l, 2) {
        lua::lua_error(l, "Usage: RQ_GetRow(dbc_name, id [, locale])");
        return 0;
    }

    let dbc_name = lua::lua_tostring(l, 1)
        .expect("lua_isstring guard ensures this is a string");

    let id = lua::lua_tonumber(l, 2) as u32;

    let locale: Option<String> = if argc >= 3 && lua::lua_isstring(l, 3) {
        lua::lua_tostring(l, 3)
    } else {
        None
    };

    // We need a &'static str key for the store. Look up via KNOWN_DBC_NAMES.
    let static_name: Option<&'static str> = dbc::KNOWN_DBC_NAMES
        .iter()
        .find(|&&n| n == dbc_name.as_str())
        .copied();

    let name = match static_name {
        Some(n) => n,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRow: unknown DBC '{}'", dbc_name));
            return 2;
        }
    };

    let schema = match dbc::get_schema(name) {
        Some(s) => s,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRow: unknown DBC '{}'", name));
            return 2;
        }
    };

    match dbc::get_record(name, id) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRow: {}", e));
            2
        }
        Ok(None) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!(
                "RQ_GetRow: no record with id {} in '{}'", id, name
            ));
            2
        }
        Ok(Some(fields)) => {
            push_row_table(l, schema, &fields, locale.as_deref());
            1
        }
    }
}

pub unsafe extern "fastcall" fn script_rq_get_rows(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    let argc = lua::lua_gettop(l);

    if argc < 1 || !lua::lua_isstring(l, 1) {
        lua::lua_error(l, "Usage: RQ_GetRows(dbc_name [, locale])");
        return 0;
    }

    let dbc_name = lua::lua_tostring(l, 1)
        .expect("lua_isstring guard ensures this is a string");

    let locale: Option<String> = if argc >= 2 && lua::lua_isstring(l, 2) {
        lua::lua_tostring(l, 2)
    } else {
        None
    };

    let static_name: Option<&'static str> = dbc::KNOWN_DBC_NAMES
        .iter()
        .find(|&&n| n == dbc_name.as_str())
        .copied();

    let name = match static_name {
        Some(n) => n,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRows: unknown DBC '{}'", dbc_name));
            return 2;
        }
    };

    let schema = match dbc::get_schema(name) {
        Some(s) => s,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRows: unknown DBC '{}'", name));
            return 2;
        }
    };

    match dbc::get_all_records(name) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRows: {}", e));
            2
        }
        Ok(all_rows) => {
            lua::lua_newtable(l);
            for (i, fields) in all_rows.iter().enumerate() {
                push_row_table(l, schema, fields, locale.as_deref());
                lua::lua_rawseti(l, -2, (i + 1) as i32);
            }
            1
        }
    }
}

pub unsafe extern "fastcall" fn script_rq_get_row_count(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();

    if lua::lua_gettop(l) != 1 || !lua::lua_isstring(l, 1) {
        lua::lua_error(l, "Usage: RQ_GetRowCount(dbc_name)");
        return 0;
    }

    let dbc_name = lua::lua_tostring(l, 1)
        .expect("lua_isstring guard ensures this is a string");

    let static_name: Option<&'static str> = dbc::KNOWN_DBC_NAMES
        .iter()
        .find(|&&n| n == dbc_name.as_str())
        .copied();

    let name = match static_name {
        Some(n) => n,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRowCount: unknown DBC '{}'", dbc_name));
            return 2;
        }
    };

    match dbc::get_record_count(name) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRowCount: {}", e));
            2
        }
        Ok(count) => {
            lua::lua_pushnumber(l, count as f64);
            1
        }
    }
}

pub unsafe extern "fastcall" fn script_rq_get_row_by_index(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    let argc = lua::lua_gettop(l);

    if argc < 2 || !lua::lua_isstring(l, 1) || !lua::lua_isnumber(l, 2) {
        lua::lua_error(l, "Usage: RQ_GetRowByIndex(dbc_name, index [, locale])");
        return 0;
    }

    let dbc_name = lua::lua_tostring(l, 1)
        .expect("lua_isstring guard");

    let index = lua::lua_tonumber(l, 2) as usize;

    let locale: Option<String> = if argc >= 3 && lua::lua_isstring(l, 3) {
        lua::lua_tostring(l, 3)
    } else {
        None
    };

    let static_name: Option<&'static str> = dbc::KNOWN_DBC_NAMES
        .iter()
        .find(|&&n| n == dbc_name.as_str())
        .copied();

    let name = match static_name {
        Some(n) => n,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRowByIndex: unknown DBC '{}'", dbc_name));
            return 2;
        }
    };

    let schema = match dbc::get_schema(name) {
        Some(s) => s,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRowByIndex: unknown DBC '{}'", name));
            return 2;
        }
    };

    match dbc::get_record_by_index(name, index) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetRowByIndex: {}", e));
            2
        }
        Ok(None) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!(
                "RQ_GetRowByIndex: index {} out of range for '{}'", index, name
            ));
            2
        }
        Ok(Some(fields)) => {
            push_row_table(l, schema, &fields, locale.as_deref());
            1
        }
    }
}

macro_rules! typed_lookup {
    ($fn_name:ident, $dbc_name:literal) => {
        pub unsafe extern "fastcall" fn $fn_name(_l: LuaState) -> u32 {
            let l = lua::get_lua_state();
            if lua::lua_gettop(l) != 1 || !lua::lua_isnumber(l, 1) {
                lua::lua_error(l, concat!("Usage: ", stringify!($fn_name), "(id)"));
                return 0;
            }
            let id = lua::lua_tonumber(l, 1) as u32;
            let schema = match dbc::get_schema($dbc_name) {
                Some(s) => s,
                None => {
                    lua::lua_pushnil(l);
                    lua::lua_pushstring(l, concat!("internal error: schema missing for ", $dbc_name));
                    return 2;
                }
            };
            match dbc::get_record($dbc_name, id) {
                Err(e) => {
                    lua::lua_pushnil(l);
                    lua::lua_pushstring(l, &format!("{}: {}", stringify!($fn_name), e));
                    2
                }
                Ok(None) => {
                    lua::lua_pushnil(l);
                    lua::lua_pushstring(l, &format!(
                        "{}: no record with id {}", stringify!($fn_name), id
                    ));
                    2
                }
                Ok(Some(fields)) => {
                    push_row_table(l, schema, &fields, None);
                    1
                }
            }
        }
    };
}

pub unsafe extern "fastcall" fn script_rq_find_row(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    let argc = lua::lua_gettop(l);

    if argc < 3 || !lua::lua_isstring(l, 1) || !lua::lua_isstring(l, 2) {
        lua::lua_error(l, "Usage: RQ_FindRow(dbc_name, field, value [, locale])");
        return 0;
    }

    let dbc_name = lua::lua_tostring(l, 1)
        .expect("lua_isstring guard");
    let field_name = lua::lua_tostring(l, 2)
        .expect("lua_isstring guard");

    let locale: Option<String> = if argc >= 4 && lua::lua_isstring(l, 4) {
        lua::lua_tostring(l, 4)
    } else {
        None
    };

    let static_name: Option<&'static str> = dbc::KNOWN_DBC_NAMES
        .iter()
        .find(|&&n| n == dbc_name.as_str())
        .copied();

    let name = match static_name {
        Some(n) => n,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_FindRow: unknown DBC '{}'", dbc_name));
            return 2;
        }
    };

    let schema = match dbc::get_schema(name) {
        Some(s) => s,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_FindRow: unknown DBC '{}'", name));
            return 2;
        }
    };

    // Resolve field: try exact match first, then try appending _enUS for locale-stripped names
    let resolved_field = field_name.as_str();
    let col = schema.iter().find(|(n, _)| {
        *n == resolved_field
            || n.strip_suffix("_enUS") == Some(resolved_field)
    });

    let (raw_field_name, field_type) = match col {
        Some((n, t)) => (*n, t),
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!(
                "RQ_FindRow: no field '{}' in DBC '{}'", field_name, name
            ));
            return 2;
        }
    };

    // Build the target FieldValue typed by the schema
    let target = match field_type {
        dbc::FieldType::Int32 => {
            if !lua::lua_isnumber(l, 3) {
                lua::lua_pushnil(l);
                lua::lua_pushstring(l, "RQ_FindRow: value must be a number for Int32 field");
                return 2;
            }
            dbc::FieldValue::Int32(lua::lua_tonumber(l, 3) as i32)
        }
        dbc::FieldType::UInt32 => {
            if !lua::lua_isnumber(l, 3) {
                lua::lua_pushnil(l);
                lua::lua_pushstring(l, "RQ_FindRow: value must be a number for UInt32 field");
                return 2;
            }
            dbc::FieldValue::UInt32(lua::lua_tonumber(l, 3) as u32)
        }
        dbc::FieldType::Float32 => {
            if !lua::lua_isnumber(l, 3) {
                lua::lua_pushnil(l);
                lua::lua_pushstring(l, "RQ_FindRow: value must be a number for Float32 field");
                return 2;
            }
            dbc::FieldValue::Float32(lua::lua_tonumber(l, 3) as f32)
        }
        dbc::FieldType::String => {
            let s = match lua::lua_tostring(l, 3) {
                Some(s) => s,
                None => {
                    lua::lua_pushnil(l);
                    lua::lua_pushstring(l, "RQ_FindRow: value must be a string for String field");
                    return 2;
                }
            };
            dbc::FieldValue::String(s)
        }
    };

    match dbc::find_records(name, raw_field_name, &target) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_FindRow: {}", e));
            2
        }
        Ok(matching_rows) => {
            lua::lua_newtable(l);
            for (i, fields) in matching_rows.iter().enumerate() {
                push_row_table(l, schema, fields, locale.as_deref());
                lua::lua_rawseti(l, -2, (i + 1) as i32);
            }
            1
        }
    }
}

typed_lookup!(script_rq_get_area_table,           "AreaTable");
typed_lookup!(script_rq_get_area_trigger,          "AreaTrigger");
typed_lookup!(script_rq_get_char_start_outfit,     "CharStartOutfit");
typed_lookup!(script_rq_get_chr_classes,           "ChrClasses");
typed_lookup!(script_rq_get_chr_races,             "ChrRaces");
typed_lookup!(script_rq_get_creature_family,       "CreatureFamily");
typed_lookup!(script_rq_get_creature_type,         "CreatureType");
typed_lookup!(script_rq_get_faction,               "Faction");
typed_lookup!(script_rq_get_faction_template,      "FactionTemplate");
typed_lookup!(script_rq_get_item_bag_family,       "ItemBagFamily");
typed_lookup!(script_rq_get_item_class,            "ItemClass");
typed_lookup!(script_rq_get_item_display_info,     "ItemDisplayInfo");
typed_lookup!(script_rq_get_item_random_properties,"ItemRandomProperties");
typed_lookup!(script_rq_get_item_set,              "ItemSet");
typed_lookup!(script_rq_get_lfg_dungeons,          "LFGDungeons");
typed_lookup!(script_rq_get_lock,                  "Lock");
typed_lookup!(script_rq_get_lock_type,             "LockType");
typed_lookup!(script_rq_get_mail_template,         "MailTemplate");
typed_lookup!(script_rq_get_map,                   "Map");
typed_lookup!(script_rq_get_quest_info,            "QuestInfo");
typed_lookup!(script_rq_get_quest_sort,            "QuestSort");
typed_lookup!(script_rq_get_skill_line,            "SkillLine");
typed_lookup!(script_rq_get_skill_line_ability,    "SkillLineAbility");
typed_lookup!(script_rq_get_skill_line_category,   "SkillLineCategory");
typed_lookup!(script_rq_get_spell,                 "Spell");
typed_lookup!(script_rq_get_spell_cast_times,      "SpellCastTimes");
typed_lookup!(script_rq_get_spell_category,        "SpellCategory");
typed_lookup!(script_rq_get_spell_dispel_type,     "SpellDispelType");
typed_lookup!(script_rq_get_spell_duration,        "SpellDuration");
typed_lookup!(script_rq_get_spell_icon,            "SpellIcon");
typed_lookup!(script_rq_get_spell_item_enchantment,"SpellItemEnchantment");
typed_lookup!(script_rq_get_spell_mechanic,        "SpellMechanic");
typed_lookup!(script_rq_get_spell_radius,          "SpellRadius");
typed_lookup!(script_rq_get_spell_range,           "SpellRange");
typed_lookup!(script_rq_get_spell_shapeshift_form, "SpellShapeshiftForm");
typed_lookup!(script_rq_get_talent,                "Talent");
typed_lookup!(script_rq_get_talent_tab,            "TalentTab");
typed_lookup!(script_rq_get_taxi_nodes,            "TaxiNodes");
typed_lookup!(script_rq_get_taxi_path,             "TaxiPath");
typed_lookup!(script_rq_get_taxi_path_node,        "TaxiPathNode");
typed_lookup!(script_rq_get_world_map_area,        "WorldMapArea");
typed_lookup!(script_rq_get_world_safe_locs,       "WorldSafeLocs");

pub unsafe extern "fastcall" fn script_rq_get_item_sub_class(_l: LuaState) -> u32 {
    let l = lua::get_lua_state();
    if lua::lua_gettop(l) != 2 || !lua::lua_isnumber(l, 1) || !lua::lua_isnumber(l, 2) {
        lua::lua_error(l, "Usage: RQ_GetItemSubClass(classId, subclassId)");
        return 0;
    }
    let class_id    = lua::lua_tonumber(l, 1) as u32;
    let subclass_id = lua::lua_tonumber(l, 2) as u32;
    if class_id > 4294 || subclass_id > 4294967 {
        lua::lua_pushnil(l);
        lua::lua_pushstring(l, "RQ_GetItemSubClass: classId or subclassId out of range");
        return 2;
    }
    let key = class_id * 1000 + subclass_id;

    let schema = match dbc::get_schema("ItemSubClass") {
        Some(s) => s,
        None => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, "internal error: schema missing for ItemSubClass");
            return 2;
        }
    };
    match dbc::get_record_composite("ItemSubClass", key) {
        Err(e) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!("RQ_GetItemSubClass: {}", e));
            2
        }
        Ok(None) => {
            lua::lua_pushnil(l);
            lua::lua_pushstring(l, &format!(
                "RQ_GetItemSubClass: no record for classId={} subclassId={}",
                class_id, subclass_id
            ));
            2
        }
        Ok(Some(fields)) => {
            push_row_table(l, schema, &fields, None);
            1
        }
    }
}
