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

/// Decompresses a raw DEFLATE stream (without the zlib header).
///
/// # Errors
///
/// Returns [`Error`] when the stream is truncated or malformed.
pub fn inflate(input: &[u8]) -> Result<Vec<u8>> {
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
                for _ in 0..length {
                    out.push(reader.read_byte()?);
                }
            }
            1 => {
                let (literals, distances) = fixed_tables();
                inflate_block(&mut reader, &literals, &distances, &mut out)?;
            }
            2 => {
                let (literals, distances) = read_dynamic_tables(&mut reader)?;
                inflate_block(&mut reader, &literals, &distances, &mut out)?;
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
) -> Result<()> {
    loop {
        let symbol = literals.decode(reader)?;
        if symbol < 256 {
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
        let start = out.len() - distance;
        for offset in 0..length {
            let byte = out[start + offset];
            out.push(byte);
        }
    }
}
