//! The bytes on disk: segment files of checksummed frames, and snapshot files (docs/23). Every
//! integer is little-endian. Everything read here is treated as hostile: each length is checked
//! against what is present before it is used.

use sha2::{Digest, Sha256};

pub const SEGMENT_MAGIC: [u8; 8] = [0x43, 0x4E, 0x53, 0x45, 0x47, 0x0D, 0x0A, 0x1A];
pub const SNAPSHOT_MAGIC: [u8; 8] = [0x43, 0x4E, 0x53, 0x4E, 0x50, 0x0D, 0x0A, 0x1A];
pub const FRAME_MAGIC: [u8; 4] = *b"CNFR";
pub const HEADER_LEN: usize = 128;
pub const FRAME_HEADER_LEN: usize = 32;
pub const SEAL_LEN: usize = 48;
pub const SNAPSHOT_HEADER_LEN: usize = 144;
pub const MAX_PAYLOAD: usize = 64 * 1024 * 1024;
pub const FORMAT_MAJOR: u16 = 1;
pub const FRAME_BATCH: u8 = 1;
pub const FRAME_PEER: u8 = 2;
pub const FRAME_SEAL: u8 = 3;
pub const CRDT_CODEC: u8 = 1;

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0x82F6_3B78 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}
static CRC_TABLE: [u32; 256] = crc_table();

/// CRC-32C, the Castagnoli polynomial.
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c = CRC_TABLE[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(b"0123456789abcdef"[usize::from(b >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(b & 15)]));
    }
    out
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    s.as_bytes()
        .chunks(2)
        .map(|p| Some(digit(p[0])? << 4 | digit(p[1])?))
        .collect()
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn le64(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

/// Which kind of document a name means.
pub fn doc_class(name: &str) -> u8 {
    if name.is_empty() {
        1
    } else if name.starts_with("d:") {
        3
    } else {
        2
    }
}

fn doc_id(name: &str) -> &str {
    name.strip_prefix("d:").unwrap_or(name)
}

/// SHA-256 of class byte and id. A document id is never a path.
pub fn doc_key(name: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([doc_class(name)]);
    h.update(doc_id(name).as_bytes());
    h.finalize().into()
}

/// The document's directory.
pub fn doc_dir(name: &str) -> String {
    let key = hex(&doc_key(name)[..16]);
    match doc_class(name) {
        1 => "tree".to_owned(),
        3 => format!("databases/{}/{key}", &key[..2]),
        _ => format!("pages/{}/{key}", &key[..2]),
    }
}

pub fn segment_name(device: &[u8; 16], sequence: u64) -> String {
    format!("{}-{sequence:016x}.seg", hex(device))
}

/// Reads a segment file's name exactly, so a hostile or mangled name is never used.
pub fn parse_segment_name(name: &str) -> Option<([u8; 16], u64)> {
    let stem = name.strip_suffix(".seg")?;
    let (d, s) = stem.split_once('-')?;
    if d.len() != 32 || s.len() != 16 {
        return None;
    }
    let device: [u8; 16] = unhex(d)?.try_into().ok()?;
    let seq: [u8; 8] = unhex(s)?.try_into().ok()?;
    Some((device, u64::from_be_bytes(seq)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentHeader {
    pub doc_class: u8,
    pub device: [u8; 16],
    pub sequence: u64,
    pub created_ms: i64,
    pub doc_key: [u8; 32],
    pub prev_sha: [u8; 32],
}

pub fn encode_segment_header(h: &SegmentHeader) -> [u8; HEADER_LEN] {
    let mut b = [0u8; HEADER_LEN];
    b[..8].copy_from_slice(&SEGMENT_MAGIC);
    b[8..10].copy_from_slice(&FORMAT_MAJOR.to_le_bytes());
    b[12..14].copy_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    b[14] = h.doc_class;
    b[15] = CRDT_CODEC;
    b[16..18].copy_from_slice(&1u16.to_le_bytes());
    b[24..40].copy_from_slice(&h.device);
    b[40..48].copy_from_slice(&h.sequence.to_le_bytes());
    b[48..56].copy_from_slice(&h.created_ms.to_le_bytes());
    b[56..88].copy_from_slice(&h.doc_key);
    b[88..120].copy_from_slice(&h.prev_sha);
    let crc = crc32c(&b[..124]);
    b[124..128].copy_from_slice(&crc.to_le_bytes());
    b
}

pub fn parse_segment_header(b: &[u8]) -> Result<SegmentHeader, &'static str> {
    if b.len() < HEADER_LEN {
        return Err("short");
    }
    if b[..8] != SEGMENT_MAGIC {
        return Err("magic");
    }
    if crc32c(&b[..124]) != le32(b, 124) {
        return Err("crc");
    }
    if le16(b, 8) > FORMAT_MAJOR {
        return Err("newer");
    }
    if usize::from(le16(b, 12)) != HEADER_LEN || b[20..24] != [0; 4] || b[120..124] != [0; 4] {
        return Err("reserved");
    }
    if b[15] != CRDT_CODEC {
        return Err("codec");
    }
    let mut device = [0; 16];
    device.copy_from_slice(&b[24..40]);
    let mut doc_key = [0; 32];
    doc_key.copy_from_slice(&b[56..88]);
    let mut prev_sha = [0; 32];
    prev_sha.copy_from_slice(&b[88..120]);
    Ok(SegmentHeader {
        doc_class: b[14],
        device,
        sequence: le64(b, 40),
        created_ms: le64(b, 48) as i64,
        doc_key,
        prev_sha,
    })
}

/// One frame's header and payload.
pub fn encode_frame(frame_type: u8, index: u64, prev_crc: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(&FRAME_MAGIC);
    out.push(frame_type);
    out.extend_from_slice(&[0, 0, 0]);
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32c(payload).to_le_bytes());
    out.extend_from_slice(&prev_crc.to_le_bytes());
    let crc = crc32c(&out[..28]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn peer_payload(peer: u64, reason: u8) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[..8].copy_from_slice(&peer.to_le_bytes());
    p[8] = reason;
    p
}

pub fn batch_payload(batch_id: [u8; 16], origin: u8, wall_ms: i64, update: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(56 + update.len());
    p.extend_from_slice(&1u16.to_le_bytes());
    p.push(origin);
    p.push(0);
    p.extend_from_slice(&batch_id);
    p.extend_from_slice(&wall_ms.to_le_bytes());
    p.extend_from_slice(&[0u8; 16]);
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&(update.len() as u32).to_le_bytes());
    p.extend_from_slice(update);
    p
}

/// The CRDT update inside a Batch payload, which must fill it exactly.
pub fn batch_update(p: &[u8]) -> Option<&[u8]> {
    if p.len() < 56 || p[3] != 0 {
        return None;
    }
    let summary = usize::try_from(le32(p, 48)).ok()?;
    let update = usize::try_from(le32(p, 52)).ok()?;
    if summary > 64 * 1024 || 56usize.checked_add(summary)?.checked_add(update)? != p.len() {
        return None;
    }
    Some(&p[56 + summary..])
}

pub fn seal_payload(frames: u64, bytes_before: u64, sha: &[u8; 32]) -> [u8; SEAL_LEN] {
    let mut p = [0u8; SEAL_LEN];
    p[..8].copy_from_slice(&frames.to_le_bytes());
    p[8..16].copy_from_slice(&bytes_before.to_le_bytes());
    p[16..48].copy_from_slice(sha);
    p
}

/// One frame found by a scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScannedFrame {
    pub frame_type: u8,
    pub offset: usize,
    pub payload: (usize, usize),
    pub crc: u32,
}

/// What a segment file holds, up to its last good frame.
#[derive(Clone, Debug)]
pub struct Scan {
    pub header: Result<SegmentHeader, &'static str>,
    pub frames: Vec<ScannedFrame>,
    /// The end of the last good frame. For a bad header with no frames, 0.
    pub good_end: usize,
    /// A valid frame exists after `good_end`: damage in the middle, not a torn tail.
    pub damage: bool,
    /// A seal frame that checks against the bytes before it.
    pub sealed: bool,
    /// A seal frame whose hash does not match.
    pub seal_mismatch: bool,
    pub sha: Option<[u8; 32]>,
}

fn frame_at(b: &[u8], at: usize, want: Option<(u64, u32)>) -> Option<ScannedFrame> {
    let end_head = at.checked_add(FRAME_HEADER_LEN)?;
    let head = b.get(at..end_head)?;
    if head[..4] != FRAME_MAGIC || head[5] != 0 || head[6..8] != [0, 0] {
        return None;
    }
    if crc32c(&head[..28]) != le32(head, 28) {
        return None;
    }
    let len = usize::try_from(le32(head, 16)).ok()?;
    if len > MAX_PAYLOAD {
        return None;
    }
    let end = end_head.checked_add(len)?;
    let payload = b.get(end_head..end)?;
    let crc = crc32c(payload);
    if crc != le32(head, 20) {
        return None;
    }
    if let Some((index, prev)) = want
        && (le64(head, 8) != index || le32(head, 24) != prev)
    {
        return None;
    }
    Some(ScannedFrame {
        frame_type: head[4],
        offset: at,
        payload: (end_head, end),
        crc,
    })
}

/// Reads frame by frame to the last good one, checking every checksum.
pub fn scan_segment(b: &[u8]) -> Scan {
    let header = parse_segment_header(b);
    let mut frames = Vec::new();
    let mut at = HEADER_LEN;
    let mut prev = 0u32;
    let mut sealed = false;
    let mut seal_mismatch = false;
    let mut sha = None;
    if b.len() > HEADER_LEN {
        while let Some(f) = frame_at(b, at, Some((frames.len() as u64, prev))) {
            at = f.payload.1;
            prev = f.crc;
            frames.push(f);
            if f.frame_type == FRAME_SEAL {
                let p = &b[f.payload.0..f.payload.1];
                let good = p.len() == SEAL_LEN
                    && le64(p, 0) == frames.len() as u64 - 1
                    && le64(p, 8) == f.offset as u64
                    && Sha256::digest(&b[..f.offset]).as_slice() == &p[16..48];
                if good {
                    sealed = true;
                    let mut s = [0; 32];
                    s.copy_from_slice(&Sha256::digest(&b[..at]));
                    sha = Some(s);
                } else {
                    seal_mismatch = true;
                }
                break;
            }
        }
    }
    let good_end = if header.is_ok() || !frames.is_empty() {
        at.min(b.len())
    } else {
        0
    };
    let mut damage = false;
    if at < b.len() && (header.is_ok() || !frames.is_empty()) {
        if sealed {
            damage = true; // Nothing may follow a seal.
        } else {
            let mut i = at + 1;
            while i + FRAME_HEADER_LEN <= b.len() {
                if b[i..i + 4] == FRAME_MAGIC && frame_at(b, i, None).is_some() {
                    damage = true;
                    break;
                }
                i += 1;
            }
        }
    }
    Scan {
        header,
        frames,
        good_end,
        damage,
        sealed,
        seal_mismatch,
        sha,
    }
}

/// A snapshot file's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub doc_class: u8,
    pub kind: u8,
    pub doc_key: [u8; 32],
    pub created_ms: i64,
    pub device: [u8; 16],
    pub seq: u64,
    pub state_hash: [u8; 32],
    pub covers: Vec<([u8; 16], u64, u64)>,
    pub version: Vec<u8>,
    pub payload: Vec<u8>,
}

pub const SNAPSHOT_CACHE: u8 = 1;
pub const SNAPSHOT_BASE: u8 = 2;

pub fn encode_snapshot(s: &Snapshot) -> Vec<u8> {
    let covers_len = s.covers.len() * 32;
    let mut b = Vec::with_capacity(
        SNAPSHOT_HEADER_LEN + covers_len + s.version.len() + s.payload.len() + 40,
    );
    b.extend_from_slice(&SNAPSHOT_MAGIC);
    b.extend_from_slice(&FORMAT_MAJOR.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&(SNAPSHOT_HEADER_LEN as u16).to_le_bytes());
    b.push(s.doc_class);
    b.push(CRDT_CODEC);
    b.extend_from_slice(&1u16.to_le_bytes());
    b.push(s.kind);
    b.extend_from_slice(&[0u8; 5]);
    b.extend_from_slice(&s.doc_key);
    b.extend_from_slice(&s.created_ms.to_le_bytes());
    b.extend_from_slice(&s.device);
    b.extend_from_slice(&s.seq.to_le_bytes());
    b.extend_from_slice(&s.state_hash);
    b.extend_from_slice(&(covers_len as u32).to_le_bytes());
    b.extend_from_slice(&(s.version.len() as u32).to_le_bytes());
    b.extend_from_slice(&(s.payload.len() as u64).to_le_bytes());
    b.extend_from_slice(&[0u8; 4]);
    let crc = crc32c(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    for (d, q, f) in &s.covers {
        b.extend_from_slice(d);
        b.extend_from_slice(&q.to_le_bytes());
        b.extend_from_slice(&f.to_le_bytes());
    }
    b.extend_from_slice(&s.version);
    b.extend_from_slice(&s.payload);
    let digest = Sha256::digest(&b);
    let foot = b.len();
    b.extend_from_slice(&digest);
    b.extend_from_slice(b"CNEN");
    let crc = crc32c(&b[foot..]);
    b.extend_from_slice(&crc.to_le_bytes());
    b
}

/// A snapshot counts only if its footer and hash check. A torn write never does.
pub fn parse_snapshot(b: &[u8]) -> Option<Snapshot> {
    if b.len() < SNAPSHOT_HEADER_LEN + 40 || b[..8] != SNAPSHOT_MAGIC {
        return None;
    }
    if crc32c(&b[..140]) != le32(b, 140) || le16(b, 8) > FORMAT_MAJOR {
        return None;
    }
    let covers_len = usize::try_from(le32(b, 120)).ok()?;
    let version_len = usize::try_from(le32(b, 124)).ok()?;
    let payload_len = usize::try_from(le64(b, 128)).ok()?;
    if covers_len % 32 != 0 || covers_len > 16 * 1024 * 1024 {
        return None;
    }
    let total = SNAPSHOT_HEADER_LEN
        .checked_add(covers_len)?
        .checked_add(version_len)?
        .checked_add(payload_len)?;
    if total.checked_add(40)? != b.len() {
        return None;
    }
    let foot = &b[total..];
    if &foot[32..36] != b"CNEN" || crc32c(&foot[..36]) != le32(foot, 36) {
        return None;
    }
    if Sha256::digest(&b[..total]).as_slice() != &foot[..32] {
        return None;
    }
    let mut covers = Vec::with_capacity(covers_len / 32);
    for c in b[SNAPSHOT_HEADER_LEN..SNAPSHOT_HEADER_LEN + covers_len].chunks(32) {
        let mut d = [0; 16];
        d.copy_from_slice(&c[..16]);
        covers.push((d, le64(c, 16), le64(c, 24)));
    }
    let mut doc_key = [0; 32];
    doc_key.copy_from_slice(&b[24..56]);
    let mut device = [0; 16];
    device.copy_from_slice(&b[64..80]);
    let mut state_hash = [0; 32];
    state_hash.copy_from_slice(&b[88..120]);
    let vstart = SNAPSHOT_HEADER_LEN + covers_len;
    Some(Snapshot {
        doc_class: b[14],
        kind: b[18],
        doc_key,
        created_ms: le64(b, 56) as i64,
        device,
        seq: le64(b, 80),
        state_hash,
        covers,
        version: b[vstart..vstart + version_len].to_vec(),
        payload: b[vstart + version_len..total].to_vec(),
    })
}

/// A small local file: payload, CRC, then a magic. Newest valid generation wins, so a torn write
/// leaves the previous generation in force.
pub fn encode_local(payload: &[u8]) -> Vec<u8> {
    let mut out = payload.to_vec();
    out.extend_from_slice(&crc32c(payload).to_le_bytes());
    out.extend_from_slice(b"CNLF");
    out
}

pub fn parse_local(b: &[u8]) -> Option<&[u8]> {
    if b.len() < 8 || &b[b.len() - 4..] != b"CNLF" {
        return None;
    }
    let body = &b[..b.len() - 8];
    (crc32c(body) == le32(b, b.len() - 8)).then_some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_matches_the_published_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    fn sample() -> Vec<u8> {
        let header = SegmentHeader {
            doc_class: 2,
            device: [7; 16],
            sequence: 9,
            created_ms: -1,
            doc_key: doc_key("p"),
            prev_sha: [0; 32],
        };
        let mut b = encode_segment_header(&header).to_vec();
        let peer = encode_frame(FRAME_PEER, 0, 0, &peer_payload(5, 6));
        let crc0 = crc32c(&peer_payload(5, 6));
        b.extend_from_slice(&peer);
        let batch = batch_payload([1; 16], 1, 3, b"update bytes");
        b.extend_from_slice(&encode_frame(FRAME_BATCH, 1, crc0, &batch));
        b
    }

    #[test]
    fn a_header_round_trips_and_any_bit_flip_is_refused() {
        let b = sample();
        let h = parse_segment_header(&b).unwrap();
        assert_eq!(h.sequence, 9);
        assert_eq!(h.created_ms, -1);
        for i in 0..HEADER_LEN {
            let mut c = b.clone();
            c[i] ^= 0x10;
            assert!(parse_segment_header(&c).is_err(), "byte {i}");
        }
    }

    #[test]
    fn a_scan_finds_every_frame_and_stops_at_a_torn_tail() {
        let b = sample();
        let full = scan_segment(&b);
        assert_eq!(full.frames.len(), 2);
        assert_eq!(full.good_end, b.len());
        assert!(!full.damage);
        let first_end = full.frames[0].payload.1;
        for cut in HEADER_LEN..b.len() {
            let s = scan_segment(&b[..cut]);
            assert_eq!(
                s.good_end,
                if cut >= first_end {
                    first_end
                } else {
                    HEADER_LEN
                },
                "cut {cut}"
            );
            assert!(!s.damage, "a tail is not damage, cut {cut}");
        }
    }

    #[test]
    fn a_flip_inside_an_earlier_frame_is_damage_not_a_crash() {
        let b = sample();
        let full = scan_segment(&b);
        let mut c = b.clone();
        c[full.frames[0].payload.0] ^= 1;
        let s = scan_segment(&c);
        assert_eq!(s.good_end, HEADER_LEN);
        assert!(s.damage);
    }

    #[test]
    fn a_flip_of_any_bit_in_a_frame_header_stops_the_scan_there() {
        let b = sample();
        let full = scan_segment(&b);
        let f = full.frames[1];
        for at in f.offset..f.offset + FRAME_HEADER_LEN {
            for bit in 0..8 {
                let mut c = b.clone();
                c[at] ^= 1 << bit;
                let s = scan_segment(&c);
                assert_eq!(s.frames.len(), 1, "byte {} bit {bit}", at - f.offset);
                assert_eq!(s.good_end, f.offset);
            }
        }
    }

    #[test]
    fn a_swapped_frame_is_refused_by_the_chain() {
        let b = sample();
        let full = scan_segment(&b);
        let (f0, f1) = (full.frames[0], full.frames[1]);
        let mut c = b[..HEADER_LEN].to_vec();
        c.extend_from_slice(&b[f1.offset..f1.payload.1]);
        c.extend_from_slice(&b[f0.offset..f0.payload.1]);
        assert!(scan_segment(&c).frames.is_empty());
    }

    #[test]
    fn a_huge_declared_length_is_refused_before_use() {
        let mut f = encode_frame(FRAME_BATCH, 0, 0, b"x");
        f[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        let crc = crc32c(&f[..28]);
        f[28..32].copy_from_slice(&crc.to_le_bytes());
        let mut b = sample()[..HEADER_LEN].to_vec();
        b.extend_from_slice(&f);
        assert!(scan_segment(&b).frames.is_empty());
    }

    #[test]
    fn a_batch_payload_must_fill_exactly() {
        let p = batch_payload([1; 16], 1, 3, b"abc");
        assert_eq!(batch_update(&p), Some(&b"abc"[..]));
        let mut longer = p.clone();
        longer.push(0);
        assert_eq!(batch_update(&longer), None);
        assert_eq!(batch_update(&p[..50]), None);
    }

    #[test]
    fn segment_names_are_read_exactly() {
        let n = segment_name(&[0xab; 16], 0x1234);
        assert_eq!(parse_segment_name(&n), Some(([0xab; 16], 0x1234)));
        for bad in [
            "../x.seg",
            "abc-1.seg",
            "",
            &n.to_uppercase(),
            &format!("{n} (1)"),
        ] {
            assert_eq!(parse_segment_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_document_id_is_never_a_path() {
        assert!(doc_dir("../../etc/passwd").starts_with("pages/"));
        assert!(!doc_dir("../../etc/passwd").contains(".."));
        assert_eq!(doc_dir(""), "tree");
        assert!(doc_dir("d:x").starts_with("databases/"));
    }

    #[test]
    fn a_snapshot_counts_only_when_whole() {
        let s = Snapshot {
            doc_class: 2,
            kind: SNAPSHOT_BASE,
            doc_key: doc_key("p"),
            created_ms: 5,
            device: [3; 16],
            seq: 2,
            state_hash: [4; 32],
            covers: vec![([3; 16], 1, 2)],
            version: vec![1, 2],
            payload: vec![9; 100],
        };
        let b = encode_snapshot(&s);
        assert_eq!(parse_snapshot(&b), Some(s));
        for cut in 0..b.len() {
            assert!(parse_snapshot(&b[..cut]).is_none(), "cut {cut}");
        }
        let mut c = b.clone();
        c[150] ^= 1;
        assert!(parse_snapshot(&c).is_none());
    }
}
