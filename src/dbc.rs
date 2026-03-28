use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Int32,
    UInt32,
    Float32,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Int32(i32),
    UInt32(u32),
    Float32(f32),
    String(String),
}

pub type Schema = &'static [(&'static str, FieldType)];
pub type DbcRows = HashMap<u32, Vec<FieldValue>>;

/// Parses a WDBC binary blob according to the given schema.
/// Returns a map from row ID (first field, interpreted as u32) to field values.
pub fn parse_wdbc(data: &[u8], schema: &[(&str, FieldType)]) -> Result<DbcRows, String> {
    if data.len() < 20 {
        return Err("DBC too short to contain header".into());
    }
    if &data[0..4] != b"WDBC" {
        return Err(format!("invalid DBC signature: {:?}", &data[0..4]));
    }

    let record_count      = u32::from_le_bytes(data[4..8].try_into().unwrap())   as usize;
    let field_count       = u32::from_le_bytes(data[8..12].try_into().unwrap())  as usize;
    let record_size       = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    let string_block_size = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;

    let records_start = 20;
    let records_end   = records_start + record_count * record_size;
    let string_start  = records_end;
    let string_end    = string_start + string_block_size;

    if data.len() < string_end {
        return Err(format!(
            "DBC data too short: expected {} bytes, got {}",
            string_end,
            data.len()
        ));
    }

    if record_size != field_count * 4 {
        return Err(format!(
            "record_size {} does not match field_count {} (expected {})",
            record_size, field_count, field_count * 4
        ));
    }

    if field_count == 0 {
        return Err("DBC has zero fields".into());
    }

    if schema.len() != field_count {
        return Err(format!(
            "schema has {} fields but DBC has {}",
            schema.len(),
            field_count
        ));
    }

    let string_block = &data[string_start..string_end];

    let mut rows = HashMap::with_capacity(record_count);

    for i in 0..record_count {
        let rec_start = records_start + i * record_size;
        let mut fields = Vec::with_capacity(field_count);

        for (j, (_name, ftype)) in schema.iter().enumerate() {
            let offset = rec_start + j * 4;
            let raw = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
            let value = match ftype {
                FieldType::Int32   => FieldValue::Int32(raw as i32),
                FieldType::UInt32  => FieldValue::UInt32(raw),
                FieldType::Float32 => FieldValue::Float32(f32::from_bits(raw)),
                FieldType::String  => {
                    let offset = raw as usize;
                    if offset >= string_block.len() {
                        return Err(format!(
                            "string offset {} is out of bounds (string block size {})",
                            offset, string_block.len()
                        ));
                    }
                    let end = string_block[offset..]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|p| offset + p)
                        .unwrap_or(string_block.len());
                    let s = std::str::from_utf8(&string_block[offset..end])
                        .unwrap_or("")
                        .to_string();
                    FieldValue::String(s)
                }
            };
            fields.push(value);
        }

        let id = match &fields[0] {
            FieldValue::Int32(n)  => *n as u32,
            FieldValue::UInt32(n) => *n,
            _ => i as u32,
        };
        // Use row index on collision so DBCs with non-unique first fields
        // (e.g. ItemSubClass, keyed by classId) preserve all rows for
        // the composite re-keying pass in get_record.
        let key = if rows.contains_key(&id) { i as u32 } else { id };
        rows.insert(key, fields);
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_dbc() -> Vec<u8> {
        // Header
        let mut data = b"WDBC".to_vec();
        data.extend_from_slice(&2u32.to_le_bytes()); // recordCount
        data.extend_from_slice(&3u32.to_le_bytes()); // fieldCount
        data.extend_from_slice(&12u32.to_le_bytes()); // recordSize (3 * 4)
        data.extend_from_slice(&13u32.to_le_bytes()); // stringBlockSize
        // Record 1: id=1, value=42, name_offset=1
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&42u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        // Record 2: id=2, value=99, name_offset=7
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&99u32.to_le_bytes());
        data.extend_from_slice(&7u32.to_le_bytes());
        // String block: \0Hello\0World\0
        data.extend_from_slice(b"\x00Hello\x00World\x00");
        data
    }

    #[test]
    fn test_parse_wdbc() {
        let schema: &[(&str, FieldType)] = &[
            ("id",    FieldType::UInt32),
            ("value", FieldType::Int32),
            ("name",  FieldType::String),
        ];
        let data = make_test_dbc();
        let result = parse_wdbc(&data, schema).unwrap();
        assert_eq!(result.len(), 2);

        let row1 = result.get(&1).unwrap();
        assert_eq!(row1[0], FieldValue::UInt32(1));
        assert_eq!(row1[1], FieldValue::Int32(42));
        assert_eq!(row1[2], FieldValue::String("Hello".into()));

        let row2 = result.get(&2).unwrap();
        assert_eq!(row2[2], FieldValue::String("World".into()));
    }

    #[test]
    fn test_parse_wdbc_bad_signature() {
        let mut data = b"NOPE".to_vec();
        data.extend_from_slice(&[0u8; 16]);
        let schema: &[(&str, FieldType)] = &[];
        assert!(parse_wdbc(&data, schema).is_err());
    }

    #[test]
    fn test_parse_wdbc_truncated() {
        let data = b"WDBC\x01\x00\x00\x00\x01\x00\x00\x00".to_vec(); // incomplete header
        let schema: &[(&str, FieldType)] = &[("id", FieldType::Int32)];
        assert!(parse_wdbc(&data, schema).is_err());
    }

    #[test]
    fn test_parse_wdbc_bad_string_offset() {
        // One record, one string field. The string offset points past the end of the string block.
        let mut data = b"WDBC".to_vec();
        data.extend_from_slice(&1u32.to_le_bytes()); // record_count=1
        data.extend_from_slice(&1u32.to_le_bytes()); // field_count=1
        data.extend_from_slice(&4u32.to_le_bytes()); // record_size=4
        data.extend_from_slice(&4u32.to_le_bytes()); // string_block_size=4
        // Record: string offset = 999 (past end of 4-byte string block)
        data.extend_from_slice(&999u32.to_le_bytes());
        // String block: 4 bytes
        data.extend_from_slice(b"\x00hi\x00");
        let schema: &[(&str, FieldType)] = &[("name", FieldType::String)];
        assert!(parse_wdbc(&data, schema).is_err());
    }

    #[test]
    fn test_parse_wdbc_bad_record_size() {
        // Header claims record_size=8 but field_count=1 (expected 4).
        let mut data = b"WDBC".to_vec();
        data.extend_from_slice(&1u32.to_le_bytes()); // record_count=1
        data.extend_from_slice(&1u32.to_le_bytes()); // field_count=1
        data.extend_from_slice(&8u32.to_le_bytes()); // record_size=8 (mismatch: should be 4)
        data.extend_from_slice(&1u32.to_le_bytes()); // string_block_size=1
        // Pad to satisfy the total length check (20 + 1*8 + 1 = 29 bytes)
        data.extend_from_slice(&[0u8; 9]);
        let schema: &[(&str, FieldType)] = &[("id", FieldType::UInt32)];
        assert!(parse_wdbc(&data, schema).is_err());
    }

    #[test]
    fn test_parse_wdbc_zero_fields() {
        // field_count=0 but record_count=1. record_size must equal field_count*4=0.
        let mut data = b"WDBC".to_vec();
        data.extend_from_slice(&1u32.to_le_bytes()); // record_count=1
        data.extend_from_slice(&0u32.to_le_bytes()); // field_count=0
        data.extend_from_slice(&0u32.to_le_bytes()); // record_size=0
        data.extend_from_slice(&1u32.to_le_bytes()); // string_block_size=1
        // String block: 1 byte
        data.extend_from_slice(b"\x00");
        let schema: &[(&str, FieldType)] = &[];
        assert!(parse_wdbc(&data, schema).is_err());
    }
}

use std::sync::OnceLock;

static SCHEMA_REGISTRY: OnceLock<HashMap<&'static str, Schema>> = OnceLock::new();

pub fn get_schema(name: &str) -> Option<Schema> {
    SCHEMA_REGISTRY.get()?.get(name).copied()
}

pub fn init_schema_registry() {
    SCHEMA_REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("AreaTable",         AREA_TABLE_SCHEMA);
        m.insert("AreaTrigger",       AREA_TRIGGER_SCHEMA);
        m.insert("CharStartOutfit",   CHAR_START_OUTFIT_SCHEMA);
        m.insert("ChrClasses",        CHR_CLASSES_SCHEMA);
        m.insert("ChrRaces",          CHR_RACES_SCHEMA);
        m.insert("CreatureFamily",    CREATURE_FAMILY_SCHEMA);
        m.insert("CreatureType",      CREATURE_TYPE_SCHEMA);
        m.insert("Faction",           FACTION_SCHEMA);
        m.insert("FactionTemplate",   FACTION_TEMPLATE_SCHEMA);
        m.insert("ItemBagFamily",     ITEM_BAG_FAMILY_SCHEMA);
        m.insert("ItemClass",         ITEM_CLASS_SCHEMA);
        m.insert("ItemDisplayInfo",   ITEM_DISPLAY_INFO_SCHEMA);
        m.insert("ItemRandomProperties", ITEM_RANDOM_PROPERTIES_SCHEMA);
        m.insert("ItemSet",           ITEM_SET_SCHEMA);
        m.insert("ItemSubClass",      ITEM_SUB_CLASS_SCHEMA);
        m.insert("LFGDungeons",       LFG_DUNGEONS_SCHEMA);
        m.insert("Lock",              LOCK_SCHEMA);
        m.insert("LockType",          LOCK_TYPE_SCHEMA);
        m.insert("MailTemplate",      MAIL_TEMPLATE_SCHEMA);
        m.insert("Map",               MAP_SCHEMA);
        m.insert("QuestInfo",         QUEST_INFO_SCHEMA);
        m.insert("QuestSort",         QUEST_SORT_SCHEMA);
        m.insert("SkillLine",         SKILL_LINE_SCHEMA);
        m.insert("SkillLineAbility",  SKILL_LINE_ABILITY_SCHEMA);
        m.insert("SkillLineCategory", SKILL_LINE_CATEGORY_SCHEMA);
        m.insert("Spell",             SPELL_SCHEMA);
        m.insert("SpellCastTimes",    SPELL_CAST_TIMES_SCHEMA);
        m.insert("SpellCategory",     SPELL_CATEGORY_SCHEMA);
        m.insert("SpellDispelType",   SPELL_DISPEL_TYPE_SCHEMA);
        m.insert("SpellDuration",     SPELL_DURATION_SCHEMA);
        m.insert("SpellIcon",         SPELL_ICON_SCHEMA);
        m.insert("SpellItemEnchantment", SPELL_ITEM_ENCHANTMENT_SCHEMA);
        m.insert("SpellMechanic",     SPELL_MECHANIC_SCHEMA);
        m.insert("SpellRadius",       SPELL_RADIUS_SCHEMA);
        m.insert("SpellRange",        SPELL_RANGE_SCHEMA);
        m.insert("SpellShapeshiftForm", SPELL_SHAPESHIFT_FORM_SCHEMA);
        m.insert("Talent",            TALENT_SCHEMA);
        m.insert("TalentTab",         TALENT_TAB_SCHEMA);
        m.insert("TaxiNodes",         TAXI_NODES_SCHEMA);
        m.insert("TaxiPath",          TAXI_PATH_SCHEMA);
        m.insert("TaxiPathNode",      TAXI_PATH_NODE_SCHEMA);
        m.insert("WorldMapArea",      WORLD_MAP_AREA_SCHEMA);
        m.insert("WorldSafeLocs",     WORLD_SAFE_LOCS_SCHEMA);
        m
    });
}

#[allow(unused)]
use FieldType::{Float32 as F, Int32 as I, String as S, UInt32 as U};

pub static AREA_TABLE_SCHEMA: Schema = &[
    ("id",                           I),
    ("mapId",                        I),
    ("parentAreaId",                 I),
    ("areaBit",                      I),
    ("flags",                        I),
    ("soundPreferenceId",            I),
    ("underwaterSoundPreferenceId",  I),
    ("soundAmbienceId",              I),
    ("zoneMusicId",                  I),
    ("zoneIntroMusicId",             I),
    ("explorationLevel",             I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",                     I),
    ("factionGroupMask",             I),
    ("liquidTypeId",                 I),
    ("minElevation",                 F),
    ("ambientMultiplier",            F),
    ("lightId",                      I),
];

pub static AREA_TRIGGER_SCHEMA: Schema = &[
    ("id",        I),
    ("mapId",     I),
    ("x",         F),
    ("y",         F),
    ("z",         F),
    ("radius",    F),
    ("boxLength", F),
    ("boxWidth",  F),
    ("boxHeight", F),
    ("boxYaw",    F),
];

pub static CHAR_START_OUTFIT_SCHEMA: Schema = &[
    ("id",                  I),
    ("itemId_1",    I), ("itemId_2",    I), ("itemId_3",    I), ("itemId_4",  I),
    ("itemId_5",    I), ("itemId_6",    I), ("itemId_7",    I), ("itemId_8",  I),
    ("itemId_9",    I), ("itemId_10",   I), ("itemId_11",   I), ("itemId_12", I),
    ("displayItemId_1",  I), ("displayItemId_2",  I), ("displayItemId_3",  I),
    ("displayItemId_4",  I), ("displayItemId_5",  I), ("displayItemId_6",  I),
    ("displayItemId_7",  I), ("displayItemId_8",  I), ("displayItemId_9",  I),
    ("displayItemId_10", I), ("displayItemId_11", I), ("displayItemId_12", I),
    ("inventoryType_1",  I), ("inventoryType_2",  I), ("inventoryType_3",  I),
    ("inventoryType_4",  I), ("inventoryType_5",  I), ("inventoryType_6",  I),
    ("inventoryType_7",  I), ("inventoryType_8",  I), ("inventoryType_9",  I),
    ("inventoryType_10", I), ("inventoryType_11", I), ("inventoryType_12", I),
];

pub static CHR_CLASSES_SCHEMA: Schema = &[
    ("id",            I),
    ("playerClass",   I),
    ("damageBonusStat", I),
    ("displayPower",  I),
    ("petNameToken",  S),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",      I),
    ("filename",      S),
    ("spellFamily",   I),
    ("flags",         I),
];

pub static CHR_RACES_SCHEMA: Schema = &[
    ("id",                    I),
    ("flags",                 I),
    ("factionId",             I),
    ("explorationSoundId",    I),
    ("maleDisplayId",         I),
    ("femaleDisplayId",       I),
    ("clientPrefix",          S),
    ("mountScale",            F),
    ("baseLanguage",          I),
    ("creatureType",          I),
    ("loginEffectSpellId",    I),
    ("combatStunSpellId",     I),
    ("resSicknessSpellId",    I),
    ("splashSoundId",         I),
    ("startingTaxiMask",      I),
    ("clientFileString",      S),
    ("cinematicSequenceId",   I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",              I),
    ("maleCustomization",     S),
    ("femaleCustomization",   S),
    ("hairCustomization",     S),
];

pub static CREATURE_FAMILY_SCHEMA: Schema = &[
    ("id",              I),
    ("minScale",        F),
    ("minScaleLevel",   I),
    ("maxScale",        F),
    ("maxScaleLevel",   I),
    ("skillLine_1",     I),
    ("skillLine_2",     I),
    ("petFoodMask",     I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",        I),
    ("iconFile",        S),
];

pub static CREATURE_TYPE_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
    ("flags", I),
];

pub static FACTION_SCHEMA: Schema = &[
    ("id",                I),
    ("reputationIdx",     I),
    ("repRaceMask_1",  I), ("repRaceMask_2",  I), ("repRaceMask_3",  I), ("repRaceMask_4",  I),
    ("repClassMask_1", I), ("repClassMask_2", I), ("repClassMask_3", I), ("repClassMask_4", I),
    ("repBase_1",      I), ("repBase_2",      I), ("repBase_3",      I), ("repBase_4",      I),
    ("repFlags_1",     I), ("repFlags_2",     I), ("repFlags_3",     I), ("repFlags_4",     I),
    ("parentFactionId",   I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",          I),
    ("description_enUS", S), ("description_koKR", S), ("description_frFR", S),
    ("description_deDE", S), ("description_zhCN", S), ("description_ruRU", S),
    ("description_esES", S), ("description_ptPT", S), ("descriptionMask", I),
];

pub static FACTION_TEMPLATE_SCHEMA: Schema = &[
    ("id",            I),
    ("faction",       I),
    ("flags",         I),
    ("factionGroup",  I),
    ("friendGroup",   I),
    ("enemyGroup",    I),
    ("enemy_1", I), ("enemy_2", I), ("enemy_3", I), ("enemy_4", I),
    ("friend_1", I), ("friend_2", I), ("friend_3", I), ("friend_4", I),
];

pub static ITEM_BAG_FAMILY_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
];

pub static ITEM_CLASS_SCHEMA: Schema = &[
    ("id",            I),
    ("subclassMapId", I),
    ("flags",         I),
    ("className_enUS", S), ("className_koKR", S), ("className_frFR", S),
    ("className_deDE", S), ("className_zhCN", S), ("className_ruRU", S),
    ("className_esES", S), ("className_ptPT", S), ("classNameMask",  I),
];

pub static ITEM_DISPLAY_INFO_SCHEMA: Schema = &[
    ("id",                   I),
    ("modelName_1",          S), ("modelName_2",          S),
    ("modelTexture_1",       S), ("modelTexture_2",       S),
    ("inventoryIcon",        S),
    ("geosetGroup_1",        I), ("geosetGroup_2",        I), ("geosetGroup_3", I),
    ("flags",                I),
    ("spellVisualId",        I),
    ("groupSoundIndex",      I),
    ("helmetGeosetVisId_1",  I), ("helmetGeosetVisId_2",  I),
    ("texture_1", S), ("texture_2", S), ("texture_3", S), ("texture_4", S),
    ("texture_5", S), ("texture_6", S), ("texture_7", S), ("texture_8", S),
    ("itemVisual",           I),
];

pub static ITEM_RANDOM_PROPERTIES_SCHEMA: Schema = &[
    ("id",              I),
    ("name",            S),
    ("enchantment_1",   I), ("enchantment_2", I), ("enchantment_3", I),
    ("enchantment_4",   I), ("enchantment_5", I),
    ("suffix_enUS", S), ("suffix_koKR", S), ("suffix_frFR", S), ("suffix_deDE", S),
    ("suffix_zhCN", S), ("suffix_ruRU", S), ("suffix_esES", S), ("suffix_ptPT", S),
    ("suffixMask",      I),
];

pub static ITEM_SET_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
    ("itemId_1",  I), ("itemId_2",  I), ("itemId_3",  I), ("itemId_4",  I),
    ("itemId_5",  I), ("itemId_6",  I), ("itemId_7",  I), ("itemId_8",  I),
    ("itemId_9",  I), ("itemId_10", I), ("itemId_11", I), ("itemId_12", I),
    ("itemId_13", I), ("itemId_14", I), ("itemId_15", I), ("itemId_16", I),
    ("itemId_17", I),
    ("setSpellId_1",    I), ("setSpellId_2",    I), ("setSpellId_3",    I),
    ("setSpellId_4",    I), ("setSpellId_5",    I), ("setSpellId_6",    I),
    ("setSpellId_7",    I), ("setSpellId_8",    I),
    ("setThreshold_1",  I), ("setThreshold_2",  I), ("setThreshold_3",  I),
    ("setThreshold_4",  I), ("setThreshold_5",  I), ("setThreshold_6",  I),
    ("setThreshold_7",  I), ("setThreshold_8",  I),
    ("requiredSkillId",   I),
    ("requiredSkillRank", I),
];

// Special case: no unique id. Key = classId * 1000 + subclassId.
// RQ_GetItemSubClass(classId, subclassId) encodes the key before lookup.
pub static ITEM_SUB_CLASS_SCHEMA: Schema = &[
    ("classId",            I),
    ("subclassId",         I),
    ("prereqProficiency",  I),
    ("postreqProficiency", I),
    ("flags",              I),
    ("displayFlags",       I),
    ("weaponParrySeq",     I),
    ("weaponReadySeq",     I),
    ("weaponAttackSeq",    I),
    ("weaponSwingSize",    I),
    ("displayName_enUS", S), ("displayName_koKR", S), ("displayName_frFR", S),
    ("displayName_deDE", S), ("displayName_zhCN", S), ("displayName_ruRU", S),
    ("displayName_esES", S), ("displayName_ptPT", S), ("displayNameMask",  I),
    ("verboseName_enUS", S), ("verboseName_koKR", S), ("verboseName_frFR", S),
    ("verboseName_deDE", S), ("verboseName_zhCN", S), ("verboseName_ruRU", S),
    ("verboseName_esES", S), ("verboseName_ptPT", S), ("verboseNameMask",  I),
];

pub static LFG_DUNGEONS_SCHEMA: Schema = &[
    ("id",           I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",     I),
    ("levelMin",     I),
    ("levelMax",     I),
    ("instanceType", I),
    ("faction",      I),
];

pub static LOCK_SCHEMA: Schema = &[
    ("id",       I),
    ("type_1",   I), ("type_2",   I), ("type_3",   I), ("type_4",   I),
    ("type_5",   I), ("type_6",   I), ("type_7",   I), ("type_8",   I),
    ("index_1",  I), ("index_2",  I), ("index_3",  I), ("index_4",  I),
    ("index_5",  I), ("index_6",  I), ("index_7",  I), ("index_8",  I),
    ("skill_1",  I), ("skill_2",  I), ("skill_3",  I), ("skill_4",  I),
    ("skill_5",  I), ("skill_6",  I), ("skill_7",  I), ("skill_8",  I),
    ("action_1", I), ("action_2", I), ("action_3", I), ("action_4", I),
    ("action_5", I), ("action_6", I), ("action_7", I), ("action_8", I),
];

pub static LOCK_TYPE_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS",     S), ("name_koKR",     S), ("name_frFR",     S),
    ("name_deDE",     S), ("name_zhCN",     S), ("name_ruRU",     S),
    ("name_esES",     S), ("name_ptPT",     S), ("nameMask",      I),
    ("resource_enUS", S), ("resource_koKR", S), ("resource_frFR", S),
    ("resource_deDE", S), ("resource_zhCN", S), ("resource_ruRU", S),
    ("resource_esES", S), ("resource_ptPT", S), ("resourceMask",  I),
    ("verb_enUS",     S), ("verb_koKR",     S), ("verb_frFR",     S),
    ("verb_deDE",     S), ("verb_zhCN",     S), ("verb_ruRU",     S),
    ("verb_esES",     S), ("verb_ptPT",     S), ("verbMask",      I),
    ("cursorName",    S),
];

pub static MAIL_TEMPLATE_SCHEMA: Schema = &[
    ("id",    I),
    ("body_enUS", S), ("body_koKR", S), ("body_frFR", S), ("body_deDE", S),
    ("body_zhCN", S), ("body_ruRU", S), ("body_esES", S), ("body_ptPT", S),
    ("bodyMask", I),
];

pub static MAP_SCHEMA: Schema = &[
    ("id",            I),
    ("directory",     S),
    ("instanceType",  I),
    ("isPvP",         I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",      I),
    ("levelMin",      I),
    ("levelMax",      I),
    ("maxPlayers",    I),
    ("unknown_1",     I), ("unknown_2", I), ("unknown_3", I),
    ("areaTableId",   I),
    ("description_enUS",  S), ("description_koKR",  S), ("description_frFR",  S),
    ("description_deDE",  S), ("description_zhCN",  S), ("description_ruRU",  S),
    ("description_esES",  S), ("description_ptPT",  S), ("descriptionMask",   I),
    ("description2_enUS", S), ("description2_koKR", S), ("description2_frFR", S),
    ("description2_deDE", S), ("description2_zhCN", S), ("description2_ruRU", S),
    ("description2_esES", S), ("description2_ptPT", S), ("description2Mask",  I),
    ("loadingScreenId",   I),
    ("raidOffset",        I),
    ("canGroupQueue",     I),
    ("unknown_4",         I),
];

pub static QUEST_INFO_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
];

pub static QUEST_SORT_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
];

pub static SKILL_LINE_SCHEMA: Schema = &[
    ("id",           I),
    ("categoryId",   I),
    ("skillCostsId", I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",     I),
    ("description_enUS", S), ("description_koKR", S), ("description_frFR", S),
    ("description_deDE", S), ("description_zhCN", S), ("description_ruRU", S),
    ("description_esES", S), ("description_ptPT", S), ("descriptionMask", I),
    ("spellIconId",  I),
];

pub static SKILL_LINE_ABILITY_SCHEMA: Schema = &[
    ("id",                      I),
    ("skillLine",               I),
    ("spell",                   I),
    ("raceMask",                I),
    ("classMask",               I),
    ("excludeRace",             I),
    ("excludeClass",            I),
    ("minSkillLineRank",        I),
    ("supersededBySpellId",     I),
    ("acquireMethod",           I),
    ("trivialSkillLineRankHigh", I),
    ("trivialSkillLineRankLow",  I),
    ("charcterPoints_1",        I),
    ("charcterPoints_2",        I),
    ("numSkillUps",             I),
];

pub static SKILL_LINE_CATEGORY_SCHEMA: Schema = &[
    ("id",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",  I),
    ("sortIndex", I),
];

pub static SPELL_SCHEMA: Schema = &[
    ("id",                           I),
    ("school",                       I),
    ("category",                     I),
    ("castUi",                       I),
    ("dispelType",                   I),
    ("mechanic",                     I),
    ("attribute_1",  I), ("attribute_2",  I), ("attribute_3",  I),
    ("attribute_4",  I), ("attribute_5",  I),
    ("shapeshiftMask",               I),
    ("shapeshiftExclude",            I),
    ("targets",                      I),
    ("targetCreatureType",           I),
    ("requiresSpellFocus",           I),
    ("casterAuraStat",               I),
    ("targetAuraState",              I),
    ("castingTimeIndex",             I),
    ("recoveryTime",                 I),
    ("categoryRecoveryTime",         I),
    ("interruptFlags",               I),
    ("auraInterruptFlags",           I),
    ("channelInterruptFlags",        I),
    ("procTypeMask",                 I),
    ("procChance",                   I),
    ("procCharges",                  I),
    ("maxLevel",                     I),
    ("baseLevel",                    I),
    ("spellLevel",                   I),
    ("durationIndex",                I),
    ("powerType",                    I),
    ("manaCost",                     I),
    ("manaPerLevel",                 I),
    ("manaPerSecond",                I),
    ("manaPerSecondPerLevel",        I),
    ("rangeIndex",                   I),
    ("speed",                        F),
    ("modalNextSpell",               I),
    ("stackAmount",                  I),
    ("totem_1",      I), ("totem_2", I),
    ("reagent_1",    I), ("reagent_2",    I), ("reagent_3",    I), ("reagent_4",    I),
    ("reagent_5",    I), ("reagent_6",    I), ("reagent_7",    I), ("reagent_8",    I),
    ("reagentCount_1", I), ("reagentCount_2", I), ("reagentCount_3", I), ("reagentCount_4", I),
    ("reagentCount_5", I), ("reagentCount_6", I), ("reagentCount_7", I), ("reagentCount_8", I),
    ("equippedItemClass",            I),
    ("equippedItemSubClassMask",     I),
    ("equippedItemInventoryTypeMask", I),
    ("effect_1",     I), ("effect_2",     I), ("effect_3",     I),
    ("effectDieSides_1",         I), ("effectDieSides_2",         I), ("effectDieSides_3",         I),
    ("effectBaseDice_1",         I), ("effectBaseDice_2",         I), ("effectBaseDice_3",         I),
    ("effectDicePerLevel_1",     I), ("effectDicePerLevel_2",     I), ("effectDicePerLevel_3",     I),
    ("effectRealPointsPerLevel_1", F), ("effectRealPointsPerLevel_2", F), ("effectRealPointsPerLevel_3", F),
    ("effectBasePoints_1",       I), ("effectBasePoints_2",       I), ("effectBasePoints_3",       I),
    ("effectMechanic_1",         I), ("effectMechanic_2",         I), ("effectMechanic_3",         I),
    ("effectImplicitTargetA_1",  I), ("effectImplicitTargetA_2",  I), ("effectImplicitTargetA_3",  I),
    ("effectImplicitTargetB_1",  I), ("effectImplicitTargetB_2",  I), ("effectImplicitTargetB_3",  I),
    ("effectRadiusIndex_1",      I), ("effectRadiusIndex_2",      I), ("effectRadiusIndex_3",      I),
    ("effectApplyAura_1",        I), ("effectApplyAura_2",        I), ("effectApplyAura_3",        I),
    ("effectAmplitude_1",        I), ("effectAmplitude_2",        I), ("effectAmplitude_3",        I),
    ("effectMultipleValue_1",    F), ("effectMultipleValue_2",    F), ("effectMultipleValue_3",    F),
    ("effectChainTarget_1",      I), ("effectChainTarget_2",      I), ("effectChainTarget_3",      I),
    ("effectItemType_1",         I), ("effectItemType_2",         I), ("effectItemType_3",         I),
    ("effectMiscValue_1",        I), ("effectMiscValue_2",        I), ("effectMiscValue_3",        I),
    ("effectTriggerSpell_1",     I), ("effectTriggerSpell_2",     I), ("effectTriggerSpell_3",     I),
    ("effectPointsPerCombo_1",   F), ("effectPointsPerCombo_2",   F), ("effectPointsPerCombo_3",   F),
    ("spellVisualId_1",   I), ("spellVisualId_2",   I),
    ("spellIconId",                  I),
    ("activeIconId",                 I),
    ("spellPriority",                I),
    ("name_enUS",        S), ("name_koKR",        S), ("name_frFR",        S),
    ("name_deDE",        S), ("name_zhCN",        S), ("name_ruRU",        S),
    ("name_esES",        S), ("name_ptPT",        S), ("nameMask",         I),
    ("subtext_enUS",     S), ("subtext_koKR",     S), ("subtext_frFR",     S),
    ("subtext_deDE",     S), ("subtext_zhCN",     S), ("subtext_ruRU",     S),
    ("subtext_esES",     S), ("subtext_ptPT",     S), ("subtextMask",      I),
    ("description_enUS", S), ("description_koKR", S), ("description_frFR", S),
    ("description_deDE", S), ("description_zhCN", S), ("description_ruRU", S),
    ("description_esES", S), ("description_ptPT", S), ("descriptionMask",  I),
    ("auraDescription_enUS", S), ("auraDescription_koKR", S), ("auraDescription_frFR", S),
    ("auraDescription_deDE", S), ("auraDescription_zhCN", S), ("auraDescription_ruRU", S),
    ("auraDescription_esES", S), ("auraDescription_ptPT", S), ("auraDescriptionMask",  I),
    ("manaCostPercentage",           I),
    ("startRecoveryCategory",        I),
    ("startRecoveryTime",            I),
    ("maxTargetLevel",               I),
    ("spellClassSet",                I),
    ("spellClassMask_1",  I), ("spellClassMask_2",  I),
    ("maxTargets",                   I),
    ("damageType",                   I),
    ("preventionType",               I),
    ("stanceBarOrder",               I),
    ("damageMultiplier_1", F), ("damageMultiplier_2", F), ("damageMultiplier_3", F),
    ("minFactionId",                 I),
    ("minReputation",                I),
    ("requiredAuraVision",           I),
];

pub static SPELL_CAST_TIMES_SCHEMA: Schema = &[
    ("id",       I),
    ("base",     I),
    ("perLevel", I),
    ("minimum",  I),
];

pub static SPELL_CATEGORY_SCHEMA: Schema = &[
    ("id",    I),
    ("flags", I),
];

pub static SPELL_DISPEL_TYPE_SCHEMA: Schema = &[
    ("id",                 I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",           I),
    ("showOnAuraTooltip",  I),
    ("internalName",       S),
];

pub static SPELL_DURATION_SCHEMA: Schema = &[
    ("id",       I),
    ("base",     I),
    ("perLevel", I),
    ("max",      I),
];

pub static SPELL_ICON_SCHEMA: Schema = &[
    ("id",      I),
    ("texture", S),
];

pub static SPELL_ITEM_ENCHANTMENT_SCHEMA: Schema = &[
    ("id",              I),
    ("effect_1",        I), ("effect_2",     I), ("effect_3",     I),
    ("pointsMin_1",     I), ("pointsMin_2",  I), ("pointsMin_3",  I),
    ("pointsMax_1",     I), ("pointsMax_2",  I), ("pointsMax_3",  I),
    ("effectArg_1",     I), ("effectArg_2",  I), ("effectArg_3",  I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",        I),
    ("itemVisual",      I),
    ("flags",           I),
];

pub static SPELL_MECHANIC_SCHEMA: Schema = &[
    ("id",      I),
    ("unknown", I),
];

pub static SPELL_RADIUS_SCHEMA: Schema = &[
    ("id",               I),
    ("radius",           F),
    ("radiusPerLevel",   I),
    ("radiusMax",        I),
];

pub static SPELL_RANGE_SCHEMA: Schema = &[
    ("id",          I),
    ("rangeMin",    F),
    ("rangeMax",    F),
    ("flags",       I),
    ("name_enUS",      S), ("name_koKR",      S), ("name_frFR",      S),
    ("name_deDE",      S), ("name_zhCN",      S), ("name_ruRU",      S),
    ("name_esES",      S), ("name_ptPT",      S), ("nameMask",       I),
    ("shortName_enUS", S), ("shortName_koKR", S), ("shortName_frFR", S),
    ("shortName_deDE", S), ("shortName_zhCN", S), ("shortName_ruRU", S),
    ("shortName_esES", S), ("shortName_ptPT", S), ("shortNameMask",  I),
];

pub static SPELL_SHAPESHIFT_FORM_SCHEMA: Schema = &[
    ("id",                I),
    ("bonusActionBar",    I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",          I),
    ("flags",             I),
    ("creatureType",      I),
    ("combatRoundTime",   I),
];

pub static TALENT_SCHEMA: Schema = &[
    ("id",                  I),
    ("specId",              I),
    ("row",                 I),
    ("col",                 I),
    ("spellRank_1",  I), ("spellRank_2",  I), ("spellRank_3",  I),
    ("spellRank_4",  I), ("spellRank_5",  I), ("spellRank_6",  I),
    ("spellRank_7",  I), ("spellRank_8",  I), ("spellRank_9",  I),
    ("prerequisiteTalent_1", I), ("prerequisiteTalent_2", I), ("prerequisiteTalent_3", I),
    ("prerequisiteRank_1",   I), ("prerequisiteRank_2",   I), ("prerequisiteRank_3",   I),
    ("flags",               I),
    ("requiredSpellId",     I),
];

pub static TALENT_TAB_SCHEMA: Schema = &[
    ("id",          I),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",    I),
    ("spellIconId", I),
    ("raceMask",    I),
    ("classMask",   I),
    ("orderIndex",  I),
    ("background",  S),
];

pub static TAXI_NODES_SCHEMA: Schema = &[
    ("id",    I),
    ("mapId", I),
    ("x",     F), ("y", F), ("z", F),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask",  I),
    ("mountCreatureId_1", I), ("mountCreatureId_2", I),
];

pub static TAXI_PATH_SCHEMA: Schema = &[
    ("id",       I),
    ("fromNode", I),
    ("toNode",   I),
    ("cost",     I),
];

pub static TAXI_PATH_NODE_SCHEMA: Schema = &[
    ("id",        I),
    ("pathId",    I),
    ("nodeIndex", I),
    ("mapId",     I),
    ("x",         F), ("y", F), ("z", F),
    ("flags",     I),
    ("delay",     I),
];

pub static WORLD_MAP_AREA_SCHEMA: Schema = &[
    ("id",        I),
    ("mapId",     I),
    ("areaId",    I),
    ("name",      S),
    ("locLeft",   F),
    ("locRight",  F),
    ("locTop",    F),
    ("locBottom", F),
];

pub static WORLD_SAFE_LOCS_SCHEMA: Schema = &[
    ("id",    I),
    ("mapId", I),
    ("x",     F), ("y", F), ("z", F),
    ("name_enUS", S), ("name_koKR", S), ("name_frFR", S), ("name_deDE", S),
    ("name_zhCN", S), ("name_ruRU", S), ("name_esES", S), ("name_ptPT", S),
    ("nameMask", I),
];

pub static KNOWN_DBC_NAMES: &[&str] = &[
    "AreaTable", "AreaTrigger", "CharStartOutfit", "ChrClasses", "ChrRaces",
    "CreatureFamily", "CreatureType", "Faction", "FactionTemplate",
    "ItemBagFamily", "ItemClass", "ItemDisplayInfo", "ItemRandomProperties",
    "ItemSet", "ItemSubClass", "LFGDungeons", "Lock", "LockType",
    "MailTemplate", "Map", "QuestInfo", "QuestSort", "SkillLine",
    "SkillLineAbility", "SkillLineCategory", "Spell", "SpellCastTimes",
    "SpellCategory", "SpellDispelType", "SpellDuration", "SpellIcon",
    "SpellItemEnchantment", "SpellMechanic", "SpellRadius", "SpellRange",
    "SpellShapeshiftForm", "Talent", "TalentTab", "TaxiNodes", "TaxiPath",
    "TaxiPathNode", "WorldMapArea", "WorldSafeLocs",
];

#[cfg(not(test))]
use crate::mpq;
#[cfg(not(test))]
use std::sync::Mutex;

#[cfg(not(test))]
static MPQ_LIST: OnceLock<Vec<std::path::PathBuf>> = OnceLock::new();

#[cfg(not(test))]
pub fn init_mpq_list() {
    MPQ_LIST.get_or_init(|| mpq::build_mpq_list(std::path::Path::new("Data")));
}

#[cfg(not(test))]
fn log_error(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true).append(true).open("Logs/Reliquary.log")
    {
        let _ = writeln!(f, "[Reliquary] {}", msg);
    }
}

#[cfg(not(test))]
static DBC_STORE: OnceLock<Mutex<HashMap<&'static str, Option<DbcRows>>>> = OnceLock::new();

#[cfg(not(test))]
fn get_store() -> &'static Mutex<HashMap<&'static str, Option<DbcRows>>> {
    DBC_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Looks up a single record by ID in the named DBC.
/// Returns Ok(Some(fields)) on hit, Ok(None) on miss, Err(msg) on load failure.
#[cfg(not(test))]
pub fn get_record(dbc_name: &'static str, id: u32) -> Result<Option<Vec<FieldValue>>, String> {
    let schema = get_schema(dbc_name)
        .ok_or_else(|| format!("unknown DBC '{}'", dbc_name))?;

    let store = get_store();
    // The lock is held for the full duration of the MPQ read and parse.
    // This is safe because the WoW client is single-threaded and Lua is
    // not re-entrant across DBC loads. Do not call lua_error while this
    // lock is held.
    let mut map = store.lock().unwrap();

    if !map.contains_key(dbc_name) {
        let mpq_list = MPQ_LIST.get()
            .ok_or_else(|| "MPQ list not initialized".to_string())?;

        let internal = format!("DBFilesClient\\{}.dbc", dbc_name);
        match mpq::extract_file(mpq_list, &internal) {
            None => {
                log_error(&format!("'{}' not found in any MPQ", internal));
                map.insert(dbc_name, None);
            }
            Some(data) => {
                match parse_wdbc(&data, schema) {
                    Ok(rows) => {
                        let rows = if dbc_name == "ItemSubClass" {
                            let mut rekeyed = HashMap::with_capacity(rows.len());
                            for (_, fields) in rows {
                                let class_id = match &fields[0] {
                                    FieldValue::Int32(n) if *n >= 0 => *n as u32,
                                    FieldValue::UInt32(n) => *n,
                                    _ => continue,
                                };
                                let sub_id = match &fields[1] {
                                    FieldValue::Int32(n) if *n >= 0 => *n as u32,
                                    FieldValue::UInt32(n) => *n,
                                    _ => continue,
                                };
                                rekeyed.insert(class_id * 1000 + sub_id, fields);
                            }
                            rekeyed
                        } else {
                            rows
                        };
                        map.insert(dbc_name, Some(rows));
                    }
                    Err(e) => {
                        log_error(&format!("failed to parse '{}': {}", internal, e));
                        map.insert(dbc_name, None);
                    }
                }
            }
        }
    }

    match map.get(dbc_name).expect("invariant: key always inserted in block above") {
        None => Err(format!("'DBFilesClient\\{}.dbc' not found in any MPQ", dbc_name)),
        Some(rows) => Ok(rows.get(&id).cloned()),
    }
}

/// Variant for composite-key DBCs like ItemSubClass.
#[cfg(not(test))]
pub fn get_record_composite(dbc_name: &'static str, key: u32) -> Result<Option<Vec<FieldValue>>, String> {
    get_record(dbc_name, key)
}

/// Returns all records from the named DBC as a Vec of field vectors.
#[cfg(not(test))]
pub fn get_all_records(dbc_name: &'static str) -> Result<Vec<Vec<FieldValue>>, String> {
    // Force the DBC to be loaded/cached by looking up a dummy ID.
    let _ = get_record(dbc_name, 0);

    let store = get_store();
    let map = store.lock().unwrap();
    match map.get(dbc_name) {
        Some(Some(rows)) => Ok(rows.values().cloned().collect()),
        Some(None) => Err(format!("'DBFilesClient\\{}.dbc' not found in any MPQ", dbc_name)),
        None => Err(format!("unknown DBC '{}'", dbc_name)),
    }
}

/// Returns the number of records in the named DBC.
#[cfg(not(test))]
pub fn get_record_count(dbc_name: &'static str) -> Result<usize, String> {
    let _ = get_record(dbc_name, 0);

    let store = get_store();
    let map = store.lock().unwrap();
    match map.get(dbc_name) {
        Some(Some(rows)) => Ok(rows.len()),
        Some(None) => Err(format!("'DBFilesClient\\{}.dbc' not found in any MPQ", dbc_name)),
        None => Err(format!("unknown DBC '{}'", dbc_name)),
    }
}

/// Returns the record at 1-based index in the named DBC.
#[cfg(not(test))]
pub fn get_record_by_index(dbc_name: &'static str, index: usize) -> Result<Option<Vec<FieldValue>>, String> {
    let _ = get_record(dbc_name, 0);

    let store = get_store();
    let map = store.lock().unwrap();
    match map.get(dbc_name) {
        Some(Some(rows)) => {
            if index == 0 || index > rows.len() {
                Ok(None)
            } else {
                Ok(rows.values().nth(index - 1).cloned())
            }
        }
        Some(None) => Err(format!("'DBFilesClient\\{}.dbc' not found in any MPQ", dbc_name)),
        None => Err(format!("unknown DBC '{}'", dbc_name)),
    }
}
