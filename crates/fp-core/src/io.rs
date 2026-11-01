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

/// Content of the `.fpp` project file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectFile {
    schema_version: u32,
    name: String,
    data: DataFrame,
}

/// Serializes the table into a `.fpp` project (readable, diffable JSON).
pub fn to_project_json(df: &DataFrame, name: &str) -> Result<String> {
    let project = ProjectFile {
        schema_version: PROJECT_SCHEMA_VERSION,
        name: name.to_owned(),
        data: df.clone(),
    };
    serde_json::to_string_pretty(&project).map_err(|e| Error::Project(e.to_string()))
}

/// Reads an `.fpp` project, refusing future schema versions.
pub fn from_project_json(text: &str) -> Result<DataFrame> {
    let project: ProjectFile =
        serde_json::from_str(text).map_err(|e| Error::Project(e.to_string()))?;
    if project.schema_version > PROJECT_SCHEMA_VERSION {
        return Err(Error::Project(format!(
            "schema_version {} is newer than this FreePlots supports ({}); \
             update the program",
            project.schema_version, PROJECT_SCHEMA_VERSION
        )));
    }
    Ok(project.data)
}

/// Saves an `.fpp` project.
pub fn save_project(df: &DataFrame, path: impl AsRef<Path>, name: &str) -> Result<()> {
    let path = path.as_ref();
    let json = to_project_json(df, name)?;
    std::fs::write(path, json).map_err(|e| Error::Project(format!("{}: {e}", path.display())))
}

/// Loads an `.fpp` project.
pub fn load_project(path: impl AsRef<Path>) -> Result<DataFrame> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Project(format!("{}: {e}", path.display())))?;
    from_project_json(&text)
}

/// Detected type for a column, useful for the import preview.
#[must_use]
pub fn column_dtype(col: &Column) -> DType {
    col.dtype()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;

    #[test]
    fn detects_iso_date_columns() {
        let text = "when;value\n2026-10-21;1\n2026-10-22;2\n";
        let options = CsvOptions {
            has_header: true,
            delimiter: b';',
            ..CsvOptions::default()
        };
        let df = read_csv_str(text, options).expect("frame");
        assert_eq!(df.columns()[0].dtype(), crate::data::DType::Date);
        assert_eq!(df.cell_text(0, 0).as_deref(), Some("2026-10-21"));
    }

    #[test]
    fn bom_and_latin1_text_are_decoded() {
        let (text, encoding) = decode_bytes(b"\xEF\xBB\xBFa;b\n1;2\n");
        assert_eq!(encoding, "UTF-8");
        assert!(text.starts_with("a;b"));
        let (text, encoding) = decode_bytes(b"cidade\nS\xE3o Paulo\nA\xE7ores\n");
        assert_eq!(encoding, "CP1252");
        assert!(text.contains("São Paulo"));
        assert!(text.contains("Açores"));
    }

    #[test]
    fn a_ragged_csv_is_rejected_with_the_line() {
        let path = std::env::temp_dir().join(format!(
            "fp-ragged-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos())
        ));
        std::fs::write(&path, "a,b\n1,2\n3\n").expect("write");
        let error = read_csv_file(&path).expect_err("ragged").to_string();
        assert!(error.contains("line 3"), "{error}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn non_finite_literals_stay_numeric() {
        let text = "x\n1\nnan\n3\ninf\n-Infinity\n";
        let df = read_csv_str(text, detect_options(text)).expect("frame");
        assert_eq!(df.columns()[0].dtype(), crate::data::DType::Float);
        // A missing value written as NaN or inf must not turn the column into
        // text; the sample drops and counts it.
        let sample = df
            .column("x")
            .unwrap()
            .sample(MissingPolicy::Drop)
            .expect("sample");
        assert_eq!(sample.n(), 2);
        assert_eq!(sample.excluded(), 3);
    }

    #[test]
    fn detects_semicolon_and_decimal_comma() {
        let o = detect_options("a;b\n1,5;2,5\n");
        assert_eq!(o.delimiter, b';');
        assert!(o.decimal_comma);
    }

    #[test]
    fn detects_simple_comma_as_separator() {
        let o = detect_options("a,b,c\n1,2,3\n");
        assert_eq!(o.delimiter, b',');
        assert!(!o.decimal_comma);
    }

    #[test]
    fn does_not_confuse_american_thousands_with_decimal() {
        // With a dot in the file, the comma is not treated as decimal.
        let o = detect_options("a;b\n1 234.56;7\n");
        assert_eq!(o.delimiter, b';');
        assert!(!o.decimal_comma);
    }

    #[test]
    fn reads_csv_with_decimal_comma() {
        let text = "peso;lote\n12,5;A\n13,25;A\n";
        let df = read_csv_str(text, detect_options(text)).unwrap();
        assert_eq!(df.n_rows(), 2);
        let s = df
            .column("peso")
            .unwrap()
            .sample(MissingPolicy::Drop)
            .unwrap();
        assert_eq!(s.values(), &[12.5, 13.25]);
        assert_eq!(df.column("lote").unwrap().dtype(), DType::Text);
    }

    #[test]
    fn reads_integers_as_integers() {
        let df = read_csv_str("n\n1\n2\n3\n", CsvOptions::default()).unwrap();
        assert_eq!(df.column("n").unwrap().dtype(), DType::Integer);
    }

    #[test]
    fn empty_cells_are_missing() {
        let df = read_csv_str("a,b\n1,10\n,20\n3,30\n", CsvOptions::default()).unwrap();
        let col = df.column("a").unwrap();
        assert_eq!(col.len(), 3);
        assert!(col.is_missing(1));
        let s = col.sample(MissingPolicy::Drop).unwrap();
        assert_eq!(s.values(), &[1.0, 3.0]);
    }

    #[test]
    fn fully_empty_rows_are_ignored() {
        // Documented: a blank line is not a record with an empty cell.
        let df = read_csv_str("x\n1\n\n3\n", CsvOptions::default()).unwrap();
        assert_eq!(df.n_rows(), 2);
    }

    #[test]
    fn accepts_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"x\n1\n");
        let (text, enc) = decode_bytes(&bytes);
        assert_eq!(enc, "UTF-8");
        assert_eq!(text, "x\n1\n");
    }

    #[test]
    fn decodes_cp1252() {
        // 0xE3 = 'ã' in CP1252; 0xE7 = 'ç'.
        let (text, enc) = decode_bytes(&[0xE3, 0xE7]);
        assert_eq!(enc, "CP1252");
        assert_eq!(text, "ãç");
        // 0x80 = '€' (differs from Latin-1).
        assert_eq!(decode_bytes(&[0x80]).0, "€");
    }

    #[test]
    fn rows_with_different_width_are_an_error() {
        let e = read_csv_str("a,b\n1,2\n3\n", CsvOptions::default()).unwrap_err();
        assert!(matches!(e, Error::Csv(_)));
    }

    #[test]
    fn csv_roundtrip() {
        let df = read_csv_str("x;y\n1,5;a\n2,5;b\n", detect_options("x;y\n1,5;a")).unwrap();
        let out = write_csv_string(&df, true).unwrap();
        assert!(out.contains("1,5"), "output: {out}");
        let back = read_csv_str(&out, detect_options(&out)).unwrap();
        assert_eq!(back.n_rows(), df.n_rows());
        assert_eq!(
            back.column("x").unwrap().dtype(),
            df.column("x").unwrap().dtype()
        );
    }

    #[test]
    fn edited_cells_survive_csv_roundtrip() {
        let input = "x;y\n1,5;a\n2,5;b\n";
        let mut df = read_csv_str(input, detect_options(input)).unwrap();
        df.set_cell(0, 0, "3,25").unwrap();
        df.set_cell(0, 1, "*").unwrap();
        df.set_cell(1, 1, "changed").unwrap();

        let out = write_csv_string(&df, true).unwrap();
        let back = read_csv_str(&out, detect_options(&out)).unwrap();
        assert_eq!(back.cell_text(0, 0).as_deref(), Some("3.25"));
        assert_eq!(back.cell_text(0, 1).as_deref(), Some("*"));
        assert!(back.columns()[0].is_missing(1));
        assert_eq!(back.cell_text(1, 1).as_deref(), Some("changed"));
    }

    #[test]
    #[ignore = "phase 1 import budget; run on the target machine"]
    fn import_one_million_cells_stays_inside_the_budget() {
        use std::fmt::Write as _;
        use std::time::{Duration, Instant};

        let rows = 50_000;
        let columns = 20;
        let mut text = String::with_capacity(rows * columns * 8);
        for column in 1..=columns {
            let _ = write!(text, "C{column}");
            if column < columns {
                text.push(',');
            }
        }
        text.push('\n');
        for row in 0..rows {
            for column in 0..columns {
                let _ = write!(text, "{}", row as f64 * 0.01 + column as f64);
                if column + 1 < columns {
                    text.push(',');
                }
            }
            text.push('\n');
        }

        let started = Instant::now();
        let df = read_csv_str(&text, detect_options(&text)).expect("import");
        let elapsed = started.elapsed();
        println!("1M cells import: {elapsed:?}");
        assert_eq!(df.n_rows(), rows);
        assert_eq!(df.n_columns(), columns);
        assert!(elapsed <= Duration::from_secs(3), "import took {elapsed:?}");
    }

    #[test]
    fn project_roundtrip() {
        let df = read_csv_str("x\n1.5\n2.5\n", CsvOptions::default()).unwrap();
        let json = to_project_json(&df, "test").unwrap();
        assert!(json.contains("\"schema_version\""));
        let back = from_project_json(&json).unwrap();
        assert_eq!(back, df);
    }

    #[test]
    fn future_schema_is_rejected() {
        let json = r#"{"schema_version": 99, "name": "x", "data": {"columns": []}}"#;
        let e = from_project_json(json).unwrap_err();
        assert!(matches!(e, Error::Project(_)));
        assert!(e.to_string().contains("99"));
    }

    #[test]
    fn text_column_does_not_become_a_number() {
        let df = read_csv_str("x\n1\nabc\n", CsvOptions::default()).unwrap();
        assert_eq!(df.column("x").unwrap().dtype(), DType::Text);
    }

    #[test]
    fn the_preview_detects_the_semicolon_and_decimal_comma() {
        let path = std::env::temp_dir().join("freeplots_preview.csv");
        std::fs::write(&path, "weight;height\n12,5;10\n13,5;11\n").expect("write");
        let preview = preview_csv_file(&path).expect("preview");
        assert_eq!(preview.options.delimiter, b';');
        assert!(preview.options.decimal_comma);
        assert_eq!(preview.encoding, "UTF-8");
        assert!(preview.text.contains("12,5"));
        let (df, _) = read_csv_file_with_options(&path, preview.options).expect("read");
        assert_eq!(df.n_rows(), 2);
        assert_eq!(df.column("weight").expect("weight").value(0), Some(12.5));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_preview_can_force_a_headerless_comma_file() {
        let path = std::env::temp_dir().join("freeplots_preview2.csv");
        std::fs::write(&path, "1,2\n3,4\n").expect("write");
        let preview = preview_csv_file(&path).expect("preview");
        let options = CsvOptions {
            delimiter: b',',
            decimal_comma: false,
            has_header: false,
        };
        let (df, _) = read_csv_file_with_options(&path, options).expect("read");
        assert_eq!(df.n_rows(), 2);
        assert_eq!(df.n_columns(), 2);
        assert_eq!(preview.options.delimiter, b',');
        let _ = std::fs::remove_file(path);
    }
}
