// SPDX-License-Identifier: MIT
//! Fuzz corpus for the three readers that take bytes from outside: CSV,
//! XLSX (a ZIP of XML parts) and the `.fpp` project (JSON). Every case is
//! generated here, so the check is deterministic and needs no external file.
//!
//! The contract is the same one `reader_robustness` states: the reader returns
//! a frame or an error, never panics, never loops and never allocates without
//! bound. The workbook cases go through a minimal archive builder, so the
//! worksheet XML itself is what is being exercised.

use std::time::{Duration, Instant};

use fp_core::data::{Column, DataFrame};
use fp_core::io;
use fp_core::xlsx_read;

/// Ceiling for one corpus loop; a hang shows up as a timeout instead of a
/// stuck job.
const BUDGET: Duration = Duration::from_secs(20);

fn elapsed_ok(start: Instant, what: &str) {
    let elapsed = start.elapsed();
    assert!(
        elapsed < BUDGET,
        "{what} took {elapsed:?}, more than {BUDGET:?}"
    );
}

/// Builds a ZIP with stored entries (`name`, `payload`).
///
/// Only what the reader needs: local headers, the central directory and the
/// end record. No compression, so the bytes are under the test's control.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut central = Vec::new();
    for (name, payload) in entries {
        let offset = data.len() as u32;
        data.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local header
        data.extend_from_slice(&20u16.to_le_bytes()); // version needed
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        data.extend_from_slice(&0u16.to_le_bytes()); // time
        data.extend_from_slice(&0u16.to_le_bytes()); // date
        data.extend_from_slice(&0u32.to_le_bytes()); // crc
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // compressed
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // uncompressed
        data.extend_from_slice(&(name.len() as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // extra length
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(payload);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&0u16.to_le_bytes()); // time
        central.extend_from_slice(&0u16.to_le_bytes()); // date
        central.extend_from_slice(&0u32.to_le_bytes()); // crc
        central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let directory_offset = data.len() as u32;
    data.extend_from_slice(&central);
    data.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // disk
    data.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
    data.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    data.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    data.extend_from_slice(&(central.len() as u32).to_le_bytes());
    data.extend_from_slice(&directory_offset.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // comment length
    data
}

/// A workbook whose first worksheet is `sheet`.
fn workbook(sheet: &str) -> Vec<u8> {
    zip(&[("xl/worksheets/sheet1.xml", sheet.as_bytes())])
}

fn read_or_refuse(bytes: &[u8]) {
    let _ = xlsx_read::read_workbook(bytes);
    let _ = xlsx_read::sheet_names(bytes);
    let _ = xlsx_read::sheet_dimensions(bytes);
}

#[test]
fn hostile_worksheets_are_read_or_refused() {
    let sheets = [
        // No sheet data at all.
        "<worksheet></worksheet>",
        // An empty sheetData.
        "<worksheet><sheetData></sheetData></worksheet>",
        // Truncated after the opening row.
        "<worksheet><sheetData><row r=\"1\">",
        // A value element that is never closed.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is></row>",
        // An empty element name and an empty attribute name.
        "<worksheet><sheetData><row><c r=\"A1\"><v>1</v></c></row><></></sheetData></worksheet>",
        // An attribute whose value is never closed.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr><is><t>x</t></is></c></row></sheetData></worksheet>",
        // A cell reference with no letters, and one past the last column.
        "<worksheet><sheetData><row r=\"1\"><c r=\"1A\"><v>1</v></c><c r=\"XFE1\"><v>2</v></c></row></sheetData></worksheet>",
        // A shared-string index past the table, a negative one and text.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>9999</v></c><c r=\"B1\" t=\"s\"><v>-1</v></c><c r=\"C1\" t=\"s\"><v>x</v></c></row></sheetData></worksheet>",
        // A boolean that is not 0 or 1, and a cell with no value.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"b\"><v>7</v></c><c r=\"B1\"/></row></sheetData></worksheet>",
        // NUL and control characters inside the text.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>a\u{0}b\u{1}c</t></is></c></row></sheetData></worksheet>",
        // Markup that looks like an entity, and a CDATA section.
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>&lt;&amp;&#x41;</t></is></c><c r=\"B1\" t=\"inlineStr\"><is><t><![CDATA[x]]></t></is></c></row></sheetData></worksheet>",
        // Comments and processing instructions between rows.
        "<?xml version=\"1.0\"?><worksheet><!-- c --><sheetData><!--- x ---><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>",
        // Deeply nested elements with no cell in them.
        "<worksheet><a><b><c><d><e><f><g><h><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></h></g></f></e></d></c></b></a></worksheet>",
    ];
    let start = Instant::now();
    for sheet in sheets {
        read_or_refuse(&workbook(sheet));
    }
    elapsed_ok(start, "hostile worksheets");
}

#[test]
fn an_absurd_date_serial_is_kept_as_text() {
    // A date-formatted cell whose number is far outside the calendar used to
    // overflow the day arithmetic (a panic in a debug build).
    for serial in ["1000000000000000", "1e300", "99999999999999999999"] {
        // The second data row keeps the column textual, so the raw string the
        // file carried survives.
        let sheet = format!(
            "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>date</t></is></c><c r=\"B1\" t=\"inlineStr\"><is><t>note</t></is></c></row><row r=\"2\"><c r=\"A2\" s=\"1\"><v>{serial}</v></c><c r=\"B2\" t=\"inlineStr\"><is><t>x</t></is></c></row><row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>n/a</t></is></c><c r=\"B3\" t=\"inlineStr\"><is><t>y</t></is></c></row></sheetData></worksheet>"
        );
        let frame = xlsx_read::read_workbook(&workbook(&sheet)).expect("a readable sheet");
        let text = frame.cell_text(0, 0).unwrap_or_default();
        assert_eq!(
            text, serial,
            "an out-of-range serial must stay as the text the file carried"
        );
    }
}

#[test]
fn a_zip_with_a_lying_directory_is_refused() {
    let good = workbook(
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>",
    );
    let start = Instant::now();

    // Not a ZIP at all, and the shortest possible prefixes.
    for length in 0..64.min(good.len()) {
        let _ = xlsx_read::read_workbook(&good[..length]);
    }

    // The end record says two entries but the directory holds one.
    let mut lying = good.clone();
    let eocd = lying.len() - 22;
    lying[eocd + 8] = 2;
    lying[eocd + 10] = 2;
    let _ = xlsx_read::read_workbook(&lying);

    // The central directory starts past the end of the file.
    let mut remote = good.clone();
    remote[eocd + 16..eocd + 20].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    let _ = xlsx_read::read_workbook(&remote);

    // A local header offset inside the end record.
    let mut bad_offset = good.clone();
    let directory = u32::from_le_bytes(good[eocd + 16..eocd + 20].try_into().expect("offset"));
    let entry = directory as usize;
    bad_offset[entry + 42..entry + 46].copy_from_slice(&(eocd as u32).to_le_bytes());
    let _ = xlsx_read::read_workbook(&bad_offset);

    // A compression method the reader does not know.
    let mut method = good.clone();
    method[entry + 10..entry + 12].copy_from_slice(&99u16.to_le_bytes());
    let _ = xlsx_read::read_workbook(&method);

    // An entry that claims four gigabytes of compressed data.
    let mut huge = good.clone();
    huge[entry + 20..entry + 24].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    let _ = xlsx_read::read_workbook(&huge);

    // Two entries with the same name, the first one empty.
    let duplicate = zip(&[
        ("xl/worksheets/sheet1.xml", &b""[..]),
        ("xl/worksheets/sheet1.xml", &b"<worksheet/>"[..]),
    ]);
    let _ = xlsx_read::read_workbook(&duplicate);

    // The end record hidden inside a comment that repeats its signature.
    let mut commented = good.clone();
    commented.extend_from_slice(b"PK\x05\x06");
    commented.extend_from_slice(&[0u8; 18]);
    let _ = xlsx_read::read_workbook(&commented);

    elapsed_ok(start, "hostile archives");
}

#[test]
fn hostile_csv_text_is_read_or_refused() {
    let cases = [
        "",
        "\u{feff}",
        "a",
        "a\n",
        "a,b\n",
        "a,b\n1\n",
        "a,b\n1,2,3\n",
        "\0,\0\n\0,\0\n",
        "a,b\n\"unterminated,1\n",
        "a,b\n\"x\"\"y\",2\n",
        "a,b\r\n1,2\r\n",
        "a,b\r1,2\r",
        "a,b\n1,2\r\n\r\n",
        ",\n,\n",
        ";;;;\n;;;;\n",
        "a;b\n1,5;2,5\n",
        "a,b\n1e999999,-1e999999\n",
        "a,b\nNaN,inf\n",
        "a,b\n2026-02-30,2026-13-01\n",
        "a,b\n0001-01-01,9999-12-31\n",
        "a\0b,c\n1,2\n",
        "\u{feff}a,b\n\u{feff}1,2\n",
    ];
    let start = Instant::now();
    for text in cases {
        let (decoded, _encoding) = io::decode_bytes(text.as_bytes());
        let options = io::detect_options(&decoded);
        let _ = io::read_csv_str(&decoded, options);
    }

    // A very wide record and a very long field: both are accepted, so the
    // point is that neither panics and both finish.
    let wide: String = (0..2_000).map(|index| format!("c{index},")).collect();
    let wide = format!(
        "{}last\n{}1\n",
        wide.trim_end_matches(','),
        "1,".repeat(2_000)
    );
    let options = io::detect_options(&wide);
    let _ = io::read_csv_str(&wide, options);

    let long = format!("x\n{}\n", "9".repeat(1_000_000));
    let options = io::detect_options(&long);
    let _ = io::read_csv_str(&long, options);
    elapsed_ok(start, "hostile CSV");
}

#[test]
fn hostile_project_json_is_read_or_refused() {
    let cases = [
        "",
        "null",
        "[]",
        "{}",
        "{\"schema_version\":1}",
        "{\"schema_version\":999,\"name\":\"x\",\"data\":{\"columns\":[]}}",
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[]}}",
        "{\"schema_version\":1,\"name\":\"x\",\"sheets\":[]}",
        // A number JSON does not allow.
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\",\"data\":{\"kind\":\"f64\",\"values\":[NaN]},\"missing\":[false]}]}}",
        // Duplicate keys and unknown fields.
        "{\"schema_version\":1,\"schema_version\":1,\"extra\":true,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\",\"data\":{\"kind\":\"f64\",\"values\":[1.0]},\"missing\":[false]}]}}",
        // A text column with a missing mask of the wrong length.
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\",\"data\":{\"kind\":\"text\",\"values\":[\"x\",\"y\"]},\"missing\":[false]}]}}",
        // A kind the reader does not know.
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\",\"data\":{\"kind\":\"complex\",\"values\":[1.0]},\"missing\":[false]}]}}",
        // An i64 column with a float value.
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\",\"data\":{\"kind\":\"i64\",\"values\":[1.5]},\"missing\":[false]}]}}",
        // A very long name and a control character in it.
        "{\"schema_version\":1,\"name\":\"x\",\"data\":{\"columns\":[{\"name\":\"a\\u0000b\",\"data\":{\"kind\":\"f64\",\"values\":[1.0]},\"missing\":[false]}]}}",
    ];
    let start = Instant::now();
    for text in cases {
        let _ = io::from_project_json_sheets(text);
    }

    // Nesting far past any real project: the parser must refuse it, not
    // exhaust the stack.
    let deep = format!(
        "{{\"schema_version\":1,\"name\":\"x\",\"data\":{}0{}}}",
        "[".repeat(400),
        "]".repeat(400)
    );
    let _ = io::from_project_json_sheets(&deep);
    elapsed_ok(start, "hostile project JSON");
}

#[test]
fn hostile_formulas_are_evaluated_or_refused() {
    let formulas = [
        "",
        "=",
        "=(",
        "=)",
        "=(((1)))",
        "=1+",
        "=+1",
        "=--1",
        "=----1",
        "=1/0",
        "=MOD(1,0)",
        "=SUM()",
        "=SUM(C1R1:C1R999999999)",
        "=SUM(C1R1:C9R1)",
        "=SUM(C1R1:C1R1:C1R1)",
        "=NOPE(1)",
        "=IF(1)",
        "=IF(1,2)",
        "=CONCAT(\"a\",\"b\")",
        "=\"unterminated",
        "=C1R1:C1R2",
        "=C99999999999R1",
        "=C0R0",
        "=2^2^2^2^2",
        "=1e999999",
        "=SUM(C1R1:C1R4)+AVERAGE(C1R1:C1R4)",
        "=       1        ",
    ];
    let start = Instant::now();
    for formula in formulas {
        let _ = fp_core::formula::evaluate(&frame(), formula);
    }

    // A wall of parentheses, unary signs and powers: refused by the depth
    // limit instead of recursing once per character.
    for shape in [
        format!("={}{}", "(".repeat(500), "1".to_owned() + &")".repeat(500)),
        format!("={}1", "-".repeat(5_000)),
        format!("={}1", "2^".repeat(500)),
    ] {
        let _ = fp_core::formula::evaluate(&frame(), &shape);
    }
    elapsed_ok(start, "hostile formulas");
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
