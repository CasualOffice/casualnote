//! A small ZIP reader and writer, only as much as docs/36 needs. The reader treats every field as
//! hostile: nothing is indexed or added without a check, and sizes are measured, never trusted.

use crate::{PackageError, refuse};
use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZFlush, MZStatus};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const END64: u32 = 0x0606_4b50;
const LOCATOR64: u32 = 0x0706_4b50;
/// 1980-01-01, the fixed timestamp.
const DOS_DATE: u16 = 33;
const UTF8_FLAG: u16 = 0x0800;
const ZIP64_ABOVE_ENTRIES: usize = 65_534;
const NEEDS_64: u64 = 0xFFFF_FFFF;
const CHUNK: usize = 1 << 16;

/// Entries past this are refused while reading.
pub const MAX_ENTRIES: usize = 1_000_000;
const MAX_RATIO: u64 = 200;
const RATIO_APPLIES_ABOVE: u64 = 1 << 20;

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

fn crc_update(mut crc: u32, data: &[u8]) -> u32 {
    for byte in data {
        let slot = ((crc ^ u32::from(*byte)) & 0xFF) as usize;
        crc = CRC_TABLE.get(slot).copied().unwrap_or(0) ^ (crc >> 8);
    }
    crc
}

/// The CRC-32 of some bytes.
pub fn crc32(data: &[u8]) -> u32 {
    !crc_update(!0, data)
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// One central directory record. An offset past 4 GiB goes in a ZIP64 extra field.
pub(crate) fn central_record(out: &mut Vec<u8>, name: &[u8], crc: u32, size: u32, offset: u64) {
    let wide = offset >= NEEDS_64;
    put32(out, CENTRAL);
    put16(out, if wide { 45 } else { 20 });
    put16(out, if wide { 45 } else { 20 });
    put16(out, UTF8_FLAG);
    put16(out, 0);
    put16(out, 0);
    put16(out, DOS_DATE);
    put32(out, crc);
    put32(out, size);
    put32(out, size);
    put16(out, name.len() as u16);
    put16(out, if wide { 12 } else { 0 });
    put16(out, 0);
    put16(out, 0);
    put16(out, 0);
    put32(out, 0);
    put32(out, if wide { u32::MAX } else { offset as u32 });
    out.extend_from_slice(name);
    if wide {
        put16(out, 1);
        put16(out, 8);
        put64(out, offset);
    }
}

/// Writes stored entries in the order given.
pub fn write(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, PackageError> {
    let mut size = 22usize;
    for (name, data) in entries {
        size = size.saturating_add(76 + 2 * name.len() + data.len());
    }
    let mut out = Vec::with_capacity(size);
    let mut central = Vec::with_capacity(entries.len().saturating_mul(64));
    for (name, data) in entries {
        let name = name.as_bytes();
        if name.len() > usize::from(u16::MAX) {
            return Err(refuse("CN-PKG-0004", "a path is too long for a ZIP file"));
        }
        let length = u32::try_from(data.len())
            .map_err(|_| refuse("CN-PKG-0007", "an entry is over the size limit"))?;
        let crc = crc32(data);
        let offset = out.len() as u64;
        put32(&mut out, LOCAL);
        put16(&mut out, 20);
        put16(&mut out, UTF8_FLAG);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, length);
        put32(&mut out, length);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        central_record(&mut central, name, crc, length, offset);
    }
    let start = out.len() as u64;
    let central_size = central.len() as u64;
    out.extend_from_slice(&central);
    let count = entries.len();
    let wide = count > ZIP64_ABOVE_ENTRIES || start >= NEEDS_64 || central_size >= NEEDS_64;
    if wide {
        let at = out.len() as u64;
        put32(&mut out, END64);
        put64(&mut out, 44);
        put16(&mut out, 45);
        put16(&mut out, 45);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put64(&mut out, count as u64);
        put64(&mut out, count as u64);
        put64(&mut out, central_size);
        put64(&mut out, start);
        put32(&mut out, LOCATOR64);
        put32(&mut out, 0);
        put64(&mut out, at);
        put32(&mut out, 1);
    }
    let short = if wide { u16::MAX } else { count as u16 };
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, short);
    put16(&mut out, short);
    put32(&mut out, u32::try_from(central_size).unwrap_or(u32::MAX));
    put32(&mut out, u32::try_from(start).unwrap_or(u32::MAX));
    put16(&mut out, 0);
    Ok(out)
}

/// One entry as the central directory describes it, which may be lying.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub name_is_utf8: bool,
    pub flags: u16,
    pub method: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    pub offset: u64,
    pub attributes: u32,
}

impl Entry {
    /// Whether this is a directory record.
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}
fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn damaged() -> PackageError {
    refuse("CN-PKG-0001", "the file is damaged or is not a ZIP")
}

fn find_end(data: &[u8]) -> Option<usize> {
    let last = data.len().checked_sub(22)?;
    let first = last.saturating_sub(usize::from(u16::MAX));
    (first..=last).rev().find(|at| {
        u32_at(data, *at) == Some(END)
            && u16_at(data, at + 20)
                .is_some_and(|comment| at + 22 + usize::from(comment) <= data.len())
    })
}

/// Reads the central directory. Stops one past the entry limit, so the caller can refuse by name.
pub fn central_directory(data: &[u8]) -> Result<Vec<Entry>, PackageError> {
    let end = find_end(data).ok_or_else(damaged)?;
    let mut count = u64::from(u16_at(data, end + 10).ok_or_else(damaged)?);
    let mut dir_size = u64::from(u32_at(data, end + 12).ok_or_else(damaged)?);
    let mut dir_at = u64::from(u32_at(data, end + 16).ok_or_else(damaged)?);
    if let Some(locator) = end.checked_sub(20)
        && u32_at(data, locator) == Some(LOCATOR64)
    {
        let at = u64_at(data, locator + 8).ok_or_else(damaged)?;
        let at = usize::try_from(at).map_err(|_| damaged())?;
        if u32_at(data, at) != Some(END64) {
            return Err(damaged());
        }
        count = u64_at(data, at.checked_add(32).ok_or_else(damaged)?).ok_or_else(damaged)?;
        dir_size = u64_at(data, at + 40).ok_or_else(damaged)?;
        dir_at = u64_at(data, at + 48).ok_or_else(damaged)?;
    }
    let from = usize::try_from(dir_at).map_err(|_| damaged())?;
    let length = usize::try_from(dir_size).map_err(|_| damaged())?;
    let dir = data
        .get(from..from.checked_add(length).ok_or_else(damaged)?)
        .ok_or_else(damaged)?;
    if count.checked_mul(46).is_none_or(|least| least > dir_size) {
        return Err(damaged());
    }
    let mut entries = Vec::new();
    let mut at = 0usize;
    while at < dir.len() && entries.len() <= MAX_ENTRIES {
        if u32_at(dir, at) != Some(CENTRAL) {
            return Err(damaged());
        }
        let field16 = |o: usize| u16_at(dir, at + o).ok_or_else(damaged);
        let field32 = |o: usize| u32_at(dir, at + o).ok_or_else(damaged);
        let flags = field16(8)?;
        let method = field16(10)?;
        let crc = field32(16)?;
        let mut compressed = u64::from(field32(20)?);
        let mut size = u64::from(field32(24)?);
        let name_length = usize::from(field16(28)?);
        let extra_length = usize::from(field16(30)?);
        let comment_length = usize::from(field16(32)?);
        let attributes = field32(38)?;
        let mut offset = u64::from(field32(42)?);
        let name_at = at + 46;
        let raw = dir
            .get(name_at..name_at + name_length)
            .ok_or_else(damaged)?;
        let extra = dir
            .get(name_at + name_length..name_at + name_length + extra_length)
            .ok_or_else(damaged)?;
        let wide = |e: &mut usize| -> Result<u64, PackageError> {
            let v = u64_at(extra, *e).ok_or_else(damaged)?;
            *e += 8;
            Ok(v)
        };
        let mut e = 0usize;
        while e + 4 <= extra.len() {
            let tag = u16_at(extra, e).ok_or_else(damaged)?;
            let len = usize::from(u16_at(extra, e + 2).ok_or_else(damaged)?);
            if tag == 1 {
                let mut cursor = e + 4;
                if size == NEEDS_64 {
                    size = wide(&mut cursor)?;
                }
                if compressed == NEEDS_64 {
                    compressed = wide(&mut cursor)?;
                }
                if offset == NEEDS_64 {
                    offset = wide(&mut cursor)?;
                }
            }
            e += 4 + len;
        }
        let (name, name_is_utf8) = match std::str::from_utf8(raw) {
            Ok(s) => (s.to_string(), true),
            Err(_) => (String::from_utf8_lossy(raw).into_owned(), false),
        };
        entries.push(Entry {
            name,
            name_is_utf8,
            flags,
            method,
            crc,
            compressed,
            size,
            offset,
            attributes,
        });
        at = name_at + name_length + extra_length + comment_length;
    }
    if entries.len() <= MAX_ENTRIES && u64::try_from(entries.len()).ok() != Some(count) {
        return Err(damaged());
    }
    Ok(entries)
}

/// Finds an entry's raw bytes through its local header, which must agree on the name.
fn raw_bytes<'a>(data: &'a [u8], entry: &Entry) -> Result<&'a [u8], PackageError> {
    let at = usize::try_from(entry.offset).map_err(|_| damaged())?;
    if u32_at(data, at) != Some(LOCAL) {
        return Err(damaged());
    }
    let name_length = usize::from(u16_at(data, at + 26).ok_or_else(damaged)?);
    let extra_length = usize::from(u16_at(data, at + 28).ok_or_else(damaged)?);
    let name = data
        .get(at + 30..at + 30 + name_length)
        .ok_or_else(damaged)?;
    if name != entry.name.as_bytes() {
        return Err(damaged());
    }
    let start = at + 30 + name_length + extra_length;
    let length = usize::try_from(entry.compressed).map_err(|_| damaged())?;
    data.get(start..start.checked_add(length).ok_or_else(damaged)?)
        .ok_or_else(damaged)
}

fn over_ratio(size: u64, compressed: u64) -> bool {
    size > RATIO_APPLIES_ABOVE && size > compressed.max(1).saturating_mul(MAX_RATIO)
}

/// Passes an entry through `sink` a piece at a time, holding one piece in memory. Sizes are
/// measured as the bytes appear, and the CRC is checked. Returns the bytes produced.
pub fn stream(
    data: &[u8],
    entry: &Entry,
    limit: u64,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), PackageError>,
) -> Result<u64, PackageError> {
    let raw = raw_bytes(data, entry)?;
    let too_big = || refuse("CN-PKG-0007", "an entry is over the size limit");
    let mut crc = !0u32;
    let mut produced = 0u64;
    match entry.method {
        0 => {
            if entry.compressed != entry.size {
                return Err(damaged());
            }
            if entry.size > limit {
                return Err(too_big());
            }
            for piece in raw.chunks(1 << 20) {
                crc = crc_update(crc, piece);
                sink(piece)?;
            }
            produced = entry.size;
        }
        8 => {
            let mut state = InflateState::new_boxed(DataFormat::Raw);
            let mut buffer = vec![0u8; CHUNK];
            let mut input = raw;
            loop {
                let result = inflate(&mut state, input, &mut buffer, MZFlush::None);
                input = input.get(result.bytes_consumed..).unwrap_or(&[]);
                let piece = buffer.get(..result.bytes_written).unwrap_or(&[]);
                produced = produced.saturating_add(piece.len() as u64);
                if produced > limit {
                    return Err(too_big());
                }
                if produced > entry.size {
                    return Err(refuse(
                        "CN-PKG-0007",
                        "an entry is larger than its header says",
                    ));
                }
                if over_ratio(produced, entry.compressed) {
                    return Err(refuse("CN-PKG-0007", "an entry expands more than 200:1"));
                }
                crc = crc_update(crc, piece);
                sink(piece)?;
                match result.status {
                    Ok(MZStatus::StreamEnd) => break,
                    Ok(_) if result.bytes_consumed > 0 || result.bytes_written > 0 => {}
                    _ => return Err(damaged()),
                }
            }
            if produced != entry.size {
                return Err(damaged());
            }
        }
        _ => return Err(damaged()),
    }
    if !crc != entry.crc {
        return Err(damaged());
    }
    Ok(produced)
}

/// Reads a whole entry, up to `limit` bytes.
pub fn read_all(data: &[u8], entry: &Entry, limit: u64) -> Result<Vec<u8>, PackageError> {
    let mut out = Vec::new();
    stream(data, entry, limit, &mut |piece| {
        out.extend_from_slice(piece);
        Ok(())
    })?;
    Ok(out)
}
