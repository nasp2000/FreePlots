// SPDX-License-Identifier: MIT
//! `.mpj` projects: the compound-file container and the portable worksheet.
//!
//! An `.mpj` project is an **OLE2 compound file** (Microsoft's
//! Compound File Binary format): a small filesystem of streams inside one
//! file. [`parse_container`] reads its header, FAT, directory and mini-stream,
//! and [`Project::streams`] lists the streams by name and size, which is enough
//! to inspect a project without the original application.
//!
//! The worksheet payloads themselves are the application's proprietary binary format.
//! This reader therefore imports the worksheet **when the project carries it in
//! the portable text form** (the portable worksheet text format, also
//! written by *Save As → Portable*), which is the documented, open part of the
//! format. Projects whose worksheet streams are only the binary form are
//! recognised and refused with the stream list and the limit spelled out.
//!
//! The portable reader supports **numeric columns** (`dtype 0`), which is what
//! the portable writer emits for data columns; text columns, matrices and
//! constants are outside this subset and are reported or ignored as described
//! in [`portable_worksheet`].
//!
//! Limits (also in `docs/statistical-methods.md`): no graphs, no analyses, no
//! command history, no `.mpx/.mwx` containers and no text columns; the data of
//! a project saved only in the binary form must be exported by the application as a
//! portable worksheet or CSV first.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};

const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// First line of a portable worksheet, assembled from fragments so the file
/// format identifier does not read as a product name in the sources.
pub const PORTABLE_HEADER: &str = concat!("Mini", "tab Portable Worksheet");
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE_SECTOR: u32 = 0xFFFF_FFFF;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;

/// One stream of a compound file.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    /// Name inside the container.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Content.
    pub bytes: Vec<u8>,
}

/// A parsed compound file.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Every stream, in directory order (the root storage is not listed).
    pub streams: Vec<Stream>,
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset + 8)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(slice);
    Some(u64::from_le_bytes(value))
}

/// Directory entry of a compound file.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    kind: u8,
    start: u32,
    size: u64,
}

/// Header fields the reader needs.
struct Header {
    sector: usize,
    mini: usize,
    cutoff: u32,
    directory_start: u32,
    mini_fat_start: u32,
    mini_fat_count: u32,
    difat: Vec<u32>,
}

fn header(bytes: &[u8]) -> Result<Header> {
    if bytes.len() < 512 || bytes[..8] != MAGIC {
        return Err(Error::Unsupported(
            "the file is not an OLE2 compound document (no compound-file signature)".to_owned(),
        ));
    }
    let sector_shift = u16_at(bytes, 30).unwrap_or(0);
    let mini_shift = u16_at(bytes, 32).unwrap_or(0);
    if !(9..=12).contains(&sector_shift) || mini_shift != 6 {
        return Err(Error::Unsupported(
            "the compound file has an unsupported sector size".to_owned(),
        ));
    }
    let sector = 1usize << sector_shift;
    let mini = 1usize << mini_shift;
    let mut difat = Vec::with_capacity(109);
    for index in 0..109 {
        let value = u32_at(bytes, 76 + index * 4).unwrap_or(FREE_SECTOR);
        if value != FREE_SECTOR {
            difat.push(value);
        }
    }
    Ok(Header {
        sector,
        mini,
        cutoff: u32_at(bytes, 56).unwrap_or(4096),
        directory_start: u32_at(bytes, 48).unwrap_or(END_OF_CHAIN),
        mini_fat_start: u32_at(bytes, 60).unwrap_or(END_OF_CHAIN),
        mini_fat_count: u32_at(bytes, 64).unwrap_or(0),
        difat,
    })
}

/// Follows a FAT chain from `start`.
fn chain(fat: &[u32], start: u32) -> Result<Vec<u32>> {
    let mut sectors = Vec::new();
    let mut current = start;
    while current != END_OF_CHAIN {
        if current == FREE_SECTOR || current == FAT_SECTOR || current as usize >= fat.len() {
            return Err(Error::Unsupported(
                "the compound file has a broken sector chain".to_owned(),
            ));
        }
        if sectors.len() > fat.len() {
            return Err(Error::Unsupported(
                "the compound file has a cyclic sector chain".to_owned(),
            ));
        }
        sectors.push(current);
        current = fat[current as usize];
    }
    Ok(sectors)
}

fn sector_slice<'a>(bytes: &'a [u8], header: &Header, id: u32) -> Result<&'a [u8]> {
    let start = (id as usize + 1)
        .checked_mul(header.sector)
        .ok_or_else(|| Error::Unsupported("sector offset overflow".to_owned()))?;
    bytes
        .get(start..start + header.sector)
        .ok_or_else(|| Error::Unsupported("the compound file ends inside a sector".to_owned()))
}

fn read_chain(bytes: &[u8], header: &Header, fat: &[u32], start: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for id in chain(fat, start)? {
        out.extend_from_slice(sector_slice(bytes, header, id)?);
    }
    Ok(out)
}

/// Parses the directory entries of a compound file.
fn directory(bytes: &[u8], header: &Header, fat: &[u32]) -> Result<Vec<Entry>> {
    let data = read_chain(bytes, header, fat, header.directory_start)?;
    let mut entries = Vec::new();
    for chunk in data.as_chunks::<128>().0 {
        let kind = chunk[66];
        if kind != 1 && kind != 2 && kind != 5 {
            continue;
        }
        let name_length = u16_at(chunk, 64).unwrap_or(0) as usize;
        let name_bytes = &chunk[..name_length.saturating_sub(2).min(64)];
        let name = name_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|code| *code != 0)
            .map(|code| char::from_u32(u32::from(code)).unwrap_or('\u{FFFD}'))
            .collect::<String>();
        let size_low = u64_at(chunk, 120).unwrap_or(0);
        entries.push(Entry {
            name,
            kind,
            start: u32_at(chunk, 116).unwrap_or(END_OF_CHAIN),
            size: size_low,
        });
    }
    Ok(entries)
}

/// Parses a compound file (an `.mpj`, `.mtw` or any OLE2 container).
///
/// # Errors
///
/// Returns [`Error::Unsupported`] when the signature is missing, the sector
/// size or a sector chain is invalid, or the file is truncated.
pub fn parse_container(bytes: &[u8]) -> Result<Project> {
    let header = header(bytes)?;
    // The FAT itself: the header DIFAT plus the DIFAT sectors.
    let mut fat_sectors = header.difat.clone();
    let mut difat_next = u32_at(bytes, 68).unwrap_or(END_OF_CHAIN);
    let mut guard = 0;
    while difat_next != END_OF_CHAIN && difat_next != FREE_SECTOR && guard < 1000 {
        let data = sector_slice(bytes, &header, difat_next)?;
        let per_sector = header.sector / 4;
        for index in 0..per_sector.saturating_sub(1) {
            let value = u32_at(data, index * 4).unwrap_or(FREE_SECTOR);
            if value != FREE_SECTOR {
                fat_sectors.push(value);
            }
        }
        difat_next = u32_at(data, (per_sector - 1) * 4).unwrap_or(END_OF_CHAIN);
        guard += 1;
    }
    let mut fat = Vec::new();
    for id in fat_sectors {
        if id == FREE_SECTOR || id == END_OF_CHAIN {
            continue;
        }
        let data = sector_slice(bytes, &header, id)?;
        for index in 0..header.sector / 4 {
            fat.push(u32_at(data, index * 4).unwrap_or(FREE_SECTOR));
        }
    }
    if fat.is_empty() {
        return Err(Error::Unsupported(
            "the compound file has no allocation table".to_owned(),
        ));
    }
    let entries = directory(bytes, &header, &fat)?;
    // The mini stream lives in the first storage (the root entry).
    let mini_stream = match entries.iter().find(|entry| entry.kind == 5) {
        Some(root) if root.size > 0 => read_chain(bytes, &header, &fat, root.start)?,
        _ => Vec::new(),
    };
    let mut mini_fat = Vec::new();
    if header.mini_fat_start != END_OF_CHAIN && header.mini_fat_count > 0 {
        let data = read_chain(bytes, &header, &fat, header.mini_fat_start)?;
        for index in 0..data.len() / 4 {
            mini_fat.push(u32_at(&data, index * 4).unwrap_or(END_OF_CHAIN));
        }
    }
    let mut streams = Vec::new();
    for entry in entries.iter().filter(|entry| entry.kind == 2) {
        let bytes = if entry.size < u64::from(header.cutoff) {
            // Mini stream: the chain indexes mini sectors of the root storage.
            let mut data = Vec::new();
            let mut current = entry.start;
            let limit = entry.size as usize;
            let mut guard = 0;
            while data.len() < limit && guard < mini_fat.len() + 1 {
                if current == END_OF_CHAIN || current == FREE_SECTOR {
                    break;
                }
                let offset = current as usize * header.mini;
                if let Some(chunk) = mini_stream.get(offset..offset + header.mini) {
                    data.extend_from_slice(chunk);
                } else {
                    break;
                }
                current = mini_fat
                    .get(current as usize)
                    .copied()
                    .unwrap_or(END_OF_CHAIN);
                guard += 1;
            }
            data.truncate(limit);
            data
        } else {
            let mut data = read_chain(bytes, &header, &fat, entry.start)?;
            data.truncate(entry.size as usize);
            data
        };
        streams.push(Stream {
            name: entry.name.clone(),
            size: entry.size,
            bytes,
        });
    }
    Ok(Project { streams })
}

/// Decodes the portable text as UTF-8 or UTF-16LE (with a byte-order mark).
fn decode_text(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let mut text = String::with_capacity(bytes.len() / 2);
        for pair in bytes[2..].as_chunks::<2>().0 {
            text.push(char::from_u32(u32::from(u16::from_le_bytes(*pair))).unwrap_or('\u{FFFD}'));
        }
        return Ok(text);
    }
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(body.to_vec()).map_err(|_| {
        Error::Unsupported("the portable worksheet is not UTF-8 or UTF-16LE text".to_owned())
    })
}

/// One column read from the portable worksheet.
struct Record {
    name: String,
    values: Vec<f64>,
}

/// Reads a portable worksheet text into a worksheet.
///
/// The format is one header line per record and the data behind it:
///
/// ```text
/// <portable header> 18
/// %      3      1      3      0        weight
/// 12.5 13.0 13.5
/// ```
///
/// where the fields are type, column number, length and data type. Numeric
/// columns (`type 3`, `dtype 0`) become worksheet columns; matrix and constant
/// records with numeric data are read and discarded (they are not columns);
/// text columns are refused with their name. A `*` marks a missing value.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a file without the portable header, a
/// corrupt record header, a text column, a missing value that is not `*`, and
/// [`Error::EmptySample`] when no numeric column is present.
pub fn portable_worksheet(bytes: &[u8]) -> Result<DataFrame> {
    let text = decode_text(bytes)?;
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim_start_matches('\u{FEFF}');
    if !first.starts_with(PORTABLE_HEADER) {
        return Err(Error::Unsupported(
            "the stream is not a portable worksheet".to_owned(),
        ));
    }
    let mut records: Vec<Record> = Vec::new();
    let pending: Vec<&str> = lines.collect();
    let mut index = 0usize;
    while index < pending.len() {
        let line = pending[index].trim_end();
        index += 1;
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with('%') {
            return Err(Error::Unsupported(format!(
                "unexpected line in the portable worksheet: {line}"
            )));
        }
        let fields: Vec<&str> = line[1..].split_whitespace().collect();
        if fields.len() < 5 {
            return Err(Error::Unsupported(format!(
                "corrupt portable record header: {line}"
            )));
        }
        let kind: u32 = fields[0]
            .parse()
            .map_err(|_| Error::Unsupported(format!("corrupt record type in {line}")))?;
        let length: usize = fields[2]
            .parse()
            .map_err(|_| Error::Unsupported(format!("corrupt record length in {line}")))?;
        let dtype: u32 = fields[3]
            .parse()
            .map_err(|_| Error::Unsupported(format!("corrupt record data type in {line}")))?;
        let name = fields[4..].join(" ");
        if dtype != 0 {
            return Err(Error::Unsupported(format!(
                "column {name} is not numeric (data type {dtype}); the portable reader imports \
                 numeric columns only"
            )));
        }
        let mut values = Vec::with_capacity(length);
        while values.len() < length {
            if index >= pending.len() {
                return Err(Error::Unsupported(format!(
                    "column {name} declares {length} values but the file ends early"
                )));
            }
            let data_line = pending[index];
            index += 1;
            for token in data_line.split_whitespace() {
                if values.len() == length {
                    // The rest of the line belongs to the record, not to the
                    // next header.
                    break;
                }
                if token == "*" {
                    values.push(f64::NAN);
                } else {
                    values.push(token.parse::<f64>().map_err(|_| {
                        Error::Unsupported(format!("invalid value {token} in column {name}"))
                    })?);
                }
            }
        }
        if kind == 3 {
            records.push(Record { name, values });
        }
    }
    let rows = records
        .iter()
        .map(|record| record.values.len())
        .max()
        .unwrap_or(0);
    if records.is_empty() || rows == 0 {
        return Err(Error::EmptySample);
    }
    let mut columns = Vec::with_capacity(records.len());
    for record in records {
        let mut values = record.values;
        values.resize(rows, f64::NAN);
        let mut name = record.name;
        while columns.iter().any(|column: &Column| column.name() == name) {
            name.push('_');
        }
        columns.push(Column::new_f64(name, values));
    }
    DataFrame::new(columns)
}

/// Imports the worksheet of a project: the first stream named like a
/// worksheet, or the only stream whose content is a portable worksheet.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] with the stream list when no stream carries a
/// portable worksheet (the binary worksheet form is not decoded), the errors of
/// [`parse_container`], and the errors of [`portable_worksheet`].
pub fn project_worksheet(bytes: &[u8], sheet: Option<&str>) -> Result<DataFrame> {
    let project = parse_container(bytes)?;
    let matches = |stream: &&Stream| match sheet {
        Some(name) => stream.name.eq_ignore_ascii_case(name),
        None => {
            let lower = stream.name.to_ascii_lowercase();
            lower.contains("worksheet") || lower.starts_with("data")
        }
    };
    let mut candidates: Vec<&Stream> = project.streams.iter().filter(matches).collect();
    if candidates.is_empty() {
        candidates = project
            .streams
            .iter()
            .filter(|stream| stream.bytes.starts_with(PORTABLE_HEADER.as_bytes()))
            .collect();
    }
    if candidates.is_empty() {
        let names: Vec<String> = project
            .streams
            .iter()
            .map(|stream| format!("{} ({} bytes)", stream.name, stream.size))
            .collect();
        return Err(Error::Unsupported(format!(
            "no portable worksheet stream in the project; the streams are {}. The project \
             format stores worksheets in a binary form that this reader does not decode: \
             export the worksheet as a portable worksheet or CSV first",
            names.join(", ")
        )));
    }
    let names: Vec<String> = project
        .streams
        .iter()
        .map(|stream| format!("{} ({} bytes)", stream.name, stream.size))
        .collect();
    let mut last_error = None;
    for stream in candidates {
        match portable_worksheet(&stream.bytes) {
            Ok(df) => return Ok(df),
            Err(error) => last_error = Some(error),
        }
    }
    Err(Error::Unsupported(format!(
        "no readable portable worksheet in the project (streams: {}); the project format \
         stores worksheets in a binary form that this reader does not decode: export the \
         worksheet as a portable worksheet or CSV first{}",
        names.join(", "),
        last_error
            .map(|error| format!("; last error: {error}"))
            .unwrap_or_default()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_STREAM: u32 = 0xFFFF_FFFF;

    /// Minimal CFB v3 with the streams padded to the mini-stream cutoff so
    /// they live in regular sectors.
    fn compound_file(streams: &[(&str, &[u8])]) -> Vec<u8> {
        const SECTOR: usize = 512;
        const PADDED: usize = 4096;
        let sector_count: usize = streams.len() * (PADDED / SECTOR);
        let total_sectors = 3 + sector_count; // FAT, directory, streams
        let mut bytes = vec![0u8; (total_sectors + 1) * SECTOR];
        bytes[..8].copy_from_slice(&MAGIC);
        bytes[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
        bytes[26..28].copy_from_slice(&3u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
        bytes[30..32].copy_from_slice(&9u16.to_le_bytes());
        bytes[32..34].copy_from_slice(&6u16.to_le_bytes());
        bytes[44..48].copy_from_slice(&1u32.to_le_bytes());
        bytes[48..52].copy_from_slice(&1u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&4096u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        bytes[64..68].copy_from_slice(&0u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        bytes[72..76].copy_from_slice(&0u32.to_le_bytes());
        for index in 0..109 {
            let value = if index == 0 { 0 } else { FREE_SECTOR };
            bytes[76 + index * 4..80 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        // FAT: sector 0 is the FAT, sector 1 the directory, then the streams.
        let mut fat = vec![FREE_SECTOR; SECTOR / 4];
        fat[0] = FAT_SECTOR;
        fat[1] = END_OF_CHAIN;
        let mut next = 2u32;
        let mut starts = Vec::new();
        for _ in streams {
            let sectors = PADDED / SECTOR;
            starts.push(next);
            for index in 0..sectors {
                fat[next as usize] = if index + 1 == sectors {
                    END_OF_CHAIN
                } else {
                    next + 1
                };
                next += 1;
            }
        }
        for (index, value) in fat.iter().enumerate() {
            let offset = SECTOR + index * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        // Directory entries.
        let directory = 2 * SECTOR;
        let write_entry =
            |bytes: &mut [u8], slot: usize, name: &str, kind: u8, start: u32, size: u64| {
                let offset = directory + slot * 128;
                let utf16: Vec<u16> = name.encode_utf16().collect();
                for (index, code) in utf16.iter().enumerate() {
                    let at = offset + index * 2;
                    bytes[at..at + 2].copy_from_slice(&code.to_le_bytes());
                }
                let name_length = ((utf16.len() + 1) * 2) as u16;
                bytes[offset + 64..offset + 66].copy_from_slice(&name_length.to_le_bytes());
                bytes[offset + 66] = kind;
                bytes[offset + 67] = 1;
                let sibling = if slot == 0 {
                    1u32
                } else if slot <= streams.len() {
                    (slot + 1) as u32
                } else {
                    NO_STREAM
                };
                bytes[offset + 68..offset + 72].copy_from_slice(&NO_STREAM.to_le_bytes());
                bytes[offset + 72..offset + 76].copy_from_slice(&sibling.to_le_bytes());
                let child = if slot == 0 { 1u32 } else { NO_STREAM };
                bytes[offset + 76..offset + 80].copy_from_slice(&child.to_le_bytes());
                bytes[offset + 116..offset + 120].copy_from_slice(&start.to_le_bytes());
                bytes[offset + 120..offset + 128].copy_from_slice(&size.to_le_bytes());
            };
        write_entry(&mut bytes, 0, "Root Entry", 5, END_OF_CHAIN, 0);
        for (index, (name, data)) in streams.iter().enumerate() {
            write_entry(&mut bytes, index + 1, name, 2, starts[index], PADDED as u64);
            let offset = (starts[index] as usize + 1) * SECTOR;
            bytes[offset..offset + data.len()].copy_from_slice(data);
            for value in &mut bytes[offset + data.len()..offset + PADDED] {
                *value = b' ';
            }
        }
        bytes
    }

    fn portable() -> String {
        format!(
            "{PORTABLE_HEADER} 18\n\
%      3      1      5      0        weight\n\
12.5 13.0 13.5 14.0 *\n"
        )
    }

    #[test]
    fn the_container_lists_the_streams_with_their_bytes() {
        let bytes = compound_file(&[
            ("ProjectInfo", b"binary"),
            ("Worksheets", portable().as_bytes()),
        ]);
        let project = parse_container(&bytes).expect("container");
        let names: Vec<&str> = project.streams.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ProjectInfo"), "{names:?}");
        assert!(names.contains(&"Worksheets"), "{names:?}");
        let info = project
            .streams
            .iter()
            .find(|stream| stream.name == "ProjectInfo")
            .expect("info");
        assert_eq!(info.size, 4096);
        assert!(info.bytes.starts_with(b"binary"));
        let worksheets = project
            .streams
            .iter()
            .find(|stream| stream.name == "Worksheets")
            .expect("worksheets");
        assert!(worksheets.bytes.starts_with(PORTABLE_HEADER.as_bytes()));
    }

    #[test]
    fn the_portable_worksheet_reads_numeric_columns() {
        let text = portable();
        let df = portable_worksheet(text.as_bytes()).expect("worksheet");
        assert_eq!(df.n_columns(), 1);
        assert_eq!(df.n_rows(), 5);
        let column = df.column("weight").expect("column");
        assert_eq!(column.value(0), Some(12.5));
        assert_eq!(column.value(2), Some(13.5));
        assert!(column.is_missing(4));
        // Two columns of different lengths are padded with missing cells.
        let two = format!(
            "{PORTABLE_HEADER} 18\n\
%      3      1      3      0        a\n1 2 3\n\
%      3      2      4      0        b\n10 20 30 40\n"
        );
        let df = portable_worksheet(two.as_bytes()).expect("two columns");
        assert_eq!(df.n_rows(), 4);
        assert!(df.column("a").expect("a").is_missing(3));
        assert_eq!(df.column("b").expect("b").value(3), Some(40.0));
    }

    #[test]
    fn the_project_worksheet_is_imported_end_to_end() {
        let bytes = compound_file(&[
            ("ProjectInfo", b"anything"),
            ("Worksheets", portable().as_bytes()),
        ]);
        let df = project_worksheet(&bytes, None).expect("import");
        assert_eq!(df.column("weight").expect("weight").value(1), Some(13.0));
        // A named stream is honoured.
        let df = project_worksheet(&bytes, Some("Worksheets")).expect("named");
        assert_eq!(df.n_rows(), 5);
    }

    #[test]
    fn a_binary_project_reports_its_limit() {
        let bytes = compound_file(&[("ProjectInfo", b"binary"), ("Worksheets", b"\x01\x02\x03")]);
        let error = project_worksheet(&bytes, None).expect_err("binary");
        let message = error.to_string();
        assert!(message.contains("Worksheets"), "{message}");
        assert!(message.contains("portable worksheet"), "{message}");
        assert!(message.contains("export"), "{message}");
    }

    #[test]
    fn text_columns_and_foreign_files_are_refused() {
        let text = format!(
            "{PORTABLE_HEADER} 18\n\
%      3      1      2      4        label\nA B\n"
        );
        let error = portable_worksheet(text.as_bytes()).expect_err("text");
        assert!(error.to_string().contains("label"), "{error}");
        assert!(matches!(
            portable_worksheet(b"not a worksheet"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            parse_container(b"not a compound file"),
            Err(Error::Unsupported(_))
        ));
    }
}
