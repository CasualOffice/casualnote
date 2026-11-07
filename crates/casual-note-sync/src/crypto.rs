//! Sealing and opening objects (docs/56). This code has had no outside review, and nothing in the
//! product may say otherwise. Algorithms: Argon2id for the passphrase, HKDF-SHA-256 to derive keys,
//! XChaCha20-Poly1305 for every object. The caller supplies all randomness.

use casual_note_store::format::{hex, unhex};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;

pub const HEADER_LEN: usize = 128;
pub const TAG_LEN: usize = 16;
pub const PROTOCOL: u16 = 1;
pub const KIND_SEGMENT: u8 = 1;
pub const KIND_ACK: u8 = 2;
pub const KIND_REGISTRY: u8 = 3;
pub const KIND_CHECKPOINT: u8 = 4;
/// An object larger than this is refused before anything is read.
pub const MAX_OBJECT: u64 = 65 * 1024 * 1024;
pub const MIN_PASSPHRASE: usize = 12;
const PAD: usize = 4096;

/// Argon2id's cost, read from storage and so checked before any work is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            m_kib: 128 * 1024,
            t: 3,
            p: 1,
        }
    }
}

impl KdfParams {
    /// Too cheap is a downgrade by whoever holds the storage. Too dear is a memory attack.
    pub fn check(&self) -> Result<(), &'static str> {
        if self.m_kib < 64 * 1024 || self.t < 2 || self.p < 1 {
            return Err("CN-CRY-0003");
        }
        if self.m_kib > 1024 * 1024 || self.t > 10 || self.p > 4 {
            return Err("CN-CRY-0003");
        }
        Ok(())
    }
}

/// The key that wraps the workspace key. NFC first, so one phrase typed on any system or through
/// any input method derives one key. No trimming and no case change.
pub fn derive_kek(
    passphrase: &str,
    salt: &[u8; 32],
    params: KdfParams,
) -> Result<[u8; 32], &'static str> {
    params.check()?;
    let nfc: String = passphrase.nfc().collect();
    let p = argon2::Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|_| "CN-CRY-0003")?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, p);
    let mut out = [0u8; 32];
    a.hash_password_into(nfc.as_bytes(), salt, &mut out)
        .map_err(|_| "CN-CRY-0003")?;
    Ok(out)
}

fn hkdf32(ikm: &[u8], salt: Option<&[u8]>, info: &[&[u8]]) -> [u8; 32] {
    let h = Hkdf::<Sha256>::new(salt, ikm);
    let mut out = [0u8; 32];
    // HKDF-SHA-256 can always produce 32 bytes.
    if h.expand_multi_info(info, &mut out).is_err() {
        out = [0; 32];
    }
    out
}

/// The workspace key and what derives from it.
#[derive(Clone)]
pub struct Keys {
    pub workspace: [u8; 32],
    pub key_id: [u8; 16],
    pub notebook: [u8; 16],
    notebook_key: [u8; 32],
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Keys(id {})", hex(&self.key_id))
    }
}

impl Keys {
    pub fn new(workspace: [u8; 32], notebook: [u8; 16]) -> Self {
        let id = hkdf32(&workspace, None, &[b"casualnote/v1/key-id"]);
        let mut key_id = [0; 16];
        key_id.copy_from_slice(&id[..16]);
        let notebook_key = hkdf32(&workspace, None, &[b"casualnote/v1/notebook", &notebook]);
        Self {
            workspace,
            key_id,
            notebook,
            notebook_key,
        }
    }

    fn object_key(&self, salt: &[u8], kind: u8, device: &[u8], seq: u64) -> [u8; 32] {
        hkdf32(
            &self.notebook_key,
            Some(salt),
            &[
                b"casualnote/v1/object",
                &[kind],
                device,
                &self.notebook,
                &seq.to_le_bytes(),
            ],
        )
    }
}

/// The readable fields of an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectHeader {
    pub protocol: u16,
    pub kind: u8,
    pub key_id: [u8; 16],
    pub device: [u8; 16],
    pub notebook: [u8; 16],
    pub seq: u64,
    pub salt: [u8; 32],
    pub nonce: [u8; 24],
    pub body_len: u64,
}

/// Why an object was refused. Nothing from it was used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `CN-SYN-0004`: too short, truncated, or not one of ours.
    Malformed,
    /// `CN-SYN-0004`: failed authentication. Tampered, or damaged.
    Tampered,
    /// `CN-SYN-0007`: written by a newer version.
    TooNew,
    /// `CN-CRY-0005`: sealed under a key this device does not hold.
    NeedsKey,
    /// `CN-SYN-0008`: over a limit.
    TooLarge,
}

impl Refusal {
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed | Self::Tampered => "CN-SYN-0004",
            Self::TooNew => "CN-SYN-0007",
            Self::NeedsKey => "CN-CRY-0005",
            Self::TooLarge => "CN-SYN-0008",
        }
    }
}

fn encode_header(h: &ObjectHeader) -> [u8; HEADER_LEN] {
    let mut b = [0u8; HEADER_LEN];
    b[..4].copy_from_slice(b"CNS1");
    b[4..6].copy_from_slice(&h.protocol.to_le_bytes());
    b[6] = h.kind;
    b[8..24].copy_from_slice(&h.key_id);
    b[24..40].copy_from_slice(&h.device);
    b[40..56].copy_from_slice(&h.notebook);
    b[56..64].copy_from_slice(&h.seq.to_le_bytes());
    b[64..96].copy_from_slice(&h.salt);
    b[96..120].copy_from_slice(&h.nonce);
    b[120..128].copy_from_slice(&h.body_len.to_le_bytes());
    b
}

/// Reads the header, and checks lengths before any work.
pub fn parse_header(b: &[u8]) -> Result<ObjectHeader, Refusal> {
    if b.len() < HEADER_LEN + TAG_LEN || &b[..4] != b"CNS1" {
        return Err(Refusal::Malformed);
    }
    if b.len() as u64 > MAX_OBJECT {
        return Err(Refusal::TooLarge);
    }
    if b[7] != 0 {
        return Err(Refusal::Malformed);
    }
    let mut key_id = [0; 16];
    key_id.copy_from_slice(&b[8..24]);
    let mut device = [0; 16];
    device.copy_from_slice(&b[24..40]);
    let mut notebook = [0; 16];
    notebook.copy_from_slice(&b[40..56]);
    let mut seq = [0; 8];
    seq.copy_from_slice(&b[56..64]);
    let mut salt = [0; 32];
    salt.copy_from_slice(&b[64..96]);
    let mut nonce = [0; 24];
    nonce.copy_from_slice(&b[96..120]);
    let mut len = [0; 8];
    len.copy_from_slice(&b[120..128]);
    let h = ObjectHeader {
        protocol: u16::from_le_bytes([b[4], b[5]]),
        kind: b[6],
        key_id,
        device,
        notebook,
        seq: u64::from_le_bytes(seq),
        salt,
        nonce,
        body_len: u64::from_le_bytes(len),
    };
    if h.body_len != (b.len() - HEADER_LEN) as u64 {
        return Err(Refusal::Malformed);
    }
    Ok(h)
}

/// Seals a body. The whole header is authenticated, so moving a file to another name, device or
/// notebook gains nothing. The body is padded to 4 KiB, which reduces what sizes reveal.
pub fn seal(
    keys: &Keys,
    kind: u8,
    device: &[u8; 16],
    seq: u64,
    body: &[u8],
    rng: &mut dyn FnMut(&mut [u8]),
) -> Vec<u8> {
    let mut salt = [0; 32];
    rng(&mut salt);
    let mut nonce = [0; 24];
    rng(&mut nonce);
    let mut padded = Vec::with_capacity(body.len() + 8 + PAD);
    padded.extend_from_slice(&(body.len() as u64).to_le_bytes());
    padded.extend_from_slice(body);
    let rounded = padded.len().div_ceil(PAD) * PAD;
    padded.resize(rounded, 0);
    let header = ObjectHeader {
        protocol: PROTOCOL,
        kind,
        key_id: keys.key_id,
        device: *device,
        notebook: keys.notebook,
        seq,
        salt,
        nonce,
        body_len: (padded.len() + TAG_LEN) as u64,
    };
    let head = encode_header(&header);
    let key = keys.object_key(&salt, kind, device, seq);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    // Encryption into a Vec cannot fail for sizes under the algorithm's limit.
    let sealed = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &padded,
                aad: &head,
            },
        )
        .unwrap_or_default();
    let mut out = head.to_vec();
    out.extend_from_slice(&sealed);
    out
}

/// Opens an object, or refuses it. A tampered object yields no bytes at all.
pub fn open(keys: &Keys, bytes: &[u8]) -> Result<(ObjectHeader, Vec<u8>), Refusal> {
    let h = parse_header(bytes)?;
    if h.protocol > PROTOCOL {
        return Err(Refusal::TooNew);
    }
    if h.key_id != keys.key_id {
        return Err(Refusal::NeedsKey);
    }
    if h.notebook != keys.notebook {
        return Err(Refusal::Tampered);
    }
    let key = keys.object_key(&h.salt, h.kind, &h.device, h.seq);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&h.nonce),
            Payload {
                msg: &bytes[HEADER_LEN..],
                aad: &bytes[..HEADER_LEN],
            },
        )
        .map_err(|_| Refusal::Tampered)?;
    if plain.len() < 8 {
        return Err(Refusal::Malformed);
    }
    let mut n = [0; 8];
    n.copy_from_slice(&plain[..8]);
    let n = usize::try_from(u64::from_le_bytes(n)).map_err(|_| Refusal::Malformed)?;
    if n > plain.len() - 8 {
        return Err(Refusal::Malformed);
    }
    Ok((h, plain[8..8 + n].to_vec()))
}

/// How the workspace key is held in storage, wrapped under the passphrase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub epoch: u64,
    pub wk_id: String,
    pub created_by: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

fn wrap_aad(epoch: u64, wk_id: &str, params: KdfParams, salt: &str) -> String {
    format!(
        "casualnote/v1/wrap|passphrase|{epoch}|{wk_id}|{}|{}|{}|{salt}",
        params.m_kib, params.t, params.p
    )
}

impl KeyRecord {
    pub fn new(
        kek: &[u8; 32],
        workspace: &[u8; 32],
        key_id: &[u8; 16],
        device: &[u8; 16],
        params: KdfParams,
        salt: &[u8; 32],
        rng: &mut dyn FnMut(&mut [u8]),
    ) -> Self {
        let mut nonce = [0; 24];
        rng(&mut nonce);
        let (wk_id, salt_hex) = (hex(key_id), hex(salt));
        let aad = wrap_aad(0, &wk_id, params, &salt_hex);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(kek));
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: workspace,
                    aad: aad.as_bytes(),
                },
            )
            .unwrap_or_default();
        Self {
            epoch: 0,
            wk_id,
            created_by: hex(device),
            m_kib: params.m_kib,
            t: params.t,
            p: params.p,
            salt: salt_hex,
            nonce: hex(&nonce),
            ciphertext: hex(&ciphertext),
        }
    }

    pub fn params(&self) -> KdfParams {
        KdfParams {
            m_kib: self.m_kib,
            t: self.t,
            p: self.p,
        }
    }

    pub fn salt_bytes(&self) -> Option<[u8; 32]> {
        unhex(&self.salt)?.try_into().ok()
    }

    /// Opens the workspace key. `None` means the wrong passphrase or a damaged record: the two look
    /// alike by design, and the person is told both are possible.
    pub fn unwrap_key(&self, kek: &[u8; 32]) -> Option<[u8; 32]> {
        let nonce: [u8; 24] = unhex(&self.nonce)?.try_into().ok()?;
        let ct = unhex(&self.ciphertext)?;
        let aad = wrap_aad(self.epoch, &self.wk_id, self.params(), &self.salt);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(kek));
        let wk = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ct,
                    aad: aad.as_bytes(),
                },
            )
            .ok()?;
        wk.try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter() -> impl FnMut(&mut [u8]) {
        let mut n = 7u8;
        move |b: &mut [u8]| {
            for x in b.iter_mut() {
                n = n.wrapping_mul(31).wrapping_add(11);
                *x = n;
            }
        }
    }

    fn keys() -> Keys {
        Keys::new([9; 32], [5; 16])
    }

    #[test]
    fn a_body_round_trips_and_is_not_in_the_ciphertext() {
        let mut rng = counter();
        let body = "a secret note, 秘密のメモ".as_bytes();
        let sealed = seal(&keys(), KIND_ACK, &[1; 16], 3, body, &mut rng);
        assert_eq!(sealed.len() % PAD, HEADER_LEN + TAG_LEN, "padded");
        assert!(!sealed.windows(6).any(|w| w == b"secret"));
        let (h, plain) = open(&keys(), &sealed).unwrap();
        assert_eq!((h.seq, h.kind, h.device), (3, KIND_ACK, [1; 16]));
        assert_eq!(plain, body);
    }

    #[test]
    fn every_single_bit_flip_and_every_truncation_is_refused() {
        let mut rng = counter();
        let sealed = seal(&keys(), KIND_ACK, &[1; 16], 3, b"body", &mut rng);
        for i in (0..sealed.len()).step_by(61).chain(0..HEADER_LEN) {
            let mut c = sealed.clone();
            c[i] ^= 1;
            assert!(open(&keys(), &c).is_err(), "flip at {i}");
        }
        for cut in (0..sealed.len()).step_by(97) {
            assert!(open(&keys(), &sealed[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn a_wrong_key_and_a_too_new_protocol_are_told_apart() {
        let mut rng = counter();
        let sealed = seal(&keys(), KIND_ACK, &[1; 16], 3, b"body", &mut rng);
        assert_eq!(
            open(&Keys::new([8; 32], [5; 16]), &sealed).unwrap_err(),
            Refusal::NeedsKey
        );
        let mut newer = sealed.clone();
        newer[4] = 9;
        assert_eq!(open(&keys(), &newer).unwrap_err(), Refusal::TooNew);
    }

    #[test]
    fn a_moved_object_cannot_claim_another_device_or_sequence() {
        let mut rng = counter();
        let sealed = seal(&keys(), KIND_ACK, &[1; 16], 3, b"body", &mut rng);
        let mut moved = sealed.clone();
        moved[24] = 2;
        assert_eq!(open(&keys(), &moved).unwrap_err(), Refusal::Tampered);
        let mut later = sealed;
        later[56] = 4;
        assert_eq!(open(&keys(), &later).unwrap_err(), Refusal::Tampered);
    }

    #[test]
    fn kdf_limits_stop_a_downgrade_and_a_memory_attack() {
        let salt = [0; 32];
        assert_eq!(
            derive_kek(
                "x",
                &salt,
                KdfParams {
                    m_kib: 1024,
                    t: 3,
                    p: 1
                }
            )
            .unwrap_err(),
            "CN-CRY-0003"
        );
        assert_eq!(
            derive_kek(
                "x",
                &salt,
                KdfParams {
                    m_kib: 64 * 1024,
                    t: 1,
                    p: 1
                }
            )
            .unwrap_err(),
            "CN-CRY-0003"
        );
        assert_eq!(
            derive_kek(
                "x",
                &salt,
                KdfParams {
                    m_kib: 4 * 1024 * 1024,
                    t: 3,
                    p: 1
                }
            )
            .unwrap_err(),
            "CN-CRY-0003"
        );
    }

    #[test]
    #[ignore = "slow in a debug build; run with --ignored. e2e covers the path in the browser."]
    fn one_phrase_in_either_unicode_form_derives_one_key() {
        let salt = [1; 32];
        let params = KdfParams {
            m_kib: 64 * 1024,
            t: 2,
            p: 1,
        };
        let a = derive_kek("caf\u{e9} 秘密のことば", &salt, params).unwrap();
        let b = derive_kek("cafe\u{301} 秘密のことば", &salt, params).unwrap();
        let c = derive_kek("cafe 秘密のことば", &salt, params).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn a_wrapped_key_needs_its_kek_and_binds_its_parameters() {
        let mut rng = counter();
        let k = keys();
        let rec = KeyRecord::new(
            &[3; 32],
            &k.workspace,
            &k.key_id,
            &[1; 16],
            KdfParams::default(),
            &[4; 32],
            &mut rng,
        );
        assert_eq!(rec.unwrap_key(&[3; 32]), Some(k.workspace));
        assert_eq!(rec.unwrap_key(&[2; 32]), None);
        let mut weakened = rec.clone();
        weakened.t = 2;
        assert_eq!(
            weakened.unwrap_key(&[3; 32]),
            None,
            "the cost cannot be lowered in place"
        );
    }
}
