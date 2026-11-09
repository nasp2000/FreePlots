// SPDX-License-Identifier: MIT
//! Reader for Excel workbooks (`.xlsx`).
//!
//! The importer understands the parts of the format that a data sheet uses:
//! the shared strings, the inline strings and the numbers of the first
//! worksheet. The archive is read with [`crate::zip`] and the cells are
//! decoded with a small XML scanner, so no dependency is needed.

use std::collections::BTreeMap;

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::zip::ZipArchive;

/// Reads the first worksheet of an `.xlsx` workbook.
///
/// # Errors
///
/// Returns [`Error`] when the archive is damaged, when the workbook has no
/// worksheet or when the cells cannot be decoded.
pub fn read_workbook(data: &[u8]) -> Result<DataFrame> {
    read_workbook_sheet(data, None)
}

/// Reads a worksheet of an `.xlsx` workbook by name (`None` for the first).
///
/// # Errors
///
/// Returns [`Error`] when the archive is damaged, when the worksheet is not in
/// the workbook (the message lists the available names) or when the cells
/// cannot be decoded.
pub fn read_workbook_sheet(data: &[u8], sheet_name: Option<&str>) -> Result<DataFrame> {
    let archive = ZipArchive::new(data.to_vec())?;
    let shared = match archive.read_suffix("sharedStrings.xml") {
        Ok(bytes) => parse_shared_strings(&String::from_utf8_lossy(&bytes)),
        Err(_) => Vec::new(),
    };
    let sheet = match sheet_name {
        Some(name) => read_named_sheet(&archive, name)?,
        None => read_first_sheet(&archive)?,
    };
    let date_styles = read_date_styles(&archive);
    let table = parse_sheet(&sheet, &shared, &date_styles)?;
    build_frame(table)
}

/// Number of formula cells in a worksheet (`<f>` tags).
///
/// The reader uses the cached value of a formula; this count lets the
/// interface warn when that happened.
///
/// # Errors
///
/// As [`read_workbook_sheet`].
pub fn sheet_formula_count(data: &[u8], sheet_name: Option<&str>) -> Result<usize> {
    let archive = ZipArchive::new(data.to_vec())?;
    let sheet = match sheet_name {
        Some(name) => read_named_sheet(&archive, name)?,
        None => read_first_sheet(&archive)?,
    };
    Ok(count_formulas(&sheet))
}

fn count_formulas(xml: &str) -> usize {
    xml.matches("<f>").count() + xml.matches("<f ").count()
}

/// Names of the worksheets, in workbook order.
pub fn sheet_names(data: &[u8]) -> Result<Vec<String>> {
    let archive = ZipArchive::new(data.to_vec())?;
    let workbook = archive
        .read_suffix("workbook.xml")
        .map_err(|_| Error::Unsupported("the workbook has no workbook.xml".to_owned()))?;
    let text = String::from_utf8_lossy(&workbook);
    Ok(sheet_entries(&text)
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

/// `(name, relationship id)` of every worksheet of the workbook.
fn sheet_entries(text: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut position = 0usize;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        if tag.name != "sheet" || tag.closing {
            continue;
        }
        let Some(name) = attribute(&tag, "name") else {
            continue;
        };
        let identifier = attribute(&tag, "r:id")
            .or_else(|| attribute(&tag, "id"))
            .unwrap_or_default();
        entries.push((name, identifier));
    }
    entries
}

/// Reads the worksheet with the given name.
fn read_named_sheet(archive: &ZipArchive, name: &str) -> Result<String> {
    let workbook = archive
        .read_suffix("workbook.xml")
        .map_err(|_| Error::Unsupported("the workbook has no workbook.xml".to_owned()))?;
    let workbook_text = String::from_utf8_lossy(&workbook);
    let entries = sheet_entries(&workbook_text);
    let Some((_, identifier)) = entries.iter().find(|(label, _)| label == name) else {
        let available: Vec<String> = entries.into_iter().map(|(label, _)| label).collect();
        return Err(Error::Unsupported(format!(
            "the workbook has no sheet named {name:?}; available: {}",
            if available.is_empty() {
                "none".to_owned()
            } else {
                available.join(", ")
            }
        )));
    };
    let relations = archive
        .read_suffix("workbook.xml.rels")
        .map_err(|_| Error::Unsupported("the workbook has no relationships".to_owned()))?;
    let relations_text = String::from_utf8_lossy(&relations);
    let target = relationship_target(&relations_text, identifier).ok_or_else(|| {
        Error::Unsupported(format!("the sheet relationship {identifier} has no target"))
    })?;
    for candidate in [
        format!("xl/{}", target.trim_start_matches('/')),
        target.trim_start_matches('/').to_owned(),
    ] {
        if let Ok(bytes) = archive.read(&candidate) {
            return Ok(String::from_utf8_lossy(&bytes).to_string());
        }
    }
    Err(Error::Unsupported(format!(
        "the worksheet {name:?} could not be read"
    )))
}

/// Target of the relationship with the given identifier.
fn relationship_target(text: &str, identifier: &str) -> Option<String> {
    let mut position = 0usize;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        if tag.name != "Relationship" {
            continue;
        }
        if attribute(&tag, "Id").as_deref() != Some(identifier) {
            continue;
        }
        return attribute(&tag, "Target");
    }
    None
}

/// Reads the XML of the first worksheet of the archive.
fn read_first_sheet(archive: &ZipArchive) -> Result<String> {
    // The workbook relationships name the worksheets; when they are missing,
    // the first worksheet of the archive is used.
    if let Ok(relations) = archive.read_suffix("workbook.xml.rels") {
        let text = String::from_utf8_lossy(&relations);
        if let Some(target) = first_relationship_target(&text) {
            let path = format!("xl/{target}");
            if let Ok(bytes) = archive.read(&path) {
                return Ok(String::from_utf8_lossy(&bytes).to_string());
            }
            let trimmed = target.trim_start_matches('/');
            if let Ok(bytes) = archive.read(trimmed) {
                return Ok(String::from_utf8_lossy(&bytes).to_string());
            }
        }
    }
    let bytes = archive.read_suffix("worksheets/sheet1.xml")?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// The target of the first worksheet relationship.
fn first_relationship_target(text: &str) -> Option<String> {
    let mut position = 0usize;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        if tag.name != "Relationship" {
            continue;
        }
        let kind = attribute(&tag, "Type").unwrap_or_default();
        if !kind.ends_with("/worksheet") {
            continue;
        }
        if let Some(target) = attribute(&tag, "Target") {
            return Some(target);
        }
    }
    None
}

/// Reads the shared strings of the workbook.
fn parse_shared_strings(text: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut position = 0usize;
    let mut current = String::new();
    let mut inside = false;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        match tag.name.as_str() {
            "si" if !tag.closing => {
                inside = true;
                current.clear();
            }
            "si" => {
                inside = false;
                strings.push(std::mem::take(&mut current));
            }
            "t" if inside && !tag.closing => {
                current.push_str(&text_content(text, position));
            }
            _ => {}
        }
    }
    strings
}

/// Styles whose number format is a date, as a flag per style index.
fn read_date_styles(archive: &ZipArchive) -> Vec<bool> {
    let Ok(bytes) = archive.read_suffix("styles.xml") else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    date_styles(&text)
}

/// Classifies the cell styles of `xl/styles.xml`.
///
/// Excel writes the dates as numbers whose **number format** says they are
/// dates. The built-in identifiers 14–22 and 45–47 are dates and times; a
/// custom format is a date when its code contains a day, month or year token
/// and is not a plain number or percentage.
fn date_styles(text: &str) -> Vec<bool> {
    // Custom formats: id → code.
    let mut formats: BTreeMap<u32, String> = BTreeMap::new();
    let mut position = 0usize;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        if tag.name != "numFmt" || tag.closing {
            continue;
        }
        let Some(id) = attribute(&tag, "numFmtId").and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(code) = attribute(&tag, "formatCode") else {
            continue;
        };
        formats.insert(id, code);
    }
    let mut flags = Vec::new();
    let mut position = 0usize;
    let mut inside_cell_xfs = false;
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        match tag.name.as_str() {
            "cellXfs" => inside_cell_xfs = !tag.closing,
            "xf" if inside_cell_xfs && !tag.closing => {
                let id = attribute(&tag, "numFmtId")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0);
                let is_date = match formats.get(&id) {
                    Some(code) => is_date_format(code),
                    None => matches!(id, 14..=22 | 45..=47),
                };
                flags.push(is_date);
            }
            _ => {}
        }
    }
    flags
}

/// Whether a number format code describes a date or a time.
fn is_date_format(code: &str) -> bool {
    // The literal text of a format sits in quotes and in brackets; only the
    // tokens outside them matter.
    let mut cleaned = String::with_capacity(code.len());
    let mut in_quotes = false;
    let mut in_brackets = false;
    for character in code.chars() {
        match character {
            '"' => in_quotes = !in_quotes,
            '[' if !in_quotes => in_brackets = true,
            ']' if !in_quotes => in_brackets = false,
            other if !in_quotes && !in_brackets => cleaned.push(other.to_ascii_lowercase()),
            _ => {}
        }
    }
    let has_date_token = cleaned.contains('y')
        || cleaned.contains('d')
        || cleaned.contains("m/")
        || cleaned.contains("/m");
    let has_time_token =
        cleaned.contains('h') || cleaned.contains(":mm") || cleaned.contains(":ss");
    let is_number = cleaned.contains('#') && !has_date_token;
    (has_date_token || has_time_token) && !is_number
}

/// Converts an Excel serial number to an ISO date (`YYYY-MM-DD`).
///
/// Serial 1 is 1900-01-01, and Excel keeps the fictitious 1900-02-29, so the
/// serials from 60 onwards are one day ahead of the real calendar.
fn excel_serial_to_iso(serial: f64) -> Option<String> {
    if !serial.is_finite() || serial <= 0.0 {
        return None;
    }
    let days = serial.floor() as i64;
    let offset = if days >= 60 { 25569 } else { 25568 };
    let timestamp = (days - offset) * 86_400;
    let text = crate::data::format_date(timestamp / 86_400);
    if text.is_empty() { None } else { Some(text) }
}

/// Reads the cells of a worksheet into a rectangular table of strings.
fn parse_sheet(text: &str, shared: &[String], date_styles: &[bool]) -> Result<Vec<Vec<String>>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut position = 0usize;
    let mut current: Vec<String> = Vec::new();
    let mut cell_type = String::new();
    let mut cell_style = 0usize;
    let mut column = 0usize;
    let mut inside_cell = false;
    let mut inside_value = false;
    let mut value = String::new();
    while let Some((tag, next)) = next_tag(text, position) {
        position = next;
        match tag.name.as_str() {
            "row" if !tag.closing => {
                current.clear();
            }
            "row" => {
                if !current.is_empty() {
                    rows.push(std::mem::take(&mut current));
                }
            }
            "c" if !tag.closing => {
                inside_cell = true;
                value.clear();
                cell_type = attribute(&tag, "t").unwrap_or_default();
                cell_style = attribute(&tag, "s")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                column = match attribute(&tag, "r") {
                    Some(reference) => column_index(&reference).ok_or_else(|| {
                        Error::Unsupported(format!(
                            "cell reference {reference} is outside the worksheet"
                        ))
                    })?,
                    None => current.len(),
                };
            }
            "c" => {
                inside_cell = false;
                let text = match cell_type.as_str() {
                    "s" => value
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| shared.get(index).cloned())
                        .unwrap_or_default(),
                    "b" => {
                        if value == "1" {
                            "TRUE".to_owned()
                        } else {
                            "FALSE".to_owned()
                        }
                    }
                    "" if date_styles.get(cell_style).copied().unwrap_or(false) => value
                        .parse::<f64>()
                        .ok()
                        .and_then(excel_serial_to_iso)
                        .unwrap_or_else(|| value.clone()),
                    _ => value.clone(),
                };
                while current.len() <= column {
                    current.push(String::new());
                }
                current[column] = text;
            }
            "v" | "t" if inside_cell && !tag.closing => {
                inside_value = true;
                value.clear();
            }
            "v" | "t" if inside_cell => {
                inside_value = false;
            }
            _ => {}
        }
        if inside_value && !tag.closing && !tag.self_closing {
            // The text of the element is read until the closing tag.
            let end = find_closing(text, position, &tag.name);
            value.push_str(&text[position..end]);
            position = end;
            inside_value = false;
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    Ok(rows)
}

/// The text between a tag and its closing tag.
fn text_content(text: &str, from: usize) -> String {
    let end = text[from..]
        .find('<')
        .map_or(text.len(), |index| from + index);
    text[from..end].to_string()
}

/// The position of the closing tag of an element.
fn find_closing(text: &str, from: usize, name: &str) -> usize {
    let needle = format!("</{name}");
    text[from..]
        .find(&needle)
        .map_or(text.len(), |index| from + index)
}

/// The zero-based column of a cell reference such as `AB12`.
///
/// Returns `None` when the letters overflow or name a column past Excel's last
/// column (`XFD`): a workbook can claim any string there, and a hostile one
/// must not make the reader allocate gigabytes.
fn column_index(reference: &str) -> Option<usize> {
    /// One past the last column Excel allows (`XFD`, zero-based 16383).
    const MAX_COLUMNS: usize = 16_384;
    let mut index = 0usize;
    let mut letters = 0usize;
    for character in reference.chars() {
        if !character.is_ascii_alphabetic() {
            break;
        }
        letters += 1;
        index = index
            .checked_mul(26)?
            .checked_add(character.to_ascii_uppercase() as usize - 'A' as usize + 1)?;
        if index > MAX_COLUMNS {
            return None;
        }
    }
    if letters == 0 { None } else { Some(index - 1) }
}

/// Builds a worksheet from the raw cells.
fn build_frame(table: Vec<Vec<String>>) -> Result<DataFrame> {
    let Some(header) = table.first() else {
        return Err(Error::EmptySample);
    };
    let width = table.iter().map(Vec::len).max().unwrap_or(header.len());
    let names: Vec<String> = (0..width)
        .map(|index| {
            let name = header.get(index).map_or("", String::as_str).trim();
            if name.is_empty() {
                format!("C{}", index + 1)
            } else {
                name.to_owned()
            }
        })
        .collect();
    let mut columns: Vec<Column> = Vec::with_capacity(width);
    for (index, name) in names.iter().enumerate() {
        let mut values: Vec<String> = Vec::with_capacity(table.len());
        for row in table.iter().skip(1) {
            values.push(row.get(index).cloned().unwrap_or_default());
        }
        let numeric = values
            .iter()
            .filter(|value| !value.trim().is_empty())
            .all(|value| value.trim().parse::<f64>().is_ok());
        if numeric {
            let numbers: Vec<f64> = values
                .iter()
                .map(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        f64::NAN
                    } else {
                        trimmed.parse::<f64>().unwrap_or(f64::NAN)
                    }
                })
                .collect();
            columns.push(Column::new_f64(name.clone(), numbers));
        } else {
            columns.push(Column::new_text(name.clone(), values));
        }
    }
    DataFrame::new(columns)
}

/// One XML tag.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    /// The name of the tag, without the namespace prefix.
    name: String,
    /// The attributes of the tag.
    attributes: Vec<(String, String)>,
    /// Whether the tag closes an element.
    closing: bool,
    /// Whether the tag is self-closing.
    self_closing: bool,
}

/// The value of an attribute of a tag.
fn attribute(tag: &Tag, name: &str) -> Option<String> {
    tag.attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

/// Reads the next XML tag of the text.
fn next_tag(text: &str, from: usize) -> Option<(Tag, usize)> {
    let mut position = from;
    loop {
        let start = text[position..].find('<')? + position;
        let rest = &text[start + 1..];
        if rest.starts_with('!') || rest.starts_with('?') {
            position = start + 1;
            continue;
        }
        let end = rest.find('>')? + start + 1;
        let body = &text[start + 1..end];
        let closing = body.starts_with('/');
        let body = body.trim_start_matches('/');
        let self_closing = body.ends_with('/');
        let body = body.trim_end_matches('/');
        let mut parts = body.split_whitespace();
        let name = parts.next().unwrap_or("").to_string();
        let name = name.rsplit(':').next().unwrap_or(&name).to_owned();
        let mut attributes = Vec::new();
        for part in parts {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            let value = value.trim_matches('"').trim_matches('\'');
            attributes.push((key.to_owned(), value.to_owned()));
        }
        return Some((
            Tag {
                name,
                attributes,
                closing,
                self_closing,
            },
            end + 1,
        ));
    }
}

/// The number of rows and the columns of a worksheet, for the tests.
pub fn sheet_dimensions(data: &[u8]) -> Result<(usize, usize)> {
    let archive = ZipArchive::new(data.to_vec())?;
    let sheet = read_first_sheet(&archive)?;
    let table = parse_sheet(&sheet, &[], &[])?;
    let width = table.iter().map(Vec::len).max().unwrap_or(0);
    Ok((table.len(), width))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;
    use crate::xlsx::workbook;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            crate::data::Column::new_f64("weight", vec![10.0, 10.5, 11.0]),
            crate::data::Column::new_text(
                "label",
                vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn a_workbook_written_by_the_engine_is_read_back() {
        let df = frame();
        let bytes = workbook(&df);
        let read = read_workbook(&bytes).expect("workbook");
        assert_eq!(read.n_rows(), 3);
        assert_eq!(read.n_columns(), 2);
        assert_eq!(read.columns()[0].name(), "weight");
        assert_eq!(read.columns()[1].name(), "label");
        let values = read.columns()[0]
            .numeric(MissingPolicy::Drop)
            .expect("numbers");
        assert!((values[0] - 10.0).abs() < 1e-9);
        assert!((values[2] - 11.0).abs() < 1e-9);
        assert_eq!(read.columns()[1].cell_text(1), "b");
    }

    #[test]
    fn the_sheets_are_listed_and_can_be_selected() {
        let bytes = workbook(&frame());
        let names = sheet_names(&bytes).expect("names");
        assert_eq!(names, vec!["Sheet1".to_owned()]);
        // Reading the only sheet by name gives the same table.
        let read = read_workbook_sheet(&bytes, Some("Sheet1")).expect("sheet");
        assert_eq!(read.n_rows(), 3);
        // A missing sheet lists the available ones.
        let error = read_workbook_sheet(&bytes, Some("Missing")).expect_err("missing sheet");
        let message = error.to_string();
        assert!(message.contains("Sheet1"), "{message}");
        assert!(message.contains("Missing"), "{message}");
        // No name means the first sheet.
        let first = read_workbook_sheet(&bytes, None).expect("first");
        assert_eq!(first.n_columns(), 2);
    }

    #[test]
    fn formulas_are_counted_and_fills_are_not() {
        assert_eq!(count_formulas("<c><f>1+1</f><v>2</v></c>"), 1);
        assert_eq!(
            count_formulas("<c><f t=\"shared\" si=\"0\"/><v>2</v></c>"),
            1
        );
        assert_eq!(count_formulas("<fills><fill/></fills>"), 0);
    }

    #[test]
    fn the_date_number_formats_are_recognised() {
        // Built-in formats: 14 is a date, 0 is general.
        let styles = r#"<styleSheet><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/></cellXfs><numFmts><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts></styleSheet>"#;
        let flags = date_styles(styles);
        assert_eq!(flags, vec![false, true, true]);
        // A currency format or a percentage is not a date.
        assert!(!is_date_format("#,##0.00"));
        assert!(!is_date_format("0.0%"));
        assert!(is_date_format("dd/mm/yyyy"));
        assert!(is_date_format("mmm-yy"));
        assert!(is_date_format("hh:mm:ss"));
    }

    #[test]
    fn excel_serials_become_iso_dates() {
        // 2024-01-01 is serial 45292; the leap-year bug shifts the early ones.
        assert_eq!(excel_serial_to_iso(45292.0).as_deref(), Some("2024-01-01"));
        assert_eq!(excel_serial_to_iso(25569.0).as_deref(), Some("1970-01-01"));
        assert_eq!(excel_serial_to_iso(1.0).as_deref(), Some("1900-01-01"));
        assert!(excel_serial_to_iso(0.0).is_none());
        assert!(excel_serial_to_iso(f64::NAN).is_none());
    }

    #[test]
    fn a_cell_with_a_date_style_is_read_as_a_date() {
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>date</t></is></c></row><row r="2"><c r="A2" s="1"><v>45292</v></c><c r="B2"><v>42</v></c></row></sheetData></worksheet>"#;
        let table = parse_sheet(sheet, &[], &[false, true]).expect("sheet");
        assert_eq!(table[1][0], "2024-01-01");
        // The unformatted neighbour stays a number.
        assert_eq!(table[1][1], "42");
    }

    #[test]
    fn absurd_cell_references_are_refused() {
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="ZZZZZZZZZZZZZZ1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;
        assert!(parse_sheet(sheet, &[], &[]).is_err());
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="XFD1"><v>1</v></c></row></sheetData></worksheet>"#;
        let table = parse_sheet(sheet, &[], &[]).expect("a column inside the Excel limit");
        assert_eq!(table[0].len(), 16_384);
    }

    #[test]
    fn the_dimensions_of_a_workbook_are_reported() {
        let bytes = workbook(&frame());
        let (rows, columns) = sheet_dimensions(&bytes).expect("dimensions");
        assert_eq!((rows, columns), (4, 2));
    }

    #[test]
    fn a_file_that_is_not_a_workbook_is_rejected() {
        assert!(read_workbook(b"not a workbook").is_err());
    }

    #[test]
    fn cell_references_map_to_columns() {
        assert_eq!(column_index("A1"), Some(0));
        assert_eq!(column_index("B7"), Some(1));
        assert_eq!(column_index("Z3"), Some(25));
        assert_eq!(column_index("AA3"), Some(26));
        assert_eq!(column_index("XFD1"), Some(16_383));
        assert_eq!(column_index("XFE1"), None);
        assert_eq!(column_index("ZZZZZZZZZZZZZZ1"), None);
        assert_eq!(column_index("1"), None);
    }
}
