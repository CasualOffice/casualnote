#!/usr/bin/env python3
"""Each refusal in docs/36 has a hostile package here. Run: python3 tools/test_cnotepack.py"""
import hashlib
import json
import pathlib
import secrets
import stat
import sys
import tempfile
import unittest
import unittest.mock
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cnotepack  # noqa: E402

PAGE = {"id": "p1", "title": "مرحبا 你好", "parent": None, "kind": "document", "view": "page", "favorite": False,
        "blocks": [{"id": "b1", "type": "paragraph", "content": [{"text": "नमस्ते 日本語 hola"}]}]}
DB = {"id": "d1", "name": "Tasks", "defaultView": "table", "statuses": [{"id": "s", "name": "To do", "group": "todo"}],
      "columns": [], "rows": []}
INK_PAGE = dict(PAGE, canvas={"note": {"x": 0, "y": 0, "width": 100}, "items": [], "inkFile": "ink/p1.jsonl"})
STROKE = {"color": "red", "id": "s1", "kind": "pen", "points": [[1, 2, 0.5], [3.25, 4, 1]], "width": 3}
ARROW = {"from": [0, 0], "id": "s2", "kind": "arrow", "to": [9, 9]}
SCHEMA = pathlib.Path(__file__).resolve().parent.parent / "schemas" / "cnotepack-1.schema.json"


def ink_line(record):
    return json.dumps(record, sort_keys=True, separators=(",", ":")).encode() + b"\n"


def ink_files(*lines, page=INK_PAGE):
    return {"pages/p1.json": json.dumps(page, ensure_ascii=False).encode(), "ink/p1.jsonl": b"".join(lines)}


def build(path, *, mutate_manifest=None, files=None, order_first=None, mimetype=cnotepack.MIMETYPE,
          mime_method=zipfile.ZIP_STORED, extra=None, unlisted=None, attrs=None, skip_manifest=False, flag=None, stored=()):
    """Write a package, valid unless a keyword breaks it."""
    contents = {"pages/p1.json": json.dumps(PAGE, ensure_ascii=False).encode(), "databases/d1.json": json.dumps(DB).encode(),
                "readable/مرحبا 你好.md": "# مرحبا 你好\n".encode(), "assets/" + hashlib.sha256(b"png").hexdigest() + ".png": b"png"}
    contents.update(files or {})
    manifest = {"format": "casualnote.package", "version": "1.0", "created": "2026-11-07T12:00:00Z", "generator": "test",
                "scope": "workspace", "languages": ["ar"],
                "pages": [{"id": "p1", "title": PAGE["title"], "path": "pages/p1.json", "readable": "readable/مرحبا 你好.md"}],
                "databases": [{"id": "d1", "name": "Tasks", "path": "databases/d1.json"}],
                "files": [{"path": n, "sha256": hashlib.sha256(c).hexdigest(), "bytes": len(c)} for n, c in contents.items()]}
    if mutate_manifest:
        mutate_manifest(manifest)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(zipfile.ZipInfo("mimetype"), mimetype, compress_type=mime_method)
        if not skip_manifest:
            z.writestr("manifest.json", json.dumps(manifest))
        for name, data in {**contents, **(unlisted or {})}.items():
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_STORED if name in stored else zipfile.ZIP_DEFLATED
            if attrs and name in attrs:
                info.external_attr = attrs[name]
            z.writestr(info, data)
        for name, data in (extra or []):
            z.writestr(zipfile.ZipInfo(name), data)
    if flag:
        flag(path)
    return path


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)
        self.path = self.dir / "t.cnotepack"

    def tearDown(self):
        self.tmp.cleanup()

    def refused(self, code):
        with self.assertRaises(cnotepack.Refused) as caught:
            cnotepack.inspect(self.path)
        self.assertEqual(caught.exception.code, code, str(caught.exception))

    def test_valid_package_is_accepted(self):
        build(self.path)
        manifest, _, _ = cnotepack.inspect(self.path)
        self.assertEqual(manifest["version"], "1.0")

    def test_text_in_every_script_survives_extraction(self):
        build(self.path)
        out = self.dir / "out"
        cnotepack.extract(self.path, out)
        self.assertEqual((out / "readable" / "مرحبا 你好.md").read_text("utf8"), "# مرحبا 你好\n")
        self.assertIn("नमस्ते 日本語 hola", (out / "pages" / "p1.json").read_text("utf8"))

    def test_damage_is_refused_by_name_and_never_changes_notes(self):
        build(self.path)
        original = self.path.read_bytes()
        def contents():
            _, names, archive = cnotepack.inspect(self.path)
            return {n: archive.read(i) for n, i in names.items()}
        clean = contents()
        for at in range(0, len(original), 7):
            damaged = bytearray(original)
            damaged[at] ^= 0xFF
            self.path.write_bytes(damaged)
            try:
                self.assertEqual(contents(), clean, f"byte {at} silently changed the notes")
            except cnotepack.Refused:
                pass

    def test_not_a_zip(self):
        self.path.write_bytes(b"this is not a zip")
        self.refused("CN-PKG-0001")

    def test_wrong_mimetype(self):
        build(self.path, mimetype=b"application/zip")
        self.refused("CN-PKG-0001")

    def test_compressed_mimetype(self):
        build(self.path, mime_method=zipfile.ZIP_DEFLATED)
        self.refused("CN-PKG-0001")

    def test_missing_manifest(self):
        build(self.path, skip_manifest=True)
        self.refused("CN-PKG-0002")

    def test_foreign_manifest(self):
        build(self.path, mutate_manifest=lambda m: m.update(format="something.else"))
        self.refused("CN-PKG-0002")

    def test_newer_major_version(self):
        build(self.path, mutate_manifest=lambda m: m.update(version="2.0"))
        self.refused("CN-PKG-0003")

    def test_newer_minor_version_is_accepted(self):
        build(self.path, mutate_manifest=lambda m: m.update(version="1.9"))
        cnotepack.inspect(self.path)

    def test_path_traversal(self):
        build(self.path, extra=[("../../evil.txt", b"x")])
        self.refused("CN-PKG-0004")

    def test_absolute_path(self):
        build(self.path, extra=[("/etc/evil", b"x")])
        self.refused("CN-PKG-0004")

    def test_windows_drive_path(self):
        build(self.path, extra=[("C:/evil", b"x")])
        self.refused("CN-PKG-0004")

    def test_decomposed_path(self):
        build(self.path, extra=[("readable/cafe\u0301.md", b"x")])
        self.refused("CN-PKG-0004")

    def test_paths_differing_only_by_case(self):
        build(self.path, extra=[("readable/A.md", b"x"), ("readable/a.md", b"y")])
        self.refused("CN-PKG-0005")

    def test_symbolic_link(self):
        link = (stat.S_IFLNK | 0o777) << 16
        build(self.path, files={"assets/link": b"/etc/passwd"}, attrs={"assets/link": link})
        self.refused("CN-PKG-0006")

    def test_encrypted_entry(self):
        def set_flag(path):
            data = bytearray(pathlib.Path(path).read_bytes())
            at = data.find(b"PK\x01\x02")
            while at != -1:
                name_len = int.from_bytes(data[at + 28:at + 30], "little")
                if data[at + 46:at + 46 + name_len] == b"databases/d1.json":
                    data[at + 8] |= 1
                at = data.find(b"PK\x01\x02", at + 4)
            pathlib.Path(path).write_bytes(data)
        build(self.path, flag=set_flag)
        self.refused("CN-PKG-0006")

    def test_zip_bomb_ratio(self):
        build(self.path, files={"assets/bomb.bin": b"\0" * (8 * 1024 * 1024)})
        self.refused("CN-PKG-0007")

    def test_json_nested_too_deep(self):
        deep = "[" * 200 + "]" * 200
        page = dict(PAGE, x_deep=None)
        text = json.dumps(page).replace("null}", deep + "}", 1) if False else json.dumps(page).replace('"x_deep": null', '"x-deep": ' + deep)
        build(self.path, files={"pages/p1.json": text.encode()})
        self.refused("CN-PKG-0007")

    def test_hash_mismatch(self):
        def corrupt(m):
            m["files"][0]["sha256"] = "0" * 64
        build(self.path, mutate_manifest=corrupt)
        self.refused("CN-PKG-0008")

    def test_listed_file_missing(self):
        def add(m):
            m["files"].append({"path": "assets/gone.png", "sha256": "0" * 64, "bytes": 1})
        build(self.path, mutate_manifest=add)
        self.refused("CN-PKG-0008")

    def test_unlisted_file(self):
        build(self.path, unlisted={"assets/smuggled.bin": b"x"})
        self.refused("CN-PKG-0009")

    def test_page_id_mismatch(self):
        build(self.path, files={"pages/p1.json": json.dumps(dict(PAGE, id="other")).encode()})
        self.refused("CN-PKG-0010")

    def test_unknown_block_type_is_kept_and_reported(self):
        page = dict(PAGE, blocks=[{"id": "b", "type": "futuretype", "x-new": 1}])
        build(self.path, files={"pages/p1.json": json.dumps(page).encode()})
        report = []
        _, names, archive = cnotepack.inspect(self.path, report)
        self.assertEqual(len(report), 1)
        self.assertIn("futuretype", report[0])
        self.assertIn("futuretype", archive.read(names["pages/p1.json"]).decode())

    def test_block_without_a_type_is_still_refused(self):
        page = dict(PAGE, blocks=[{"id": "b", "content": []}])
        build(self.path, files={"pages/p1.json": json.dumps(page).encode()})
        self.refused("CN-PKG-0010")

    def test_ink_file_is_accepted(self):
        build(self.path, files=ink_files(ink_line(STROKE), ink_line(ARROW)))
        cnotepack.inspect(self.path)

    def test_malformed_ink_line(self):
        build(self.path, files=ink_files(ink_line(STROKE), b'{"id":"s2","kind":"pen"\n'))
        self.refused("CN-PKG-0010")

    def test_ink_line_with_wrong_fields(self):
        for bad in (dict(STROKE, points=[[1, 2, 7]]), dict(ARROW, to=[1]), dict(STROKE, surprise=1), dict(STROKE, kind="laser"), dict(STROKE, width=0)):
            build(self.path, files=ink_files(ink_line(bad)))
            self.refused("CN-PKG-0010")

    def test_ink_line_with_non_finite_number(self):
        build(self.path, files=ink_files(b'{"from":[0,NaN],"id":"s2","kind":"arrow","to":[9,9]}\n'))
        self.refused("CN-PKG-0010")

    def test_over_long_ink_line(self):
        pad = secrets.token_hex(8 * 1024 * 1024 + 16)
        build(self.path, files=ink_files(ink_line(dict(ARROW, **{"x-pad": pad}))))
        self.refused("CN-PKG-0007")

    def test_ink_file_without_a_final_newline(self):
        build(self.path, files=ink_files(ink_line(STROKE).rstrip(b"\n")))
        self.refused("CN-PKG-0010")

    def test_ink_file_may_pass_the_json_limit(self):
        line = ink_line(ARROW)
        body = line * (cnotepack.MAX_JSON_BYTES // len(line) + 1000)
        build(self.path, files=ink_files(body), stored={"ink/p1.jsonl"})
        cnotepack.inspect(self.path)

    def test_ink_file_must_be_listed(self):
        build(self.path, files={"pages/p1.json": json.dumps(INK_PAGE).encode()})
        self.refused("CN-PKG-0010")

    def test_ink_file_must_be_the_pages_own(self):
        page = dict(INK_PAGE, canvas=dict(INK_PAGE["canvas"], inkFile="readable/مرحبا 你好.md"))
        build(self.path, files={"pages/p1.json": json.dumps(page, ensure_ascii=False).encode()})
        self.refused("CN-PKG-0010")

    def test_ink_file_no_page_names(self):
        build(self.path, files={"ink/p1.jsonl": ink_line(STROKE)})
        self.refused("CN-PKG-0010")

    def test_ink_inside_page_json_is_refused(self):
        page = dict(PAGE, canvas={"note": {"x": 0, "y": 0, "width": 100}, "items": [], "ink": [STROKE]})
        build(self.path, files={"pages/p1.json": json.dumps(page).encode()})
        self.refused("CN-PKG-0010")

    def test_stored_entry_with_extreme_redundancy_reads(self):
        build(self.path, files={"assets/zeros.bin": b"\0" * (8 * 1024 * 1024)}, stored={"assets/zeros.bin"})
        cnotepack.inspect(self.path)

    def test_more_than_100000_entries_read_through_zip64(self):
        assets = {f"assets/{i:06d}": b"" for i in range(100_001)}
        build(self.path, files=assets, stored=set(assets))
        self.assertIn(b"PK\x06\x06", self.path.read_bytes()[-4096:], "expected a ZIP64 end record")
        manifest, _, _ = cnotepack.inspect(self.path)
        self.assertGreater(len(manifest["files"]), 100_000)

    def test_entry_limit_is_enforced(self):
        build(self.path)
        with unittest.mock.patch.object(cnotepack, "MAX_ENTRIES", 3):
            self.refused("CN-PKG-0007")

    def test_manifest_may_pass_the_page_limit(self):
        pad = secrets.token_hex(cnotepack.MAX_JSON_BYTES // 2 + 1024)
        build(self.path, mutate_manifest=lambda m: m.update({"x-pad": pad}))
        cnotepack.inspect(self.path)

    def test_manifest_limit_is_enforced(self):
        pad = secrets.token_hex(1024 * 1024)
        build(self.path, mutate_manifest=lambda m: m.update({"x-pad": pad}))
        with unittest.mock.patch.object(cnotepack, "MAX_MANIFEST_BYTES", 1024 * 1024):
            self.refused("CN-PKG-0007")

    def test_page_json_keeps_its_limit(self):
        page = dict(PAGE, **{"x-pad": secrets.token_hex(1024 * 1024)})
        build(self.path, files={"pages/p1.json": json.dumps(page, ensure_ascii=False).encode()})
        with unittest.mock.patch.object(cnotepack, "MAX_JSON_BYTES", 1024 * 1024):
            self.refused("CN-PKG-0007")

    def test_schema_matches_the_reader(self):
        defs = json.loads(SCHEMA.read_text("utf8"))["$defs"]
        canvas = defs["page"]["properties"]["canvas"]
        self.assertIn("inkLine", defs)
        self.assertIn("inkFile", canvas["properties"])
        self.assertNotIn("ink", canvas["properties"])
        self.assertNotIn("ink", defs)
        self.assertNotIn("enum", defs["block"]["properties"]["type"])
        self.assertEqual(defs["database"]["properties"]["columns"]["items"]["properties"]["hidden"], {"type": "boolean"})

    def test_schema_accepts_and_refuses_samples(self):
        try:
            import jsonschema
        except ImportError:
            self.skipTest("jsonschema is only in the test environment")
        root = json.loads(SCHEMA.read_text("utf8"))
        def valid(name, value):
            return jsonschema.Draft202012Validator({**root, "$ref": f"#/$defs/{name}"}).is_valid(value)
        self.assertTrue(valid("page", INK_PAGE))
        self.assertTrue(valid("inkLine", STROKE))
        self.assertTrue(valid("inkLine", ARROW))
        self.assertFalse(valid("inkLine", dict(STROKE, points=[[1, 2, 7]])))
        self.assertFalse(valid("page", dict(PAGE, canvas={"note": {"x": 0, "y": 0, "width": 1}, "items": [], "ink": []})))
        self.assertTrue(valid("page", dict(PAGE, blocks=[{"id": "b", "type": "futuretype", "x-new": 1, "extra": 2}])))
        self.assertFalse(valid("page", dict(PAGE, blocks=[{"id": "b", "type": "paragraph", "surprise": 1, "content": []}])))
        self.assertFalse(valid("page", dict(PAGE, blocks=[{"id": "b", "type": "heading", "content": []}])))
        self.assertTrue(valid("database", dict(DB, columns=[{"id": "c", "name": "Old", "type": "text", "hidden": True}])))


if __name__ == "__main__":
    unittest.main(verbosity=1)
