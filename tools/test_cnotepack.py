#!/usr/bin/env python3
"""Each refusal in docs/36 has a hostile package here. Run: python3 tools/test_cnotepack.py"""
import hashlib
import json
import pathlib
import stat
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cnotepack  # noqa: E402

PAGE = {"id": "p1", "title": "مرحبا 你好", "parent": None, "kind": "document", "view": "page", "favorite": False,
        "blocks": [{"id": "b1", "type": "paragraph", "content": [{"text": "नमस्ते 日本語 hola"}]}]}
DB = {"id": "d1", "name": "Tasks", "defaultView": "table", "statuses": [{"id": "s", "name": "To do", "group": "todo"}],
      "columns": [], "rows": []}


def build(path, *, mutate_manifest=None, files=None, order_first=None, mimetype=cnotepack.MIMETYPE,
          mime_method=zipfile.ZIP_STORED, extra=None, unlisted=None, attrs=None, skip_manifest=False, flag=None):
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
            info.compress_type = zipfile.ZIP_DEFLATED
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

    def test_unknown_block_type(self):
        page = dict(PAGE, blocks=[{"id": "b", "type": "script", "content": []}])
        build(self.path, files={"pages/p1.json": json.dumps(page).encode()})
        self.refused("CN-PKG-0010")


if __name__ == "__main__":
    unittest.main(verbosity=1)
