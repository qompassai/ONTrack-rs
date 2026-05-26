// /qompassai/ontrack-rs/crates/ontrack-core/tests/parser.rs
// Qompass AI — parser integration tests
// Copyright (C) 2026 Qompass AI, All rights reserved.
use ontrack_core::parse_addresses;
use std::io::Write;

#[test]
fn parses_csv_with_header() {
    let tmp = std::env::temp_dir().join("ontrack-rs-parser.csv");
    let mut f = std::fs::File::create(&tmp).unwrap();
    writeln!(f, "address,note").unwrap();
    writeln!(f, "123 Main St Spokane WA,foo").unwrap();
    writeln!(f, "456 Elm St,").unwrap();
    writeln!(f, ",empty").unwrap();
    drop(f);

    let addrs = parse_addresses(&tmp).unwrap();
    assert_eq!(addrs.len(), 2);
    assert_eq!(addrs[0], "123 Main St Spokane WA");
    assert_eq!(addrs[1], "456 Elm St");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn missing_address_column_is_error() {
    let tmp = std::env::temp_dir().join("ontrack-rs-parser-bad.csv");
    let mut f = std::fs::File::create(&tmp).unwrap();
    writeln!(f, "street,city").unwrap();
    writeln!(f, "123 Main,Spokane").unwrap();
    drop(f);
    assert!(parse_addresses(&tmp).is_err());
    std::fs::remove_file(&tmp).ok();
}
