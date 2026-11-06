//! Minimal ZIP reader for the Excel importer.
//!
//! Only what an `.xlsx` archive needs: the central directory, the stored
//! entries and the deflated ones (through [`crate::inflate`]). No dependency,
//! no seek abstraction: the whole archive lives in memory.

use crate::error::{Error, Result};
use crate::inflate::inflate;

/// The signature of the end-of-central-directory record.
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
/// The signature of a central-directory entry.
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// The signature of a local file header.
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;

/// One entry of the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// The name of the entry, with forward slashes.
    pub name: String,
    /// The compression method (0 = stored, 8 = deflated).
    pub method: u16,
    /// The size of the compressed data.
    pub compressed_size: usize,
    /// The size of the uncompressed data.
    pub uncompressed_size: usize,
    /// The offset of the local file header.
    pub offset: usize,
}

/// A ZIP archive read into memory.
#[derive(Debug, Clone)]
pub struct ZipArchive {
    data: Vec<u8>,
    entries: Vec<ZipEntry>,
}

impl ZipArchive {
    /// Reads the central directory of an archive.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the archive is not a ZIP file or when the
    /// central directory is damaged.
    pub fn new(data: Vec<u8>) -> Result<Self> {
        let eocd = find_eocd(&data).ok_or_else(|| Error::Csv("not a ZIP archive".to_owned()))?;
        let count = read_u16(&data, eocd + 10)? as usize;
        let mut offset = read_u32(&data, eocd + 16)? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if read_u32(&data, offset)? != CENTRAL_SIGNATURE {
                return Err(Error::Csv("corrupt ZIP central directory".to_owned()));
            }
            let method = read_u16(&data, offset + 10)?;
            let compressed_size = read_u32(&data, offset + 20)? as usize;
            let uncompressed_size = read_u32(&data, offset + 24)? as usize;
            let name_length = read_u16(&data, offset + 28)? as usize;
            let extra_length = read_u16(&data, offset + 30)? as usize;
            let comment_length = read_u16(&data, offset + 32)? as usize;
            let local_offset = read_u32(&data, offset + 42)? as usize;
            let name_start = offset + 46;
            let name = String::from_utf8_lossy(
                data.get(name_start..name_start + name_length)
                    .ok_or_else(|| Error::Csv("corrupt ZIP entry name".to_owned()))?,
            )
            .to_string();
            entries.push(ZipEntry {
                name,
                method,
                compressed_size,
                uncompressed_size,
                offset: local_offset,
            });
            offset = name_start + name_length + extra_length + comment_length;
        }
        Ok(Self { data, entries })
    }

    /// The entries of the archive.
    #[must_use]
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// The names of the entries.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// Reads an entry by name.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the entry does not exist or cannot be
    /// decompressed.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| Error::Csv(format!("the archive has no entry {name}")))?;
        self.read_entry(entry)
    }

    /// Reads the first entry whose name ends with `suffix`.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when no entry matches or the entry cannot be
    /// decompressed.
    pub fn read_suffix(&self, suffix: &str) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name.ends_with(suffix))
            .ok_or_else(|| Error::Csv(format!("the archive has no entry ending in {suffix}")))?;
        self.read_entry(entry)
    }

    /// Reads and decompresses one entry.
    fn read_entry(&self, entry: &ZipEntry) -> Result<Vec<u8>> {
        let header = entry.offset;
        if read_u32(&self.data, header)? != LOCAL_SIGNATURE {
            return Err(Error::Csv("corrupt ZIP local header".to_owned()));
        }
        let name_length = read_u16(&self.data, header + 26)? as usize;
        let extra_length = read_u16(&self.data, header + 28)? as usize;
        let start = header + 30 + name_length + extra_length;
        let end = start + entry.compressed_size;
        let raw = self
            .data
            .get(start..end)
            .ok_or_else(|| Error::Csv("truncated ZIP entry".to_owned()))?;
        match entry.method {
            0 => Ok(raw.to_vec()),
            8 => inflate(raw),
            other => Err(Error::Csv(format!(
                "unsupported ZIP compression method {other}"
            ))),
        }
    }
}

/// Finds the end-of-central-directory record.
fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(22 + 0xFFFF);
    let mut position = data.len() - 22;
    loop {
        if read_u32(data, position).ok() == Some(EOCD_SIGNATURE) {
            return Some(position);
        }
        if position == start {
            return None;
        }
        position -= 1;
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| Error::Csv("truncated ZIP archive".to_owned()))?;
    Ok(u16::from(bytes[0]) | (u16::from(bytes[1]) << 8))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| Error::Csv("truncated ZIP archive".to_owned()))?;
    Ok(u32::from(bytes[0])
        | (u32::from(bytes[1]) << 8)
        | (u32::from(bytes[2]) << 16)
        | (u32::from(bytes[3]) << 24))
}
