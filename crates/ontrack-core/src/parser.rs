// /qompassai/ontrack-rs/crates/ontrack-core/src/parser.rs
// Qompass AI — OnTrack core: address file parser
// Copyright (C) 2026 Qompass AI, All rights reserved.
// -----------------------------------------------------
//! CSV / Excel parser.
//!
//! Files must contain a column named `address` (case-insensitive). All other
//! columns are ignored. Empty cells are skipped.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};

/// Parse a CSV or Excel file and return a list of address strings.
///
/// Supported extensions: `.csv`, `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`.
pub fn parse_addresses<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "csv" => parse_csv(path),
        "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => parse_excel(path),
        other => Err(anyhow!("unsupported file extension: .{other}")),
    }
}

fn parse_csv(path: &Path) -> Result<Vec<String>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("opening CSV {}", path.display()))?;

    let headers = rdr.headers()?.clone();
    let idx = headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case("address"))
        .ok_or_else(|| anyhow!("CSV is missing an 'address' column"))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        if let Some(v) = rec.get(idx) {
            let v = v.trim();
            if !v.is_empty() {
                out.push(v.to_string());
            }
        }
    }
    Ok(out)
}

fn parse_excel(path: &Path) -> Result<Vec<String>> {
    let mut wb = open_workbook_auto(path)
        .with_context(|| format!("opening workbook {}", path.display()))?;
    let sheet_name = wb
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("workbook has no sheets"))?;
    let range = wb
        .worksheet_range(&sheet_name)
        .with_context(|| format!("reading sheet {sheet_name}"))?;

    let mut rows = range.rows();
    let header = rows.next().ok_or_else(|| anyhow!("workbook is empty"))?;
    let idx = header
        .iter()
        .position(|c| matches!(c, Data::String(s) if s.trim().eq_ignore_ascii_case("address")))
        .ok_or_else(|| anyhow!("workbook is missing an 'address' column"))?;

    let mut out = Vec::new();
    for row in rows {
        if let Some(cell) = row.get(idx) {
            let s = match cell {
                Data::String(s) => s.trim().to_string(),
                Data::Float(f) => f.to_string(),
                Data::Int(i) => i.to_string(),
                Data::DateTime(dt) => dt.to_string(),
                Data::Bool(b) => b.to_string(),
                _ => String::new(),
            };
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    Ok(out)
}
