//! Every refusal in docs/36 has a hostile package here, built by hand. Damage anywhere is refused
//! by name, never with a panic. tools/cnotepack.py must agree on each code, which
//! `the_python_reader_agrees` checks.

use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Run};
use casual_note_ops::{Op, apply};
use casual_note_package::{export, import_package, read_package, zip};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;

const MIME: &[u8] = b"application/vnd.casualnote.package+zip";

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn page(id: &str) -> Value {
    json!({ "id": id, "title": "مرحبا", "parent": null, "kind": "document", "view": "page", "favorite": false,
            "blocks": [{ "id": "b1", "type": "paragraph", "content": [{ "text": "नमस्ते" }] }] })
}

/// A valid package, as named files.
#[derive(Clone)]
struct Fixture {
    files: Vec<(String, Vec<u8>)>,
    manifest: Value,
    extra: Vec<(String, Vec<u8>)>,
}

impl Fixture {
    fn new() -> Self {
        let mut f = Self {
            files: vec![],
            manifest: Value::Null,
            extra: vec![],
        };
        f.set("pages/p1.json", serde_json::to_vec(&page("p1")).unwrap());
        f.set("databases/d1.json", serde_json::to_vec(&json!({ "id": "d1", "name": "T", "defaultView": "table", "statuses": [{ "id": "s", "name": "x", "group": "todo" }], "columns": [], "rows": [] })).unwrap());
        f.set("readable/مرحبا.md", "# مرحبا\n".as_bytes().to_vec());
        f.manifest = json!({ "format": "casualnote.package", "version": "1.0", "created": "2026-11-07T12:00:00Z", "generator": "test", "scope": "workspace", "languages": [],
            "pages": [{ "id": "p1", "title": "مرحبا", "path": "pages/p1.json", "readable": "readable/مرحبا.md" }],
            "databases": [{ "id": "d1", "name": "T", "path": "databases/d1.json" }] });
        f
    }

    fn set(&mut self, name: &str, data: Vec<u8>) {
        self.files.retain(|(n, _)| n != name);
        self.files.push((name.into(), data));
    }

    fn entries(&self) -> Vec<(String, Vec<u8>)> {
        let mut manifest = self.manifest.clone();
        let listed: Vec<Value> = self
            .files
            .iter()
            .map(|(n, d)| json!({ "path": n, "sha256": hash(d), "bytes": d.len() }))
            .collect();
        if manifest.get("files").is_none() {
            manifest["files"] = Value::Array(listed);
        }
        let mut out = vec![
            ("mimetype".to_string(), MIME.to_vec()),
            (
                "manifest.json".to_string(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
        ];
        out.extend(self.files.clone());
        out.extend(self.extra.clone());
        out
    }

    fn bytes(&self) -> Vec<u8> {
        zip::write(&self.entries()).unwrap()
    }
}

fn refused(bytes: &[u8]) -> &'static str {
    read_package(bytes).err().map_or("accepted", |e| e.code)
}

fn python(bytes: &[u8], name: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cnotepack-hostile");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.cnotepack"));
    std::fs::write(&path, bytes).unwrap();
    let tool = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/cnotepack.py");
    let out = Command::new("python3")
        .arg(tool)
        .arg("verify")
        .arg(&path)
        .output()
        .expect("python3");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Patches every CD and local copy of a field, for headers the writer never produces.
fn patch_central(bytes: &mut [u8], name: &str, offset: usize, value: &[u8]) {
    let sig = [0x50, 0x4b, 0x01, 0x02];
    let mut at = 0;
    while at + 46 <= bytes.len() {
        if bytes[at..at + 4] == sig {
            let len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
            if &bytes[at + 46..at + 46 + len] == name.as_bytes() {
                bytes[at + offset..at + offset + value.len()].copy_from_slice(value);
                return;
            }
        }
        at += 1;
    }
    panic!("no central record for {name}");
}

fn deflated(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(data, 9)
}

/// A ZIP with deflated entries and full control over the header's claims.
fn raw_zip(entries: &[(&str, u16, &[u8], u32, u32)]) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, method, stored, crc, size) in entries {
        let at = out.len() as u32;
        let head = |sig: u32, v: &mut Vec<u8>, central_part: bool| {
            v.extend_from_slice(&sig.to_le_bytes());
            v.extend_from_slice(&20u16.to_le_bytes());
            if central_part {
                v.extend_from_slice(&20u16.to_le_bytes());
            }
            v.extend_from_slice(&0x800u16.to_le_bytes());
            v.extend_from_slice(&method.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&33u16.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&(name.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
        };
        head(0x0403_4b50, &mut out, false);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(stored);
        head(0x0201_4b50, &mut central, true);
        central.extend_from_slice(&[0; 2]);
        central.extend_from_slice(&[0; 2]);
        central.extend_from_slice(&[0; 2]);
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&at.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    let size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0; 2]);
    out
}

/// (name, the package, the code both readers must refuse it with)
fn cases() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    let good = Fixture::new();
    let mut c: Vec<(&'static str, Vec<u8>, &'static str)> = Vec::new();
    c.push((
        "not_a_zip",
        b"this is not a zip file at all, not even slightly".to_vec(),
        "CN-PKG-0001",
    ));
    c.push(("empty_file", vec![], "CN-PKG-0001"));
    let mut wrong_mime = good.entries();
    wrong_mime[0].1 = b"application/zip".to_vec();
    c.push((
        "wrong_mimetype",
        zip::write(&wrong_mime).unwrap(),
        "CN-PKG-0001",
    ));
    let mut mime_second = good.entries();
    mime_second.swap(0, 1);
    c.push((
        "mimetype_not_first",
        zip::write(&mime_second).unwrap(),
        "CN-PKG-0001",
    ));
    let compressed = deflated(MIME);
    c.push((
        "mimetype_deflated",
        raw_zip(&[(
            "mimetype",
            8,
            &compressed,
            zip::crc32(MIME),
            MIME.len() as u32,
        )]),
        "CN-PKG-0001",
    ));

    let mut no_manifest = good.entries();
    no_manifest.remove(1);
    c.push((
        "manifest_missing",
        zip::write(&no_manifest).unwrap(),
        "CN-PKG-0002",
    ));
    let mut bad_json = good.entries();
    bad_json[1].1 = b"{ not json".to_vec();
    c.push((
        "manifest_not_json",
        zip::write(&bad_json).unwrap(),
        "CN-PKG-0002",
    ));
    let mut other = good.clone();
    other.manifest["format"] = json!("something.else");
    c.push(("manifest_wrong_format", other.bytes(), "CN-PKG-0002"));
    let mut version = good.clone();
    version.manifest["version"] = json!("one");
    c.push(("manifest_bad_version", version.bytes(), "CN-PKG-0002"));

    let mut newer = good.clone();
    newer.manifest["version"] = json!("2.0");
    c.push(("newer_major", newer.bytes(), "CN-PKG-0003"));

    for (name, path) in [
        ("path_absolute", "/etc/passwd"),
        ("path_dotdot", "a/../../evil"),
        ("path_backslash", "a\\evil"),
        ("path_drive", "C:evil"),
        ("path_control", "a\u{1}b"),
        ("path_not_nfc", "cafe\u{301}.md"),
    ] {
        let mut f = good.clone();
        f.extra.push((path.to_string(), b"x".to_vec()));
        c.push((name, f.bytes(), "CN-PKG-0004"));
    }

    let mut dup = good.clone();
    dup.extra.push(("pages/p1.json".to_string(), b"x".to_vec()));
    c.push(("duplicate_path", dup.bytes(), "CN-PKG-0005"));
    let mut case = good.clone();
    case.extra
        .push(("PAGES/p1.JSON".to_string(), b"x".to_vec()));
    c.push(("differ_by_case", case.bytes(), "CN-PKG-0005"));
    let mut sharp = good.clone();
    sharp.extra.push(("straße.md".to_string(), b"x".to_vec()));
    sharp.extra.push(("STRASSE.md".to_string(), b"x".to_vec()));
    c.push(("differ_by_full_case_fold", sharp.bytes(), "CN-PKG-0005"));

    let mut encrypted = good.bytes();
    patch_central(&mut encrypted, "pages/p1.json", 8, &0x0801u16.to_le_bytes());
    c.push(("encrypted_entry", encrypted, "CN-PKG-0006"));
    let mut symlink = good.bytes();
    patch_central(
        &mut symlink,
        "pages/p1.json",
        38,
        &(0o120_777u32 << 16).to_le_bytes(),
    );
    c.push(("symlink_entry", symlink, "CN-PKG-0006"));

    let mut over = good.bytes();
    patch_central(&mut over, "pages/p1.json", 24, &(3u32 << 30).to_le_bytes());
    c.push(("header_says_over_2gib", over, "CN-PKG-0007"));
    let mut deep = good.clone();
    let nested = format!("{}{}", "[".repeat(100), "]".repeat(100));
    deep.set("pages/p1.json", nested.clone().into_bytes());
    c.push(("json_too_deep", deep.bytes(), "CN-PKG-0007"));
    let mut very_deep = good.clone();
    very_deep.set(
        "pages/p1.json",
        format!("{}{}", "[".repeat(300), "]".repeat(300)).into_bytes(),
    );
    c.push(("json_far_too_deep", very_deep.bytes(), "CN-PKG-0007"));
    let mut long_ink = ink_fixture("");
    long_ink.set("ink/p1.jsonl", vec![b'x'; (16 << 20) + 10]);
    c.push(("ink_line_too_long", long_ink.bytes(), "CN-PKG-0007"));
    let mut big_page = good.clone();
    let mut text = b"{\"id\":\"p1\",\"x\":\"".to_vec();
    text.resize((64 << 20) + 16, b'a');
    text.extend_from_slice(b"\"}");
    big_page.set("pages/p1.json", text);
    c.push(("page_over_64mib", big_page.bytes(), "CN-PKG-0007"));

    // A bomb: zeros deflate past 200:1.
    let bomb_plain = vec![0u8; 4 << 20];
    let mut bomb_zip = good.clone();
    bomb_zip.set("assets/bomb.bin", bomb_plain.clone());
    let bomb = bomb_entries(&bomb_zip, &bomb_plain);
    c.push(("ratio_over_200", bomb, "CN-PKG-0007"));

    let mut wrong_hash = good.clone();
    let mut listed: Vec<Value> = good
        .files
        .iter()
        .map(|(n, d)| json!({ "path": n, "sha256": hash(d), "bytes": d.len() }))
        .collect();
    listed[0]["sha256"] = json!("0".repeat(64));
    wrong_hash.manifest["files"] = Value::Array(listed);
    c.push(("hash_differs", wrong_hash.bytes(), "CN-PKG-0008"));
    let mut wrong_size = good.clone();
    let mut listed: Vec<Value> = good
        .files
        .iter()
        .map(|(n, d)| json!({ "path": n, "sha256": hash(d), "bytes": d.len() }))
        .collect();
    listed[0]["bytes"] = json!(1);
    wrong_size.manifest["files"] = Value::Array(listed);
    c.push(("size_differs", wrong_size.bytes(), "CN-PKG-0008"));
    let mut missing = good.clone();
    let mut listed: Vec<Value> = good
        .files
        .iter()
        .map(|(n, d)| json!({ "path": n, "sha256": hash(d), "bytes": d.len() }))
        .collect();
    listed.push(json!({ "path": "assets/absent.png", "sha256": "0".repeat(64), "bytes": 1 }));
    missing.manifest["files"] = Value::Array(listed);
    c.push(("listed_file_missing", missing.bytes(), "CN-PKG-0008"));

    let mut unlisted = good.clone();
    unlisted
        .extra
        .push(("assets/stowaway.bin".to_string(), b"x".to_vec()));
    c.push(("file_not_listed", unlisted.bytes(), "CN-PKG-0009"));

    let mut not_page = good.clone();
    not_page.set("pages/p1.json", b"[]".to_vec());
    c.push(("page_not_an_object", not_page.bytes(), "CN-PKG-0010"));
    let mut other_id = good.clone();
    other_id.set("pages/p1.json", serde_json::to_vec(&page("p2")).unwrap());
    c.push(("page_id_differs", other_id.bytes(), "CN-PKG-0010"));
    let mut ink_inline = good.clone();
    let mut p = page("p1");
    p["canvas"] = json!({ "note": { "x": 0, "y": 0, "width": 1 }, "items": [], "ink": [] });
    ink_inline.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    c.push(("ink_inside_page_json", ink_inline.bytes(), "CN-PKG-0010"));
    let mut other_ink = good.clone();
    let mut p = page("p1");
    p["canvas"] =
        json!({ "note": { "x": 0, "y": 0, "width": 1 }, "items": [], "inkFile": "ink/p2.jsonl" });
    other_ink.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    c.push(("ink_file_not_its_own", other_ink.bytes(), "CN-PKG-0010"));
    let mut absent_ink = good.clone();
    let mut p = page("p1");
    p["canvas"] =
        json!({ "note": { "x": 0, "y": 0, "width": 1 }, "items": [], "inkFile": "ink/p1.jsonl" });
    absent_ink.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    c.push(("ink_file_absent", absent_ink.bytes(), "CN-PKG-0010"));
    let mut orphan = good.clone();
    orphan.set(
        "ink/p1.jsonl",
        b"{\"id\":\"s\",\"kind\":\"arrow\",\"from\":[0,0],\"to\":[1,1]}\n".to_vec(),
    );
    c.push(("ink_no_page_names", orphan.bytes(), "CN-PKG-0010"));
    c.push((
        "ink_malformed_line",
        ink_fixture("not json\n").bytes(),
        "CN-PKG-0010",
    ));
    c.push((
        "ink_no_line_ending",
        ink_fixture("{\"id\":\"s\",\"kind\":\"arrow\",\"from\":[0,0],\"to\":[1,1]}").bytes(),
        "CN-PKG-0010",
    ));
    c.push((
        "ink_pressure_range",
        ink_fixture(
            "{\"color\":\"\",\"id\":\"s\",\"kind\":\"pen\",\"points\":[[0,0,2]],\"width\":1}\n",
        )
        .bytes(),
        "CN-PKG-0010",
    ));
    c.push((
        "ink_unknown_kind",
        ink_fixture("{\"id\":\"s\",\"kind\":\"laser\"}\n").bytes(),
        "CN-PKG-0010",
    ));
    let mut db_shape = good.clone();
    db_shape.set(
        "databases/d1.json",
        b"{\"id\":\"d1\",\"statuses\":1,\"columns\":[],\"rows\":[]}".to_vec(),
    );
    c.push(("database_shape", db_shape.bytes(), "CN-PKG-0010"));
    let mut block_shape = good.clone();
    let mut p = page("p1");
    p["blocks"] = json!([{ "id": "b", "type": "paragraph" }]);
    block_shape.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    c.push((
        "known_block_without_content",
        block_shape.bytes(),
        "CN-PKG-0010",
    ));
    c
}

fn ink_fixture(ink: &str) -> Fixture {
    let mut f = Fixture::new();
    let mut p = page("p1");
    p["canvas"] =
        json!({ "note": { "x": 0, "y": 0, "width": 1 }, "items": [], "inkFile": "ink/p1.jsonl" });
    f.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    f.set("ink/p1.jsonl", ink.as_bytes().to_vec());
    f
}

fn bomb_entries(f: &Fixture, plain: &[u8]) -> Vec<u8> {
    let compressed = deflated(plain);
    assert!(
        plain.len() > compressed.len() * 200,
        "a bomb must be over 200:1"
    );
    let others = f.entries();
    let mut parts: Vec<(&str, u16, &[u8], u32, u32)> = Vec::new();
    let crc = zip::crc32(plain);
    for (name, data) in &others {
        if name == "assets/bomb.bin" {
            parts.push(("assets/bomb.bin", 8, &compressed, crc, plain.len() as u32));
        } else {
            parts.push((name, 0, data, zip::crc32(data), data.len() as u32));
        }
    }
    raw_zip(&parts)
}

#[test]
fn each_refusal_has_a_hostile_package() {
    for (name, bytes, code) in cases() {
        assert_eq!(refused(&bytes), code, "{name}");
    }
}

#[test]
fn the_python_reader_agrees() {
    for (name, bytes, code) in cases() {
        let said = python(&bytes, name);
        // The two readers differ only in checks stricter than the schema's minimum.
        assert!(
            said.contains(code),
            "{name}: wanted {code}, python said {said}"
        );
    }
}

#[test]
fn a_valid_fixture_is_accepted_by_both() {
    let bytes = Fixture::new().bytes();
    assert_eq!(refused(&bytes), "accepted");
    assert!(python(&bytes, "valid").starts_with("OK."));
}

#[test]
fn a_header_that_lies_about_size_is_caught_while_inflating() {
    let plain = vec![0u8; 3 << 20];
    let compressed = deflated(&plain);
    let crc = zip::crc32(&plain);
    let zipped = raw_zip(&[
        ("mimetype", 0, MIME, zip::crc32(MIME), MIME.len() as u32),
        ("x", 8, &compressed, crc, 10),
    ]);
    let entries = zip::central_directory(&zipped).unwrap();
    let got = zip::stream(&zipped, &entries[1], 1 << 30, &mut |_| Ok(()));
    assert_eq!(got.unwrap_err().code, "CN-PKG-0007");
    let limited = raw_zip(&[
        ("mimetype", 0, MIME, zip::crc32(MIME), MIME.len() as u32),
        ("x", 8, &compressed, crc, plain.len() as u32),
    ]);
    let entries = zip::central_directory(&limited).unwrap();
    assert_eq!(
        zip::stream(&limited, &entries[1], 1024, &mut |_| Ok(()))
            .unwrap_err()
            .code,
        "CN-PKG-0007"
    );
}

fn casual_note_package_central(out: &mut Vec<u8>) {
    // Written through the public reader: an entry whose offset sits in the extra field.
    let name = b"m";
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&12u16.to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&5u64.to_le_bytes());
}

#[test]
fn central_records_with_a_wide_offset_parse() {
    let mut record = Vec::new();
    casual_note_package_central(&mut record);
    let start = 0u32;
    let mut file = record.clone();
    file.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&(record.len() as u32).to_le_bytes());
    file.extend_from_slice(&start.to_le_bytes());
    file.extend_from_slice(&[0; 2]);
    let entries = zip::central_directory(&file).unwrap();
    assert_eq!(
        entries[0].offset, 5,
        "the 64-bit offset came from the extra field"
    );
}

#[test]
fn a_count_that_lies_cannot_cause_a_huge_allocation() {
    let mut bytes = Fixture::new().bytes();
    let n = bytes.len();
    bytes[n - 12] = 0xff;
    bytes[n - 11] = 0xff;
    assert_eq!(refused(&bytes), "CN-PKG-0001");
}

#[test]
fn over_a_million_entries_is_refused() {
    // Only the central directory is read to find this, and the local data need not exist.
    let name = b"mimetype";
    let mut out = Vec::new();
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&[20, 0, 0, 8, 0, 0, 0, 0, 33, 0]);
    out.extend_from_slice(&zip::crc32(MIME).to_le_bytes());
    out.extend_from_slice(&(MIME.len() as u32).to_le_bytes());
    out.extend_from_slice(&(MIME.len() as u32).to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(name);
    out.extend_from_slice(MIME);
    let start = out.len() as u64;
    let record = |out: &mut Vec<u8>, name: &[u8], crc: u32, size: u32, offset: u32| {
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 20, 0, 0, 8, 0, 0, 0, 0, 33, 0]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name);
    };
    record(&mut out, name, zip::crc32(MIME), MIME.len() as u32, 0);
    let total = 1_000_002u64;
    for i in 1..total {
        record(&mut out, format!("f{i}").as_bytes(), 0, 0, 0);
    }
    let size = out.len() as u64 - start;
    let at = out.len() as u64;
    out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[0xff; 4]);
    out.extend_from_slice(&[0xff; 8]);
    out.extend_from_slice(&[0; 2]);
    assert_eq!(refused(&out), "CN-PKG-0007");
}

#[test]
fn a_block_type_it_does_not_know_is_kept_and_reported_through_a_rewrite() {
    let mut f = Fixture::new();
    let mut p = page("p1");
    p["blocks"] = json!([
        { "id": "b1", "type": "paragraph", "content": [{ "text": "ok" }] },
        { "id": "b2", "type": "video-2030", "content": [{ "text": "مرئي" }], "src": "x" },
        { "id": "b3", "type": "orbit", "weird": { "nested": [1, 2] } }
    ]);
    f.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    let bytes = f.bytes();
    let said = python(&bytes, "unknown_block");
    assert!(
        said.starts_with("OK.") && said.contains("video-2030"),
        "{said}"
    );
    let mut ws = Workspace::new(5).unwrap();
    let report = import_package(&mut ws, &bytes, 6).unwrap();
    assert_eq!(report.read.unknown_blocks.len(), 2);
    assert_eq!(report.read.unknown_blocks[0], ["p1", "b2", "video-2030"]);
    assert_eq!(
        ws.page("p1").unwrap().blocks[1].plain(),
        "مرئي",
        "its text stays visible"
    );
    let out = export(&ws, "2026-11-07T12:00:00Z", None).unwrap();
    let again: Value =
        serde_json::from_slice(&casual_note_package::entry_bytes(&out, "pages/p1.json").unwrap())
            .unwrap();
    assert_eq!(again["blocks"][1]["type"], "video-2030");
    assert_eq!(again["blocks"][1]["src"], "x");
    assert_eq!(
        again["blocks"][2]["weird"]["nested"][1], 2,
        "kept whole, for the next reader"
    );
    let said = python(&out, "unknown_block_again");
    assert!(said.starts_with("OK.") && said.contains("orbit"), "{said}");
}

#[test]
fn a_plain_page_link_from_another_writer_shows_a_title() {
    let mut f = Fixture::new();
    let mut p = page("p1");
    p["blocks"] = json!([{ "id": "b1", "type": "paragraph", "content": [{ "text": "see " }, { "page": "p1" }, { "text": " and " }, { "page": "gone" }] }]);
    f.set("pages/p1.json", serde_json::to_vec(&p).unwrap());
    let mut ws = Workspace::new(5).unwrap();
    let report = import_package(&mut ws, &f.bytes(), 6).unwrap();
    assert_eq!(report.dangling, ["gone"], "kept and reported");
    let got = ws.page("p1").unwrap();
    assert_eq!(got.blocks[0].plain(), "see مرحبا and Removed page");
    assert_eq!(
        got.blocks[0]
            .runs
            .iter()
            .filter(|r| r.link.is_some())
            .count(),
        2
    );
}

#[test]
fn a_parent_cycle_cannot_hang_an_import() {
    let mut f = Fixture::new();
    let mut a = page("p1");
    a["parent"] = json!("p2");
    let mut b = page("p2");
    b["parent"] = json!("p1");
    let mut c = page("p3");
    c["parent"] = json!("missing");
    f.set("pages/p1.json", serde_json::to_vec(&a).unwrap());
    f.set("pages/p2.json", serde_json::to_vec(&b).unwrap());
    f.set("pages/p3.json", serde_json::to_vec(&c).unwrap());
    f.manifest["pages"] = json!([
        { "id": "p1", "title": "a", "path": "pages/p1.json" },
        { "id": "p2", "title": "b", "path": "pages/p2.json" },
        { "id": "p3", "title": "c", "path": "pages/p3.json" }
    ]);
    let mut ws = Workspace::new(5).unwrap();
    let report = import_package(&mut ws, &f.bytes(), 6).unwrap();
    assert_eq!(report.read.moved_to_top, ["p3", "p1"]);
    assert_eq!(ws.page("p2").unwrap().meta.parent.as_deref(), Some("p1"));
    assert_eq!(ws.page("p1").unwrap().meta.parent, None);
}

#[test]
fn import_is_a_transaction_a_failure_leaves_nothing() {
    let mut f = Fixture::new();
    // A stroke past the engine's coordinate limit passes the format but not the engine.
    let mut p = page("p9");
    p["canvas"] =
        json!({ "note": { "x": 0, "y": 0, "width": 1 }, "items": [], "inkFile": "ink/p9.jsonl" });
    f.set("pages/p9.json", serde_json::to_vec(&p).unwrap());
    f.set(
        "ink/p9.jsonl",
        b"{\"color\":\"\",\"id\":\"s\",\"kind\":\"pen\",\"points\":[[1e300,0,0]],\"width\":1}\n"
            .to_vec(),
    );
    f.manifest["pages"] = json!([
        { "id": "p1", "title": "a", "path": "pages/p1.json" },
        { "id": "p9", "title": "z", "path": "pages/p9.json" }
    ]);
    let bytes = f.bytes();
    assert!(read_package(&bytes).is_ok(), "the format is fine");
    let mut ws = Workspace::new(5).unwrap();
    apply(
        &mut ws,
        &Op::CreatePage {
            id: "mine".into(),
            parent: None,
            title: "Mine".into(),
            database: None,
        },
    )
    .unwrap();
    apply(
        &mut ws,
        &Op::InsertBlock {
            page: "mine".into(),
            after: None,
            block: Block {
                id: "b".into(),
                kind: BlockKind::Paragraph,
                checked: false,
                runs: vec![Run {
                    text: "keep".into(),
                    marks: vec![],
                    link: None,
                }],
                target: None,
            },
        },
    )
    .unwrap();
    let before = ws.export().unwrap();
    let error = import_package(&mut ws, &bytes, 6).unwrap_err();
    assert_eq!(error.code, "CN-PKG-0010");
    assert!(
        !ws.has_page("p1"),
        "the page before the failure was not created"
    );
    assert_eq!(ws.pages().len(), 1);
    assert_eq!(
        ws.export().unwrap(),
        before,
        "not one byte of the workspace changed"
    );
}

#[test]
fn damage_anywhere_is_refused_by_name_and_never_panics() {
    let bytes = {
        let mut ws = Workspace::new(5).unwrap();
        import_package(&mut ws, &Fixture::new().bytes(), 6).unwrap();
        export(&ws, "2026-11-07T12:00:00Z", None).unwrap()
    };
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for at in 0..bytes.len() {
        let mut copy = bytes.clone();
        copy[at] ^= (next() as u8) | 1;
        if let Err(e) = read_package(&copy) {
            assert!(e.code.starts_with("CN-PKG-"), "{e}");
        }
    }
    for cut in (0..bytes.len()).step_by(7) {
        assert!(read_package(&bytes[..cut]).is_err(), "truncated at {cut}");
    }
    for _ in 0..500 {
        let mut noise = bytes.clone();
        for _ in 0..8 {
            let at = (next() as usize) % noise.len();
            noise[at] = next() as u8;
        }
        let _ = read_package(&noise);
    }
}
