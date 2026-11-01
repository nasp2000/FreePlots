//! Live automation bridge for the desktop window.
//!
//! `FreePlots.exe --bridge 127.0.0.1:8767` listens for one JSON command per
//! line, injects the pointer or keyboard event into the running window and
//! answers with JSON. Screenshots are written as PNG files.
//!
//! The bridge is deliberately small: it is a debugging aid for driving the
//! visible application, not a second UI.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// One queued command and the channel that answers it.
#[derive(Debug)]
pub struct Command {
    /// JSON request (`{"cmd": ..., ...}`).
    pub request: Value,
    /// Reply channel; the UI thread answers after applying the command.
    pub reply: Sender<Value>,
}

/// Command queue shared with the listener thread.
#[derive(Debug, Default)]
pub struct Bridge {
    queue: Mutex<Vec<Command>>,
    token: Option<String>,
}

impl Bridge {
    /// Starts the listener on `address` (`host:port`).
    ///
    /// # Errors
    ///
    /// Returns the bind error when the address is not available.
    pub fn start(address: &str, token: Option<String>) -> Result<Arc<Self>, String> {
        let public = !address.starts_with("127.0.0.1") && !address.starts_with("localhost");
        if public && token.as_deref().unwrap_or("").is_empty() {
            return Err(format!(
                "refusing to listen on {address} without --token <secret>"
            ));
        }
        let listener = TcpListener::bind(address)
            .map_err(|error| format!("cannot bind {address}: {error}"))?;
        let bridge = Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            token,
        });
        let shared = Arc::clone(&bridge);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let bridge = Arc::clone(&shared);
                std::thread::spawn(move || {
                    let Ok(mut writer) = stream.try_clone() else {
                        return;
                    };
                    let reader = BufReader::new(stream);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }
                        let request: Value = match serde_json::from_str(&line) {
                            Ok(value) => value,
                            Err(error) => {
                                let _ = writeln!(
                                    writer,
                                    "{}",
                                    json!({"ok": false, "error": error.to_string()})
                                );
                                continue;
                            }
                        };
                        let authorised = match bridge.token.as_deref() {
                            None => true,
                            Some(expected) => request
                                .get("token")
                                .and_then(Value::as_str)
                                .is_some_and(|given| constant_time_eq(given, expected)),
                        };
                        if !authorised {
                            let _ = writeln!(
                                writer,
                                "{}",
                                json!({"ok": false, "error": "unauthorized"})
                            );
                            continue;
                        }
                        let (reply, receiver) = channel();
                        if let Ok(mut queue) = bridge.queue.lock() {
                            queue.push(Command { request, reply });
                        }
                        let answer = receiver
                            .recv_timeout(std::time::Duration::from_secs(30))
                            .unwrap_or_else(|_| json!({"ok": false, "error": "timeout"}));
                        let _ = writeln!(writer, "{answer}");
                    }
                });
            }
        });
        Ok(bridge)
    }

    /// Takes the commands queued since the last frame.
    pub fn drain(&self) -> Vec<Command> {
        self.queue
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

/// Encodes RGBA pixels as a PNG with stored (uncompressed) deflate blocks.
///
/// Dependency-free on purpose: the bridge must not pull an image stack into
/// the desktop build.
#[must_use]
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in 0..height as usize {
        raw.push(0);
        let start = row * width as usize * 4;
        raw.extend_from_slice(&rgba[start..start + width as usize * 4]);
    }

    let mut zlib = vec![0x78, 0x01];
    let mut offset = 0;
    while offset < raw.len() {
        let len = (raw.len() - offset).min(65_535);
        let last = offset + len == raw.len();
        zlib.push(u8::from(last));
        zlib.extend_from_slice(&(len as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(len as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + len]);
        offset += len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// Compares two secrets without an early exit.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_token_comparison_is_exact() {
        assert!(constant_time_eq("s3cret", "s3cret"));
        assert!(!constant_time_eq("s3cre", "s3cret"));
        assert!(!constant_time_eq("s3cret", "s3creT"));
        assert!(!constant_time_eq("", "s3cret"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn the_png_starts_with_the_signature_and_ends_with_iend() {
        let png = encode_png(2, 2, &[255; 16]);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(png.windows(4).any(|window| window == b"IEND"));
        assert!(png.len() > 40);
    }
}
