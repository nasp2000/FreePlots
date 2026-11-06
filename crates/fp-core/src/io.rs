//! Input and output: CSV and `.fpp` project file.
//!
//! Industrial reality in pt-PT (PLAN.md, 6.1): `;` as separator, decimal
//! comma, CP1252 and BOM. Detection is explicit, never silent, and ambiguity
//! (`3,5` = three and a half? three thousand five hundred?) is resolved by the
//! rule declared in [`detect_options`] and appears in the UI preview.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::PROJECT_SCHEMA_VERSION;
use crate::data::{Column, ColumnData, DType, DataFrame};
use crate::error::{Error, Result};

/// CSV read options, after detection or as forced by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvOptions {
    /// Field separator.
    pub delimiter: u8,
    /// `true` if the comma is the decimal separator.
    pub decimal_comma: bool,
    /// `true` if the first row is a header.
    pub has_header: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            decimal_comma: false,
            has_header: true,
        }
    }
}

/// Explicit detection of separator and decimal from the text.
///
/// Rule: the most frequent separator on the line with the most candidates is
/// chosen; the comma is only treated as decimal when the separator is `;` and
/// there is no dot anywhere in the file (otherwise `1.234,56` would be
/// ambiguous).
#[must_use]
pub fn detect_options(text: &str) -> CsvOptions {
    let first_line = text.lines().next().unwrap_or("");
    let semi = first_line.matches(';').count();
    let tab = first_line.matches('\t').count();
    let comma = first_line.matches(',').count();

    let delimiter = if semi > 0 && semi >= tab && semi >= comma {
        b';'
    } else if tab > 0 && tab >= comma {
        b'\t'
    } else {
        b','
    };

    // Only counts as decimal if there are commas that are not the separator.
    let decimal_comma = delimiter == b';' && text.contains(',') && !text.contains('.');
    CsvOptions {
        delimiter,
        decimal_comma,
        has_header: true,
    }
}

/// Decodes bytes into text: UTF-8 (with or without BOM) or CP1252.
///
/// Also returns the name of the detected encoding, for the report.
#[must_use]
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static str) {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_owned(), "UTF-8"),
        Err(_) => (decode_cp1252(bytes), "CP1252"),
    }
}

fn decode_cp1252(bytes: &[u8]) -> String {
    // Only 0x80..=0x9F differs from Latin-1; the remaining bytes map 1:1.
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| {
            if (0x80..=0x9F).contains(&b) {
                HIGH[(b - 0x80) as usize]
            } else {
                char::from(b)
            }
        })
        .collect()
}

fn parse_f64(raw: &str, decimal_comma: bool) -> Option<f64> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    // `NaN`, `inf` and `-inf` are accepted as numbers: the sample policies
    // drop and count non-finite values, which is better than turning the whole
    // column into text because an export wrote one missing value that way.
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    if decimal_comma {
        let replaced = t.replace(',', ".");
        if replaced.matches('.').count() == 1
            && let Ok(v) = replaced.parse::<f64>()
        {
            return Some(v);
        }
    }
    None
}

/// Reads CSV from text, with the given options.
pub fn read_csv_str(text: &str, options: CsvOptions) -> Result<DataFrame> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(options.delimiter)
        .has_headers(options.has_header)
        .flexible(false)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = if options.has_header {
        reader
            .headers()
            .map_err(|e| csv_error(&e))?
            .iter()
            .map(|h| h.trim().to_owned())
            .collect()
    } else {
        Vec::new()
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut expected_width: Option<usize> = None;
    for rec in reader.records() {
        let rec = rec.map_err(|e| csv_error(&e))?;
        let width = rec.len();
        match expected_width {
            None => expected_width = Some(width),
            Some(w) if w != width => {
                return Err(Error::Csv(format!(
                    "line {}: {} fields, expected {}",
                    rec.position().map_or(0, csv::Position::line),
                    width,
                    w
                )));
            }
            Some(_) => {}
        }
        rows.push(rec.iter().map(|f| f.trim().to_owned()).collect());
    }

    let width = expected_width.unwrap_or(headers.len());
    if width == 0 {
        return Err(Error::Csv("file with no columns".to_owned()));
    }

    let names: Vec<String> = (0..width)
        .map(|i| {
            headers
                .get(i)
                .filter(|h| !h.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("C{}", i + 1))
        })
        .collect();

    let mut columns = Vec::with_capacity(width);
    for (ci, name) in names.iter().enumerate() {
        let raw: Vec<&str> = rows.iter().map(|r| r[ci].as_str()).collect();
        columns.push(build_column(name, &raw, options.decimal_comma)?);
    }

    DataFrame::new(columns)
}

fn build_column(name: &str, raw: &[&str], decimal_comma: bool) -> Result<Column> {
    let non_empty = raw.iter().filter(|s| !s.is_empty()).count();
    // Fully empty column: stays numeric so it can receive data later.
    if non_empty == 0 {
        return Ok(Column::new_f64(name, vec![f64::NAN; raw.len()]));
    }

    let all_int = raw.iter().all(|s| s.is_empty() || s.parse::<i64>().is_ok());
    if all_int {
        let values: Vec<i64> = raw
            .iter()
            .map(|s| s.parse::<i64>().unwrap_or(i64::MIN))
            .collect();
        let missing: Vec<bool> = raw.iter().map(|s| s.is_empty()).collect();
        return Column::from_parts(name, ColumnData::I64(values), missing);
    }

    let parsed: Vec<Option<f64>> = raw.iter().map(|s| parse_f64(s, decimal_comma)).collect();
    let numeric_share = parsed.iter().filter(|p| p.is_some()).count();
    if numeric_share == non_empty {
        let values: Vec<f64> = parsed.iter().map(|p| p.unwrap_or(f64::NAN)).collect();
        let missing: Vec<bool> = parsed.iter().map(Option::is_none).collect();
        return Column::from_parts(name, ColumnData::F64(values), missing);
    }

    let all_dates = raw
        .iter()
        .all(|value| value.trim().is_empty() || crate::data::parse_date(value).is_some());
    if all_dates {
        let values: Vec<i64> = raw
            .iter()
            .map(|value| {
                if value.trim().is_empty() {
                    i64::MIN
                } else {
                    crate::data::parse_date(value).unwrap_or(i64::MIN)
                }
            })
            .collect();
        let missing: Vec<bool> = raw.iter().map(|s| s.trim().is_empty()).collect();
        return Column::from_parts(name, ColumnData::Date(values), missing);
    }

    let values: Vec<String> = raw.iter().map(|s| (*s).to_owned()).collect();
    let missing: Vec<bool> = raw.iter().map(|s| s.is_empty()).collect();
    Column::from_parts(name, ColumnData::Text(values), missing)
}

fn csv_error(e: &csv::Error) -> Error {
    let where_ = e
        .position()
        .map(|p| format!("line {}, record {}", p.line(), p.record()))
        .unwrap_or_else(|| "unknown position".to_owned());
    Error::Csv(format!("{where_}: {e}"))
}

/// Reads a CSV file, detecting encoding and options.
pub fn read_csv_file(path: impl AsRef<Path>) -> Result<(DataFrame, &'static str)> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| Error::Csv(format!("{}: {e}", path.display())))?;
    let (text, encoding) = decode_bytes(&bytes);
    let options = detect_options(&text);
    let df = read_csv_str(&text, options)?;
    Ok((df, encoding))
}

/// Reads a CSV file with explicit options (the preview overrides).
///
/// # Errors
///
/// As [`read_csv_file`].
pub fn read_csv_file_with_options(
    path: impl AsRef<Path>,
    options: CsvOptions,
) -> Result<(DataFrame, &'static str)> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| Error::Csv(format!("{}: {e}", path.display())))?;
    let (text, encoding) = decode_bytes(&bytes);
    let df = read_csv_str(&text, options)?;
    Ok((df, encoding))
}

/// First lines of a CSV file together with the detected options.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvPreview {
    /// Up to 20 source lines, decoded as text.
    pub text: String,
    /// Detected separator, decimal and header options.
    pub options: CsvOptions,
    /// Detected encoding name (`UTF-8` or `CP1252`).
    pub encoding: String,
}

/// Reads the preview of a CSV file (at most 20 lines) and detects its options.
///
/// # Errors
///
/// Returns [`Error::Csv`] when the file cannot be read.
pub fn preview_csv_file(path: impl AsRef<Path>) -> Result<CsvPreview> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| Error::Csv(format!("{}: {e}", path.display())))?;
    let (text, encoding) = decode_bytes(&bytes);
    let options = detect_options(&text);
    let preview = text.lines().take(20).collect::<Vec<&str>>().join("\n");
    Ok(CsvPreview {
        text: preview,
        options,
        encoding: encoding.to_owned(),
    })
}

/// Writes the table to CSV.
///
/// By default it saves in "Excel pt-PT" format: `;` and decimal comma. With
/// `decimal_comma: false` it writes the neutral format (`,` and dot).
pub fn write_csv_string(df: &DataFrame, decimal_comma: bool) -> Result<String> {
    let delimiter = if decimal_comma { b';' } else { b',' };
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    let header: Vec<&str> = df.columns().iter().map(Column::name).collect();
    wtr.write_record(&header)
        .map_err(|e| Error::Csv(e.to_string()))?;

    for row in 0..df.n_rows() {
        let mut record: Vec<String> = Vec::with_capacity(df.n_columns());
        for col in df.columns() {
            if col.is_missing(row) {
                record.push(String::new());
                continue;
            }
            let cell = match col.data() {
                ColumnData::F64(v) => {
                    let s = format!("{}", v[row]);
                    if decimal_comma {
                        s.replace('.', ",")
                    } else {
                        s
                    }
                }
                ColumnData::I64(v) => v[row].to_string(),
                ColumnData::Text(v) => v[row].clone(),
                ColumnData::Bool(v) => v[row].to_string(),
                ColumnData::Date(v) => crate::data::format_date(v[row]),
            };
            record.push(cell);
        }
        wtr.write_record(&record)
            .map_err(|e| Error::Csv(e.to_string()))?;
    }

    let bytes = wtr.into_inner().map_err(|e| Error::Csv(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| Error::Csv(e.to_string()))
}

/// One worksheet inside a `.fpp` project.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectSheet {
    name: String,
    data: DataFrame,
}

/// Content of the `.fpp` project file.
///
/// Schema 1 held a single `data` frame; schema 2 holds `sheets`. Files written
/// by either version are read: a schema-1 file becomes a one-sheet project.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectFile {
    schema_version: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<DataFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sheets: Option<Vec<ProjectSheet>>,
}

/// Serializes the tables into a `.fpp` project (readable, diffable JSON).
///
/// # Errors
///
/// As [`to_project_json`].
pub fn to_project_json_sheets(sheets: &[(String, DataFrame)], name: &str) -> Result<String> {
    let sheet = |(sheet_name, data): &(String, DataFrame)| ProjectSheet {
        name: sheet_name.clone(),
        data: data.clone(),
    };
    let project = ProjectFile {
        schema_version: PROJECT_SCHEMA_VERSION,
        name: name.to_owned(),
        data: None,
        sheets: Some(sheets.iter().map(sheet).collect()),
    };
    serde_json::to_string_pretty(&project).map_err(|e| Error::Project(e.to_string()))
}

/// Reads an `.fpp` project, refusing future schema versions.
///
/// # Errors
///
/// A newer `schema_version`, malformed JSON, or a file with no sheet.
pub fn from_project_json_sheets(text: &str) -> Result<Vec<(String, DataFrame)>> {
    let project: ProjectFile =
        serde_json::from_str(text).map_err(|e| Error::Project(e.to_string()))?;
    if project.schema_version > PROJECT_SCHEMA_VERSION {
        return Err(Error::Project(format!(
            "schema_version {} is newer than this FreePlots supports ({}); \
             update the program",
            project.schema_version, PROJECT_SCHEMA_VERSION
        )));
    }
    if let Some(sheets) = project.sheets.filter(|sheets| !sheets.is_empty()) {
        return Ok(sheets
            .into_iter()
            .map(|sheet| (sheet.name, sheet.data))
            .collect());
    }
    match project.data {
        Some(data) => Ok(vec![(project.name, data)]),
        None => Err(Error::Project(
            "the project contains no worksheet".to_owned(),
        )),
    }
}

/// Serializes one table into a `.fpp` project (readable, diffable JSON).
pub fn to_project_json(df: &DataFrame, name: &str) -> Result<String> {
    to_project_json_sheets(&[(name.to_owned(), df.clone())], name)
}

/// Reads the first worksheet of an `.fpp` project.
pub fn from_project_json(text: &str) -> Result<DataFrame> {
    from_project_json_sheets(text).map(|mut sheets| {
        let (_, data) = sheets.remove(0);
        data
    })
}

/// Saves an `.fpp` project.
pub fn save_project(df: &DataFrame, path: impl AsRef<Path>, name: &str) -> Result<()> {
    save_project_sheets(&[(name.to_owned(), df.clone())], path, name)
}

/// Saves every worksheet of an `.fpp` project.
///
/// # Errors
///
/// As [`save_project`].
pub fn save_project_sheets(
    sheets: &[(String, DataFrame)],
    path: impl AsRef<Path>,
    name: &str,
) -> Result<()> {
    let path = path.as_ref();
    let json = to_project_json_sheets(sheets, name)?;
    std::fs::write(path, json).map_err(|e| Error::Project(format!("{}: {e}", path.display())))
}

/// Loads an `.fpp` project.
pub fn load_project(path: impl AsRef<Path>) -> Result<DataFrame> {
    load_project_sheets(path).map(|mut sheets| {
        let (_, data) = sheets.remove(0);
        data
    })
}

/// Loads every worksheet of an `.fpp` project, in file order.
///
/// # Errors
///
/// As [`load_project`].
pub fn load_project_sheets(path: impl AsRef<Path>) -> Result<Vec<(String, DataFrame)>> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Project(format!("{}: {e}", path.display())))?;
    from_project_json_sheets(&text)
}

/// Detected type for a column, useful for the import preview.
#[must_use]
pub fn column_dtype(col: &Column) -> DType {
    col.dtype()
}
