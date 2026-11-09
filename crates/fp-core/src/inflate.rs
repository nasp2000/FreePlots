// SPDX-License-Identifier: MIT
//! Dependency-free DEFLATE (RFC 1951) decoder.
//!
//! The ZIP reader of the engine needs to inflate the entries of an `.xlsx`
//! archive, so the decoder lives here: it reads the bit stream, rebuilds the
//! Huffman tables of every block and copies the literals and the matches into
//! the output. The implementation follows the canonical Huffman decoding of
//! RFC 1951 (the `counts`/`symbols` method).

use crate::error::{Error, Result};

/// The length codes of the literal/length alphabet.
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
/// The extra bits of the length codes.
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// The distance codes.
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
/// The extra bits of the distance codes.
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order of the code-length alphabet.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Reads the bit stream, least significant bit first.
struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u32,
    bits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            buffer: 0,
            bits: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u32> {
        if self.bits == 0 {
            let byte = *self
                .data
                .get(self.position)
                .ok_or_else(|| Error::Csv("truncated deflate stream".to_owned()))?;
            self.position += 1;
            self.buffer = u32::from(byte);
            self.bits = 8;
        }
        let value = self.buffer & 1;
        self.buffer >>= 1;
        self.bits -= 1;
        Ok(value)
    }

    fn read_bits(&mut self, count: u32) -> Result<u32> {
        let mut value = 0u32;
        for index in 0..count {
            value |= self.read_bit()? << index;
        }
        Ok(value)
    }

    fn align(&mut self) {
        self.bits = 0;
        self.buffer = 0;
    }

    fn read_byte(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or_else(|| Error::Csv("truncated deflate stream".to_owned()))?;
        self.position += 1;
        Ok(byte)
    }

    fn read_u16(&mut self) -> Result<u16> {
        let low = u16::from(self.read_byte()?);
        let high = u16::from(self.read_byte()?);
        Ok(low | (high << 8))
    }
}

/// A canonical Huffman table.
struct Huffman {
    /// The number of codes of every length.
    counts: [u16; 16],
    /// The symbols, ordered by code.
    symbols: Vec<u16>,
}

impl Huffman {
    fn build(lengths: &[u8]) -> Result<Self> {
        let mut counts = [0u16; 16];
        for &length in lengths {
            if length as usize >= 16 {
                return Err(Error::Csv("invalid Huffman code length".to_owned()));
            }
            counts[length as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for length in 1..16 {
            offsets[length] = offsets[length - 1] + counts[length - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let offset = &mut offsets[length as usize];
                symbols[*offset as usize] = symbol as u16;
                *offset += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for length in 1..16 {
            code |= reader.read_bit()? as i32;
            let count = i32::from(self.counts[length]);
            if code - first < count {
                let position = (index + (code - first)) as usize;
                return self
                    .symbols
                    .get(position)
                    .copied()
                    .ok_or_else(|| Error::Csv("invalid Huffman code".to_owned()));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::Csv("invalid Huffman code".to_owned()))
    }
}

/// The fixed tables of RFC 1951.
fn fixed_tables() -> (Huffman, Huffman) {
    let mut lengths = vec![8u8; 288];
    for item in lengths.iter_mut().take(256).skip(144) {
        *item = 9;
    }
    for item in lengths.iter_mut().take(280).skip(256) {
        *item = 7;
    }
    let literals = Huffman::build(&lengths).expect("fixed literal table");
    let distances = Huffman::build(&[5u8; 30]).expect("fixed distance table");
    (literals, distances)
}

/// The largest decompressed entry the reader accepts.
///
/// A workbook entry larger than this is not a realistic spreadsheet, and a
/// hostile archive can otherwise expand a few kilobytes into gigabytes.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// Decompresses a raw DEFLATE stream (without the zlib header).
///
/// # Errors
///
/// Returns [`Error`] when the stream is truncated, malformed or expands past
/// [`MAX_OUTPUT_BYTES`].
pub fn inflate(input: &[u8]) -> Result<Vec<u8>> {
    inflate_with_limit(input, MAX_OUTPUT_BYTES)
}

/// Decompresses a raw DEFLATE stream, refusing more than `limit` bytes.
///
/// # Errors
///
/// Returns [`Error`] when the stream is truncated, malformed or exceeds the
/// limit.
pub fn inflate_with_limit(input: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut reader = BitReader::new(input);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let final_block = reader.read_bit()?;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => {
                reader.align();
                let length = reader.read_u16()? as usize;
                let inverse = reader.read_u16()?;
                if !inverse != length as u16 {
                    return Err(Error::Csv("corrupt stored deflate block".to_owned()));
                }
                if out.len().saturating_add(length) > limit {
                    return Err(Error::Csv(format!(
                        "decompressed data exceeds {limit} bytes"
                    )));
                }
                for _ in 0..length {
                    out.push(reader.read_byte()?);
                }
            }
            1 => {
                let (literals, distances) = fixed_tables();
                inflate_block(&mut reader, &literals, &distances, &mut out, limit)?;
            }
            2 => {
                let (literals, distances) = read_dynamic_tables(&mut reader)?;
                inflate_block(&mut reader, &literals, &distances, &mut out, limit)?;
            }
            _ => return Err(Error::Csv("invalid deflate block type".to_owned())),
        }
        if final_block == 1 {
            break;
        }
    }
    Ok(out)
}

/// Reads the Huffman tables of a dynamic block.
fn read_dynamic_tables(reader: &mut BitReader<'_>) -> Result<(Huffman, Huffman)> {
    let literal_count = reader.read_bits(5)? as usize + 257;
    let distance_count = reader.read_bits(5)? as usize + 1;
    let code_count = reader.read_bits(4)? as usize + 4;
    let mut code_lengths = [0u8; 19];
    for &position in CODE_LENGTH_ORDER.iter().take(code_count) {
        code_lengths[position] = reader.read_bits(3)? as u8;
    }
    let code_table = Huffman::build(&code_lengths)?;
    let mut lengths = vec![0u8; literal_count + distance_count];
    let mut index = 0usize;
    while index < lengths.len() {
        let symbol = code_table.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[index] = symbol as u8;
                index += 1;
            }
            16 => {
                if index == 0 {
                    return Err(Error::Csv("invalid deflate code lengths".to_owned()));
                }
                let previous = lengths[index - 1];
                let repeat = 3 + reader.read_bits(2)? as usize;
                for _ in 0..repeat {
                    if index >= lengths.len() {
                        return Err(Error::Csv("invalid deflate code lengths".to_owned()));
                    }
                    lengths[index] = previous;
                    index += 1;
                }
            }
            17 => {
                let repeat = 3 + reader.read_bits(3)? as usize;
                index += repeat;
            }
            18 => {
                let repeat = 11 + reader.read_bits(7)? as usize;
                index += repeat;
            }
            _ => return Err(Error::Csv("invalid deflate code length symbol".to_owned())),
        }
    }
    if index > lengths.len() {
        return Err(Error::Csv("invalid deflate code lengths".to_owned()));
    }
    let literals = Huffman::build(&lengths[..literal_count])?;
    let distances = Huffman::build(&lengths[literal_count..])?;
    Ok((literals, distances))
}

/// Decodes one compressed block into `out`.
fn inflate_block(
    reader: &mut BitReader<'_>,
    literals: &Huffman,
    distances: &Huffman,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<()> {
    loop {
        let symbol = literals.decode(reader)?;
        if symbol < 256 {
            if out.len() >= limit {
                return Err(Error::Csv(format!(
                    "decompressed data exceeds {limit} bytes"
                )));
            }
            out.push(symbol as u8);
            continue;
        }
        if symbol == 256 {
            return Ok(());
        }
        let index = (symbol - 257) as usize;
        let Some(&base) = LENGTH_BASE.get(index) else {
            return Err(Error::Csv("invalid deflate length code".to_owned()));
        };
        let length = base as usize + reader.read_bits(u32::from(LENGTH_EXTRA[index]))? as usize;
        let distance_symbol = distances.decode(reader)? as usize;
        let Some(&distance_base) = DISTANCE_BASE.get(distance_symbol) else {
            return Err(Error::Csv("invalid deflate distance code".to_owned()));
        };
        let distance = distance_base as usize
            + reader.read_bits(u32::from(DISTANCE_EXTRA[distance_symbol]))? as usize;
        if distance == 0 || distance > out.len() {
            return Err(Error::Csv("invalid deflate back reference".to_owned()));
        }
        if out.len().saturating_add(length) > limit {
            return Err(Error::Csv(format!(
                "decompressed data exceeds {limit} bytes"
            )));
        }
        let start = out.len() - distance;
        for offset in 0..length {
            let byte = out[start + offset];
            out.push(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed-Huffman block ("Hello, world!").
    const FIXED_STREAM: &[u8] = &[
        243, 72, 205, 201, 201, 215, 81, 40, 207, 47, 202, 73, 81, 4, 0,
    ];
    /// A dynamic-Huffman block (223 bytes of text).
    const DYNAMIC_STREAM: &[u8] = &[
        93, 143, 209, 10, 128, 48, 8, 69, 127, 197, 95, 83, 146, 22, 184, 26, 228, 147, 95, 223,
        134, 211, 106, 15, 94, 69, 47, 247, 160, 177, 34, 208, 16, 148, 86, 16, 118, 172, 21, 65,
        203, 216, 108, 44, 93, 185, 221, 135, 92, 39, 240, 58, 247, 178, 24, 220, 106, 111, 78, 56,
        61, 207, 146, 178, 100, 206, 78, 121, 119, 178, 43, 37, 130, 130, 227, 113, 127, 68, 220,
        62, 95, 76, 131, 226, 3,
    ];
    /// A stored block (the 256 bytes of a byte ramp).
    const STORED_STREAM: &[u8] = &[
        1, 0, 1, 255, 254, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
        20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
        43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65,
        66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88,
        89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108,
        109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126,
        127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144,
        145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162,
        163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180,
        181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198,
        199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216,
        217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234,
        235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252,
        253, 254, 255,
    ];
    /// An empty payload.
    const EMPTY_STREAM: &[u8] = &[3, 0];

    #[test]
    fn fixed_blocks_are_decoded() {
        let out = inflate(FIXED_STREAM).expect("inflate");
        assert_eq!(out, b"Hello, world!");
    }

    #[test]
    fn dynamic_blocks_are_decoded() {
        let out = inflate(DYNAMIC_STREAM).expect("inflate");
        let text = "zeta beta alpha gamma theta delta epsilon eta epsilon eta eta zeta eta delta \
                    zeta alpha epsilon gamma zeta beta delta epsilon epsilon beta beta theta theta \
                    beta zeta beta eta gamma alpha epsilon eta eta beta alpha alpha eta";
        assert_eq!(out.len(), text.len());
        assert_eq!(out, text.as_bytes());
    }

    #[test]
    fn stored_blocks_are_decoded() {
        let out = inflate(STORED_STREAM).expect("inflate");
        assert_eq!(out.len(), 256);
        for (index, byte) in out.iter().enumerate() {
            assert_eq!(*byte, index as u8);
        }
    }

    #[test]
    fn empty_payloads_are_decoded() {
        let out = inflate(EMPTY_STREAM).expect("inflate");
        assert!(out.is_empty());
    }

    #[test]
    fn truncated_streams_are_rejected() {
        assert!(inflate(&FIXED_STREAM[..4]).is_err());
        assert!(inflate(&[]).is_err());
    }

    #[test]
    fn the_output_limit_is_enforced() {
        assert!(inflate_with_limit(FIXED_STREAM, 5).is_err());
        assert_eq!(
            inflate_with_limit(FIXED_STREAM, 13).expect("exact limit"),
            b"Hello, world!"
        );
        assert!(inflate_with_limit(STORED_STREAM, 100).is_err());
        assert_eq!(
            inflate_with_limit(STORED_STREAM, 256)
                .expect("exact limit")
                .len(),
            256
        );
    }
}
