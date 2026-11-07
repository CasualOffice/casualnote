#!/usr/bin/env python3
"""Read a CasualNote Package with nothing but the Python standard library (docs/36).

    python3 cnotepack.py verify notes.cnotepack
    python3 cnotepack.py list notes.cnotepack
    python3 cnotepack.py extract notes.cnotepack ./out

This is the walk-away test: it shares no code with CasualNote, so if it can read a package, a
package does not need us. Every package is treated as hostile. A refusal exits 1 and names its
code from docs/36.
"""
import hashlib
import json
import pathlib
import re
import stat
import sys
import unicodedata
import zipfile
import zlib

MIMETYPE = b"application/vnd.casualnote.package+zip"
SUPPORTED_MAJOR = 1
MAX_ENTRIES = 100_000
MAX_ENTRY_BYTES = 2 * 1024**3
MAX_TOTAL_BYTES = 20 * 1024**3
MAX_JSON_BYTES = 64 * 1024**2
MAX_RATIO = 200
RATIO_APPLIES_ABOVE = 1024**2
MAX_DEPTH = 64
CHUNK = 1024 * 1024

BLOCK_TYPES = {"paragraph", "heading", "bulleted", "numbered", "todo", "quote", "callout", "code", "divider", "database", "embed"}
TEXT_BLOCKS = {"paragraph", "heading", "bulleted", "numbered", "todo", "quote", "callout", "code"}


class Refused(Exception):
    def __init__(self, code, message):
        super().__init__(f"{code}: {message}")
        self.code = code


def check_path(name):
    """A path must be safe to use on any operating system. Refuse rather than repair."""
    bad = (
        not name
        or name.startswith("/")
        or "\\" in name
        or re.match(r"^[A-Za-z]:", name)
        or any(ord(c) < 32 or ord(c) == 127 for c in name)
        or any(part in ("", ".", "..") for part in name.split("/"))
        or unicodedata.normalize("NFC", name) != name
    )
    if bad:
        raise Refused("CN-PKG-0004", f"unsafe path {name!r}")


def read_bounded(archive, info, limit):
    """Measure while decompressing, because a ZIP header can lie about the size."""
    data = bytearray()
    with archive.open(info) as stream:
        while True:
            chunk = stream.read(CHUNK)
            if not chunk:
                return bytes(data)
            data += chunk
            if len(data) > limit:
                raise Refused("CN-PKG-0007", f"{info.filename} is larger than {limit} bytes")
            if len(data) > RATIO_APPLIES_ABOVE and len(data) > max(info.compress_size, 1) * MAX_RATIO:
                raise Refused("CN-PKG-0007", f"{info.filename} expands more than {MAX_RATIO}:1")


def hash_bounded(archive, info, budget):
    digest, size = hashlib.sha256(), 0
    with archive.open(info) as stream:
        while True:
            chunk = stream.read(CHUNK)
            if not chunk:
                return digest.hexdigest(), size
            size += len(chunk)
            digest.update(chunk)
            if size > MAX_ENTRY_BYTES or size > budget:
                raise Refused("CN-PKG-0007", f"{info.filename} is over the size limit")
            if size > RATIO_APPLIES_ABOVE and size > max(info.compress_size, 1) * MAX_RATIO:
                raise Refused("CN-PKG-0007", f"{info.filename} expands more than {MAX_RATIO}:1")


def depth(value, level=1):
    if level > MAX_DEPTH:
        return level
    if isinstance(value, dict):
        return max((depth(v, level + 1) for v in value.values()), default=level)
    if isinstance(value, list):
        return max((depth(v, level + 1) for v in value), default=level)
    return level


def parse_json(data, name, code):
    try:
        value = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, ValueError, RecursionError) as error:
        raise Refused(code, f"{name} is not valid JSON ({type(error).__name__})") from None
    if depth(value) > MAX_DEPTH:
        raise Refused("CN-PKG-0007", f"{name} is nested too deeply")
    return value


def check_page(page, expected_id, name):
    def fail(why):
        raise Refused("CN-PKG-0010", f"{name}: {why}")
    if not isinstance(page, dict):
        fail("not an object")
    if page.get("id") != expected_id:
        fail("id differs from the manifest")
    if not isinstance(page.get("title"), str):
        fail("title must be text")
    if page.get("kind") not in ("document", "database"):
        fail("unknown kind")
    if not isinstance(page.get("blocks"), list):
        fail("blocks must be a list")
    for block in page["blocks"]:
        if not isinstance(block, dict) or block.get("type") not in BLOCK_TYPES or not isinstance(block.get("id"), str):
            fail("a block is malformed")
        if block["type"] in TEXT_BLOCKS and not isinstance(block.get("content"), list):
            fail(f"block {block['id']} has no content")


def check_database(database, expected_id, name):
    def fail(why):
        raise Refused("CN-PKG-0010", f"{name}: {why}")
    if not isinstance(database, dict):
        fail("not an object")
    if database.get("id") != expected_id:
        fail("id differs from the manifest")
    for key in ("statuses", "columns", "rows"):
        if not isinstance(database.get(key), list):
            fail(f"{key} must be a list")


# What a damaged ZIP can raise from the standard library. None of these may reach the caller raw.
DAMAGE = (zipfile.BadZipFile, zlib.error, EOFError, NotImplementedError, RuntimeError, UnicodeError, OSError, ValueError, OverflowError)


def inspect(path):
    """Verify everything and return (manifest, {path: info}, open archive)."""
    try:
        return _inspect(path)
    except DAMAGE as error:
        raise Refused("CN-PKG-0001", f"the file is damaged ({type(error).__name__})") from None


def _inspect(path):
    archive = zipfile.ZipFile(path)
    entries = archive.infolist()
    if not entries:
        raise Refused("CN-PKG-0001", "the ZIP is empty")
    first = entries[0]
    if first.filename != "mimetype" or first.compress_type != zipfile.ZIP_STORED or first.file_size != len(MIMETYPE) or archive.read(first) != MIMETYPE:
        raise Refused("CN-PKG-0001", "mimetype must be the first entry, stored, with the exact value")
    if len(entries) > MAX_ENTRIES:
        raise Refused("CN-PKG-0007", f"more than {MAX_ENTRIES} entries")

    seen, by_name = set(), {}
    for info in entries:
        check_path(info.filename.rstrip("/") if info.is_dir() else info.filename)
        key = unicodedata.normalize("NFC", info.filename).casefold()
        if key in seen:
            raise Refused("CN-PKG-0005", f"duplicate path {info.filename!r}")
        seen.add(key)
        if info.flag_bits & 0x1:
            raise Refused("CN-PKG-0006", f"{info.filename} is encrypted")
        if stat.S_ISLNK(info.external_attr >> 16):
            raise Refused("CN-PKG-0006", f"{info.filename} is a symbolic link")
        if info.file_size > MAX_ENTRY_BYTES:
            raise Refused("CN-PKG-0007", f"{info.filename} is over the entry limit")
        if not info.is_dir():
            by_name[info.filename] = info

    if "manifest.json" not in by_name:
        raise Refused("CN-PKG-0002", "manifest.json is missing")
    manifest = parse_json(read_bounded(archive, by_name["manifest.json"], MAX_JSON_BYTES), "manifest.json", "CN-PKG-0002")
    if not isinstance(manifest, dict) or manifest.get("format") != "casualnote.package":
        raise Refused("CN-PKG-0002", "this is not a CasualNote Package manifest")
    version = manifest.get("version")
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+", version):
        raise Refused("CN-PKG-0002", "version must look like 1.0")
    if int(version.split(".")[0]) > SUPPORTED_MAJOR:
        raise Refused("CN-PKG-0003", f"package version {version} is newer than this reader supports")
    for key in ("pages", "databases", "files"):
        if not isinstance(manifest.get(key), list):
            raise Refused("CN-PKG-0002", f"manifest {key} must be a list")

    listed, budget = set(), MAX_TOTAL_BYTES
    for item in manifest["files"]:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            raise Refused("CN-PKG-0002", "a manifest file entry is malformed")
        name = item["path"]
        check_path(name)
        listed.add(name)
        if name not in by_name:
            raise Refused("CN-PKG-0008", f"{name} is listed but missing")
        digest, size = hash_bounded(archive, by_name[name], budget)
        budget -= size
        if digest != item.get("sha256") or size != item.get("bytes"):
            raise Refused("CN-PKG-0008", f"{name} does not match its hash or size")
    unlisted = set(by_name) - listed - {"mimetype", "manifest.json"}
    if unlisted:
        raise Refused("CN-PKG-0009", f"not in the manifest: {sorted(unlisted)[0]}")

    for kind, check in (("pages", check_page), ("databases", check_database)):
        for item in manifest[kind]:
            if not isinstance(item, dict) or item.get("path") not in by_name or not isinstance(item.get("id"), str):
                raise Refused("CN-PKG-0002", f"a manifest {kind} entry is malformed")
            value = parse_json(read_bounded(archive, by_name[item["path"]], MAX_JSON_BYTES), item["path"], "CN-PKG-0010")
            check(value, item["id"], item["path"])
    return manifest, by_name, archive


def extract(path, destination):
    manifest, by_name, archive = inspect(path)
    root = pathlib.Path(destination).resolve()
    for name, info in by_name.items():
        target = (root / name).resolve()
        if root not in target.parents:
            raise Refused("CN-PKG-0004", f"unsafe path {name!r}")
        target.parent.mkdir(parents=True, exist_ok=True)
        with archive.open(info) as source, open(target, "wb") as out:
            while chunk := source.read(CHUNK):
                out.write(chunk)
    return manifest


def main(argv):
    if len(argv) < 3 or argv[1] not in ("verify", "list", "extract") or (argv[1] == "extract" and len(argv) != 4):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        if argv[1] == "extract":
            manifest = extract(argv[2], argv[3])
            print(f"Extracted {len(manifest['files'])} files to {argv[3]}. Read the notes under readable/.")
            return 0
        manifest, _, _ = inspect(argv[2])
    except Refused as refusal:
        print(f"Refused. {refusal}", file=sys.stderr)
        return 1
    if argv[1] == "verify":
        print(f"OK. CasualNote Package {manifest['version']}: {len(manifest['pages'])} pages, {len(manifest['databases'])} databases, {len(manifest['files'])} files, all hashes match.")
    else:
        for page in manifest["pages"]:
            print(page.get("readable") or page["path"])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
