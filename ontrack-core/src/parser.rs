//! parser.rs — Read addresses from CSV / TSV / plain-text files.
//!
//! Supported formats:
//!   • CSV with an "address" column header (matches Python ontrack)
//!   • Plain text — one address per line (no header required)
//!   • TSV with an "address" column

use anyhow::{anyhow, Context, Result};
use std::path::Path;

/// Parse a file and return a list of address strings.
///
/// Auto-detects format:
/// - If the file has an "address" column → use it
/// - Otherwise treat every non-empty, non-comment line as an address
pub fn parse_addresses<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" => parse_csv(path, b','),
        "tsv" | "tab" => parse_csv(path, b'\t'),
        "txt" | "" => parse_plain_text(path),
        other => Err(anyhow!("Unsupported file extension: .{other}")),
    }
}

fn parse_csv(path: &Path, delimiter: u8) -> Result<Vec<String>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("Cannot open file: {}", path.display()))?;

    let headers = reader.headers()?.clone();

    // Find the "address" column (case-insensitive)
    let addr_col = headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case("address"))
        .ok_or_else(|| {
            anyhow!(
                "No 'address' column found in {}. \
                 Headers found: {:?}",
                path.display(),
                headers.iter().collect::<Vec<_>>()
            )
        })?;

    let mut addresses = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("Parse error on row {}", i + 2))?;
        if let Some(addr) = record.get(addr_col) {
            let addr = addr.trim().to_string();
            if !addr.is_empty() {
                addresses.push(addr);
            }
        }
    }

    if addresses.is_empty() {
        return Err(anyhow!("File {} contains no addresses", path.display()));
    }

    Ok(addresses)
}

fn parse_plain_text(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read file: {}", path.display()))?;

    let addresses: Vec<String> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();

    if addresses.is_empty() {
        return Err(anyhow!("File {} contains no addresses", path.display()));
    }

    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str, ext: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_csv_with_header() {
        let f = write_temp("address\n123 Main St\n456 Elm Ave\n", "csv");
        let addrs = parse_addresses(f.path()).unwrap();
        assert_eq!(addrs, vec!["123 Main St", "456 Elm Ave"]);
    }

    #[test]
    fn parses_plain_text() {
        let f = write_temp("# comment\n123 Main St\n\n456 Elm Ave\n", "txt");
        let addrs = parse_addresses(f.path()).unwrap();
        assert_eq!(addrs, vec!["123 Main St", "456 Elm Ave"]);
    }

    #[test]
    fn drops_empty_rows() {
        let f = write_temp("address\n123 Main St\n\n456 Elm Ave\n", "csv");
        let addrs = parse_addresses(f.path()).unwrap();
        assert_eq!(addrs.len(), 2);
    }

    #[test]
    fn missing_column_errors() {
        let f = write_temp("street\n123 Main St\n", "csv");
        assert!(parse_addresses(f.path()).is_err());
    }
}
