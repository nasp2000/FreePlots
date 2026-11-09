// SPDX-License-Identifier: MIT
//! Robustness of the readers that consume files from outside the application.
//!
//! A CSV, a project file and a workbook come from a user, a script or another
//! program. Every one of them can be truncated, corrupted or deliberately
//! hostile. The contract is narrow and absolute: the reader either returns a
//! frame or an error, and never panics, loops or allocates without bound.
//!
//! The corpus is generated here, from text the test owns, so the check is
//! deterministic and needs no external file or program. The workbook fixture is
//! the small synthetic workbook committed for the CLI tests.

use std::path::Path;

use fp_core::data::{Column, DataFrame};
use fp_core::io;
use fp_core::xlsx_read;

/// Parses one CSV byte string; an error is fine, a panic is not.
fn csv_round_trip(bytes: &[u8]) {
    let (text, _encoding) = io::decode_bytes(bytes);
    let options = io::detect_options(&text);
    let _ = io::read_csv_str(&text, options);
}

/// Evaluates one formula; an error is fine, a panic is not.
fn formula_round_trip(text: &str) {
    let _ = fp_core::formula::evaluate(&frame(), text);
}

/// Parses one project byte string; an error is fine, a panic is not.
fn project_round_trip(bytes: &[u8]) {
    let (text, _encoding) = io::decode_bytes(bytes);
    let _ = io::from_project_json_sheets(&text);
}

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
        Column::new_text(
            "b",
            vec![
                "x".to_owned(),
                "y".to_owned(),
                "z".to_owned(),
                "w".to_owned(),
            ],
        ),
    ])
    .expect("frame")
}

#[test]
fn every_truncation_of_a_csv_is_read_or_refused() {
    let source = b"a,b,c\r\n1,2,3\r\n4.5,-6,7e3\r\n\"x,y\",8,\r\n9,,11\r\n";
    for length in 0..=source.len() {
        csv_round_trip(&source[..length]);
    }
    let (text, encoding) = io::decode_bytes(source);
    let parsed = io::read_csv_str(&text, io::detect_options(&text)).expect("valid csv");
    assert_eq!(encoding, "UTF-8");
    assert_eq!(parsed.n_rows(), 4);
}

#[test]
fn every_single_byte_corruption_of_a_csv_is_read_or_refused() {
    let source = b"name,value,group\nalpha,1.5,A\nbeta,2.5,B\ngamma,3.5,A\n";
    for index in 0..source.len() {
        let mut bytes = source.to_vec();
        bytes[index] ^= 0xFF;
        csv_round_trip(&bytes);
    }
}

#[test]
fn hostile_csv_text_is_read_or_refused() {
    let cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        vec![0],
        vec![0xFF, 0xFE, 0x00, 0x01, 0x02],
        b"\xEF\xBB\xBFa,b\n1,2\n".to_vec(),
        b"a\n\"unterminated\n".to_vec(),
        b"a,b\n1,2,3,4,5\n".to_vec(),
        b",,\n,,\n".to_vec(),
        b"a,a,a\n1,2,3\n".to_vec(),
        b"value\nNaN\ninf\n-inf\n".to_vec(),
        b"value\n1e999999\n".to_vec(),
        b"a;b\n1,5;2,5\n".to_vec(),
        b"a\tb\n1\t2\n".to_vec(),
        format!("{}\n{}", "c,".repeat(2000), "1,".repeat(2000)).into_bytes(),
        format!("v\n{}\n", "9".repeat(64 * 1024)).into_bytes(),
    ];
    for bytes in cases {
        csv_round_trip(&bytes);
    }
}

#[test]
fn every_truncation_of_a_project_is_read_or_refused() {
    let text = io::to_project_json_sheets(&[("Sheet1".to_owned(), frame())], "probe")
        .expect("project json");
    let bytes = text.as_bytes();
    for length in 0..=bytes.len() {
        project_round_trip(&bytes[..length]);
    }
    let (text, _) = io::decode_bytes(bytes);
    let sheets = io::from_project_json_sheets(&text).expect("valid project");
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].1.n_rows(), 4);
}

#[test]
fn every_single_byte_corruption_of_a_project_is_read_or_refused() {
    let text = io::to_project_json_sheets(&[("Sheet1".to_owned(), frame())], "probe")
        .expect("project json");
    let bytes = text.as_bytes();
    for index in 0..bytes.len() {
        let mut copy = bytes.to_vec();
        copy[index] ^= 0xFF;
        project_round_trip(&copy);
    }
}

#[test]
fn hostile_formulas_are_evaluated_or_refused() {
    let cases: Vec<String> = vec![
        "=".to_owned(),
        "=\u{0}\u{1}\u{202e}".to_owned(),
        "=1e999999".to_owned(),
        format!("={}", "(".repeat(10_000)),
        format!("={}1{}", "(".repeat(10_000), ")".repeat(10_000)),
        format!("={}", "-".repeat(10_000)),
        format!("=1{}", "^1".repeat(10_000)),
        format!("={}1{}", "SUM(".repeat(10_000), ")".repeat(10_000)),
        "=SUM(C1R1:C1R1000000000)".to_owned(),
        "=SUM(C1R1:C999999999R1)".to_owned(),
        "=SUM(C1R1:C1R2".to_owned(),
        "=\"unterminated".to_owned(),
    ];
    for formula in cases {
        formula_round_trip(&formula);
    }
}

#[test]
fn hostile_project_json_is_read_or_refused() {
    let cases: Vec<String> = vec![
        "{}".to_owned(),
        "null".to_owned(),
        "[]".to_owned(),
        r#"{"schema_version":4294967295,"name":"x"}"#.to_owned(),
        r#"{"schema_version":1,"name":"x","data":{}}"#.to_owned(),
        format!("{}{}", "[".repeat(100_000), "]".repeat(100_000)),
        format!(
            "{{\"schema_version\":1,\"name\":\"{0}\",\"data\":null}}",
            "x".repeat(1_000_000)
        ),
    ];
    for text in cases {
        project_round_trip(text.as_bytes());
    }
}

#[test]
fn every_truncation_of_a_workbook_is_read_or_refused() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fp-app/tests/data/cli/normal.xlsx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        xlsx_read::read_workbook(&bytes).is_ok(),
        "the fixture must read"
    );
    // A zip reader rejects a short prefix at once; a stride keeps the test
    // quick while still covering every part of the archive.
    for length in (0..bytes.len()).step_by(7) {
        let _ = xlsx_read::read_workbook(&bytes[..length]);
        let _ = xlsx_read::sheet_names(&bytes[..length]);
        let _ = xlsx_read::sheet_dimensions(&bytes[..length]);
    }
    for length in 0..bytes.len().min(64) {
        let _ = xlsx_read::read_workbook(&bytes[..length]);
    }
}

#[test]
fn every_single_byte_corruption_of_a_workbook_is_read_or_refused() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fp-app/tests/data/cli/normal.xlsx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    for index in 0..bytes.len() {
        let mut copy = bytes.clone();
        copy[index] ^= 0xFF;
        let _ = xlsx_read::read_workbook(&copy);
    }
    // The name and dimension readers decompress and parse the same bytes; a
    // stride keeps the extra coverage cheap.
    for index in (0..bytes.len()).step_by(17) {
        let mut copy = bytes.clone();
        copy[index] ^= 0xFF;
        let _ = xlsx_read::sheet_names(&copy);
        let _ = xlsx_read::sheet_dimensions(&copy);
    }
}

#[test]
fn an_archive_that_claims_thousands_of_entries_is_refused() {
    // End of central directory: signature, empty disk fields, 65535 entries,
    // then a central-directory offset inside the record itself.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    assert!(xlsx_read::read_workbook(&bytes).is_err());
}
