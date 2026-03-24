use crate::dbc::{self, FieldValue};
use crate::lua::{self, LuaState};

const VERSION_MAJOR: u32 = 0;
const VERSION_MINOR: u32 = 1;
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

    let dbc_name = match lua::lua_tostring(l, 1) {
        Some(s) => s,
        None => {
            lua::lua_error(l, "RQ_GetRow: dbc_name is nil");
            return 0;
        }
    };

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
