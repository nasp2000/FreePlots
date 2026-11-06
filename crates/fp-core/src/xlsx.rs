//! Minimal `.xlsx` writer (Office Open XML, ZIP with stored entries).
//!
//! The writer produces a single-sheet workbook with the worksheet values of a
//! [`DataFrame`]: text cells are written as inline strings and numeric cells as
//! numbers. No external dependency is used: the ZIP container is written with
//! stored (uncompressed) entries.

use crate::data::{DType, DataFrame};

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

fn zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let directory_offset = out.len() as u32;
    let directory_size = directory.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn column_name(index: usize) -> String {
    let mut name = String::new();
    let mut value = index + 1;
    while value > 0 {
        let rem = (value - 1) % 26;
        name.insert(0, (b'A' + u8::try_from(rem).expect("digit")) as char);
        value = (value - 1) / 26;
    }
    name
}

/// Builds a single-sheet `.xlsx` workbook from a worksheet.
#[must_use]
pub fn workbook(df: &DataFrame) -> Vec<u8> {
    let mut rows = String::new();
    let mut header = String::new();
    for (index, column) in df.columns().iter().enumerate() {
        header.push_str(&format!(
            "<c r=\"{}{}\" s=\"1\" t=\"inlineStr\"><is><t>{}</t></is></c>",
            column_name(index),
            1,
            escape(column.name())
        ));
    }
    rows.push_str(&format!("<row r=\"1\">{}</row>", header));
    for (row_index, row) in (0..df.n_rows()).enumerate() {
        let mut cells = String::new();
        for (column_index, column) in df.columns().iter().enumerate() {
            let reference = format!("{}{}", column_name(column_index), row_index + 2);
            // Missing cells are written empty; `cell_text` shows them as `*`,
            // which must never reach the file.
            if df.cell_is_missing(column_index, row) {
                cells.push_str(&format!(
                    "<c r=\"{reference}\" t=\"inlineStr\"><is><t></t></is></c>"
                ));
                continue;
            }
            let text = df.cell_text(column_index, row).unwrap_or_default();
            match column.dtype() {
                DType::Float | DType::Integer => {
                    let number = text.replace(',', ".");
                    if number.parse::<f64>().is_ok() {
                        cells.push_str(&format!("<c r=\"{reference}\"><v>{number}</v></c>"));
                    } else {
                        cells.push_str(&format!(
                            "<c r=\"{reference}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                            escape(&text)
                        ));
                    }
                }
                DType::Date => {
                    // Excel serial: days since 1899-12-30 (the 1900 date system).
                    if let Some(days) = column.date(row) {
                        let serial = days + 25_569;
                        cells
                            .push_str(&format!("<c r=\"{reference}\" s=\"2\"><v>{serial}</v></c>"));
                    } else {
                        cells.push_str(&format!(
                            "<c r=\"{reference}\" t=\"inlineStr\"><is><t></t></is></c>"
                        ));
                    }
                }
                _ => {
                    cells.push_str(&format!(
                        "<c r=\"{reference}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                        escape(&text)
                    ));
                }
            }
        }
        rows.push_str(&format!("<row r=\"{}\">{}</row>", row_index + 2, cells));
    }
    // The header row is frozen and every column is as wide as its header, so
    // the workbook opens as a readable table.
    let mut cols = String::new();
    for (index, column) in df.columns().iter().enumerate() {
        let width = (column.name().chars().count() + 4).clamp(9, 40);
        cols.push_str(&format!(
            "<col min=\"{}\" max=\"{}\" width=\"{width}\" customWidth=\"1\"/>",
            index + 1,
            index + 1
        ));
    }
    let sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
         <sheetViews><sheetView workbookViewId=\"0\">\
         <pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/>\
         </sheetView></sheetViews>\
         <cols>{cols}</cols>\
         <sheetData>{rows}</sheetData></worksheet>"
    );
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
        <Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
        <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\
        </Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
        </Relationships>";
    let workbook_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
        xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
        <sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    let workbook_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
        <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
        </Relationships>";
    // Body font, white bold header font, a blue header fill and a date format.
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
        <numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"yyyy-mm-dd\"/></numFmts>\
        <fonts count=\"2\">\
        <font><sz val=\"11\"/><name val=\"Calibri\"/></font>\
        <font><b/><sz val=\"11\"/><color rgb=\"FFFFFFFF\"/><name val=\"Calibri\"/></font>\
        </fonts>\
        <fills count=\"3\">\
        <fill><patternFill patternType=\"none\"/></fill>\
        <fill><patternFill patternType=\"gray125\"/></fill>\
        <fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF1F4E79\"/><bgColor indexed=\"64\"/></patternFill></fill>\
        </fills>\
        <borders count=\"1\"><border/></borders>\
        <cellStyleXfs count=\"1\"><xf/></cellStyleXfs>\
        <cellXfs count=\"3\">\
        <xf xfId=\"0\"/>\
        <xf xfId=\"0\" fontId=\"1\" fillId=\"2\" applyFont=\"1\" applyFill=\"1\"/>\
        <xf xfId=\"0\" numFmtId=\"164\" applyNumberFormat=\"1\"/>\
        </cellXfs>\
        </styleSheet>";
    zip(&[
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", root_rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook_xml.as_bytes().to_vec()),
        (
            "xl/_rels/workbook.xml.rels",
            workbook_rels.as_bytes().to_vec(),
        ),
        ("xl/styles.xml", styles.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
    ])
}
