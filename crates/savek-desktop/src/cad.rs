//! Bounded, read-only STEP Part 21 header inspection.
//!
//! This is NOT a renderer or geometry parser. 3D visualization requires an
//! additional CAD kernel such as Open CASCADE.
use std::fs::File;
use std::io::{Read, Result};
use std::path::Path;

const HEADER_BYTES: u64 = 32 * 1024;
const MAX_FIELD_CHARS: usize = 220;

pub fn is_step(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("step" | "stp")
    )
}

/// Read only a small prefix of the file, without modifying it.
/// The content may be incomplete or non-UTF8, so fields are best effort.
pub fn inspect_header(path: &Path) -> Result<String> {
    let mut buf = Vec::new();
    File::open(path)?.take(HEADER_BYTES).read_to_end(&mut buf)?;
    Ok(summarize_step_header(&buf))
}

fn summarize_step_header(bytes: &[u8]) -> String {
    let prefix = String::from_utf8_lossy(bytes);
    let text = prefix.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with("ISO-10303-21;") {
        return "Not a recognized text STEP Part 21 header; file may be invalid or a different CAD format.".into();
    }
    let mut fields = vec!["STEP Part 21 file detected".to_owned()];
    for (key, display) in [("FILE_NAME(", "File name"), ("FILE_SCHEMA(", "Schema"), ("FILE_DESCRIPTION(", "Description")] {
        if let Some(value) = extract_statement(text, key) {
            fields.push(format!("{display}: {value}"));
        }
    }
    if fields.len() == 1 {
        fields.push("No readable header properties in first 32 KB".to_owned());
    }
    fields.join("\n")
}

/// Extract and sanitize one semicolon-terminated STEP header statement.
/// No shell commands or scripts are evaluated.
fn extract_statement(text: &str, keyword: &str) -> Option<String> {
    let upper = text.to_ascii_uppercase();
    let start = upper.find(keyword)?;
    let statement = text.get(start + keyword.len()..)?.split(';').next()?;
    let normalized: String = statement
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD_CHARS)
        .collect();
    Some(normalized.trim().trim_end_matches(')').trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_common_step_metadata() {
        let header = b"ISO-10303-21;\nHEADER;\nFILE_NAME('part.step','2026',('author'),(),'', '', '');\nFILE_SCHEMA(('AP214'));\nENDSEC;\nDATA;";
        let info = summarize_step_header(header);
        assert!(info.contains("STEP Part 21"));
        assert!(info.contains("part.step"));
        assert!(info.contains("AP214"));
    }
    #[test]
    fn rejects_junk_prefix() {
        assert!(summarize_step_header(b"nothing to see")
            .contains("Not a recognized"));
    }
    #[test]
    fn recognizes_case_insensitive_step_suffixes() {
        assert!(is_step(Path::new("part.STP")));
        assert!(is_step(Path::new("assembly.step")));
        assert!(!is_step(Path::new("image.png")));
    }
}