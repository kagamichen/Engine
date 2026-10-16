use std::collections::HashSet;
use std::path::Path;

use unicode_normalization::UnicodeNormalization;

pub(super) fn archive_name(original: &str, used: &mut HashSet<String>) -> String {
    let safe: String = original
        .nfc()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut safe = safe
        .trim_start_matches(' ')
        .trim_end_matches([' ', '.'])
        .to_owned();
    if safe.is_empty() {
        safe = "file".to_owned();
    }
    let base = safe
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            base.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        })
    {
        safe.insert(0, '_');
    }
    while safe.len() > 220 {
        safe.pop();
    }
    let path = Path::new(&safe);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let extension = path.extension().and_then(|s| s.to_str());
    let mut candidate = safe.clone();
    let mut number = 2_u64;
    while !used.insert(candidate.to_lowercase().nfc().collect()) {
        candidate = match extension {
            Some(extension) => format!("{stem} ({number}).{extension}"),
            None => format!("{stem} ({number})"),
        };
        number += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_names_and_disambiguates_equivalent_names() {
        let mut used = HashSet::new();
        assert_eq!(archive_name("报告 空格.txt", &mut used), "报告 空格.txt");
        assert_eq!(
            archive_name("报告 空格.txt", &mut used),
            "报告 空格 (2).txt"
        );
        assert_eq!(archive_name(".hidden", &mut used), ".hidden");
        assert_eq!(archive_name("ABC.txt", &mut used), "ABC.txt");
        assert_eq!(archive_name("abc.TXT", &mut used), "abc (2).TXT");
        assert_eq!(archive_name("é.txt", &mut used), "é.txt");
        assert_eq!(archive_name("e\u{301}.txt", &mut used), "é (2).txt");
    }

    #[test]
    fn prevents_paths_reserved_names_and_excessive_lengths() {
        let mut used = HashSet::new();
        for original in [
            "../../file",
            "C:\\path\\file",
            "NUL.txt",
            "CON",
            "LPT1",
            "..",
            "",
            "a\0b",
        ] {
            let name = archive_name(original, &mut used);
            assert!(!name.is_empty());
            assert!(!name.contains(['/', '\\', ':', '\0']));
            assert_ne!(name, "..");
            assert!(!["NUL.txt", "CON", "LPT1"].contains(&name.as_str()));
        }
        let first = archive_name(&"中".repeat(300), &mut used);
        let second = archive_name(&"中".repeat(300), &mut used);
        assert!(first.len() <= 220);
        assert!(second.len() < 255);
        assert_ne!(first, second);
    }
}
