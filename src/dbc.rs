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
        rows.insert(id, fields);
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
