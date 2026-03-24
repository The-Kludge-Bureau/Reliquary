use std::path::{Path, PathBuf};

/// Returns a sort key for patch MPQ filenames (case-insensitive extension).
///
/// Priority order (lowest to highest):
///   patch.mpq      -> 0
///   patch-0.mpq    -> 1
///   ...
///   patch-9.mpq    -> 10
///   patch-A.mpq    -> 11
///   ...
///   patch-Z.mpq    -> 36
///   anything else  -> u32::MAX
pub fn patch_sort_key(name: &str) -> u32 {
    // Strip the extension case-insensitively.
    let stem = if name.to_ascii_uppercase().ends_with(".MPQ") {
        &name[..name.len() - 4]
    } else {
        return u32::MAX;
    };

    let upper = stem.to_ascii_uppercase();

    if upper == "PATCH" {
        return 0;
    }

    // Must match "PATCH-X" where X is a single digit 0-9 or letter A-Z.
    if upper.starts_with("PATCH-") && upper.len() == 7 {
        let ch = upper.as_bytes()[6] as char;
        if ch >= '0' && ch <= '9' {
            return (ch as u32) - ('0' as u32) + 1; // '0'->1 .. '9'->10
        }
        if ch >= 'A' && ch <= 'Z' {
            return (ch as u32) - ('A' as u32) + 11; // 'A'->11 .. 'Z'->36
        }
    }

    u32::MAX
}

/// Returns MPQ paths ordered from lowest to highest priority.
///
/// Fixed base archives come first (in the WoW client load order), followed by
/// patch archives discovered by scanning `wow_dir`, sorted by `patch_sort_key`.
///
/// Each base archive is resolved by trying `NAME.MPQ` then `NAME.mpq`; it is
/// skipped if neither exists.
#[cfg(not(test))]
pub fn build_mpq_list(wow_dir: &Path) -> Vec<PathBuf> {
    const BASE_NAMES: &[&str] = &[
        "model", "texture", "terrain", "wmo", "sound", "misc", "interface", "fonts", "speech",
        "dbc", "speech2",
    ];

    let mut list: Vec<PathBuf> = Vec::new();

    // Add base archives in fixed order.
    for &name in BASE_NAMES {
        let upper = wow_dir.join(format!("{}.MPQ", name));
        let lower = wow_dir.join(format!("{}.mpq", name));
        if upper.exists() {
            list.push(upper);
        } else if lower.exists() {
            list.push(lower);
        }
    }

    // Discover patch archives.
    let mut patches: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(wow_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext_upper = ext.to_ascii_uppercase();
                if ext_upper == "MPQ" {
                    if let Some(file_name) = path.file_name() {
                        let file_name_str = file_name.to_string_lossy();
                        if patch_sort_key(&file_name_str) != u32::MAX {
                            patches.push(path);
                        }
                    }
                }
            }
        }
    }

    patches.sort_by_key(|p| {
        p.file_name()
            .map(|n| patch_sort_key(&n.to_string_lossy()))
            .unwrap_or(u32::MAX)
    });

    list.extend(patches);
    list
}

/// Searches `mpq_list` from highest to lowest priority, returning the first
/// match for `internal_path`.
#[cfg(not(test))]
pub fn extract_file(mpq_list: &[PathBuf], internal_path: &str) -> Option<Vec<u8>> {
    use stormlib_rs::MpqArchive;

    for mpq_path in mpq_list.iter().rev() {
        let mut archive = match MpqArchive::open(mpq_path) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if let Ok(data) = archive.read_file(internal_path) {
            return Some(data);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::patch_sort_key;

    #[test]
    fn test_patch_sort_key_base() {
        assert_eq!(patch_sort_key("patch.mpq"), 0);
        assert_eq!(patch_sort_key("patch.MPQ"), 0);
    }

    #[test]
    fn test_patch_sort_key_numbers() {
        assert_eq!(patch_sort_key("patch-0.mpq"), 1);
        assert_eq!(patch_sort_key("patch-1.mpq"), 2);
        assert_eq!(patch_sort_key("patch-2.mpq"), 3);
        assert_eq!(patch_sort_key("patch-9.mpq"), 10);
    }

    #[test]
    fn test_patch_sort_key_letters() {
        assert_eq!(patch_sort_key("patch-A.mpq"), 11);
        assert_eq!(patch_sort_key("patch-A.MPQ"), 11);
        assert_eq!(patch_sort_key("patch-Z.mpq"), 36);
        assert_eq!(patch_sort_key("patch-a.mpq"), 11);
        assert_eq!(patch_sort_key("patch-z.mpq"), 36);
    }

    #[test]
    fn test_patch_sort_key_unknown() {
        assert_eq!(patch_sort_key("base.mpq"), u32::MAX);
        assert_eq!(patch_sort_key("patch-AB.mpq"), u32::MAX);
    }

    #[test]
    fn test_patch_sort_order() {
        let mut names = vec![
            "patch-A.mpq",
            "patch-3.mpq",
            "patch.mpq",
            "patch-0.mpq",
            "patch-1.mpq",
            "patch-9.mpq",
            "patch-B.mpq",
        ];
        names.sort_by_key(|n| patch_sort_key(n));
        assert_eq!(
            names,
            vec![
                "patch.mpq",
                "patch-0.mpq",
                "patch-1.mpq",
                "patch-3.mpq",
                "patch-9.mpq",
                "patch-A.mpq",
                "patch-B.mpq",
            ]
        );
    }
}
