#!/usr/bin/env python3
"""Keep the documents honest (docs/15). Exits non-zero and names every failure.

    python3 tools/check-docs.py             check the repository
    python3 tools/check-docs.py --selftest  prove each check can fail, on a copy
"""
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
TRACKER = "docs/14-EXECUTION-TRACKER.md"
ROW = re.compile(r"^\|\s*([A-Z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)+)\s*\|")
STATUSES = {"Open", "In progress", "Blocked", "Done"}
ID_REF = re.compile(r"`((?:DEC|DOC|PROTO|PKG|GOV|SPK|MOD|OPS|MRG|STO|SDK|WEB|DB|I18N|A11Y|CNV|INK|RND|SRCH|EMB|GRF|BLK|FIL|TOOL|AST|MCP|DSK|IMP|REL|SYN|CRY|SRV|MOB|ENT|OCR|HWR|FND)-\d+)`")
ADR_REF = re.compile(r"\bADR-(\d{3})\b")
CODE_REF = re.compile(r"\bCN-[A-Z]{2,4}-\d{4}\b")
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
REQUIRED = ["README.md", "AGENTS.md", "CLAUDE.md", "CONTRIBUTING.md", "SECURITY.md", "GOVERNANCE.md", "CODE_OF_CONDUCT.md", "NOTICE", "CHANGELOG.md", "LICENSE"]
MD_EXEMPT = {"docs/12-OPEN-SOURCE-AUDIT.md"}


def fenced_free(text):
    out, fenced = [], False
    for line in text.splitlines():
        if line.startswith("```"):
            fenced = not fenced
            out.append("")
            continue
        out.append("" if fenced else line)
    return out


def check(root):
    problems = []
    docs = sorted(root.glob("docs/*.md"))
    markdown = docs + sorted(root.glob("*.md"))
    texts = {p: p.read_text("utf8") for p in markdown}

    for name in REQUIRED:
        if not (root / name).exists():
            problems.append(f"missing project file {name}")

    # Index: every document is listed, once.
    index = (root / "docs/00-README.md").read_text("utf8")
    for p in docs:
        if p.name != "00-README.md" and f"({p.name})" not in index:
            problems.append(f"docs/00-README.md does not list {p.name}")

    # One tracker, one roadmap.
    for p in root.rglob("*"):
        if p.is_file() and ".git" not in p.parts and re.search(r"(TODO|ROADMAP|TRACKER|BACKLOG)", p.name, re.I):
            rel = p.relative_to(root).as_posix()
            if rel not in (TRACKER, "docs/06-ROADMAP-AND-DELIVERY.md"):
                problems.append(f"second tracker or roadmap: {rel}")

    # Links.
    for p, text in texts.items():
        for number, line in enumerate(fenced_free(text), 1):
            for target in LINK.findall(re.sub(r"`[^`]*`", "", line)):
                if re.match(r"^[a-z]+:", target) or target.startswith("#"):
                    continue
                path = target.split("#")[0]
                if path and not (p.parent / path).exists():
                    problems.append(f"{p.relative_to(root)}:{number}: broken link {target}")

    # Tracker rows.
    tracker = (root / TRACKER).read_text("utf8")
    ids = {}
    for number, line in enumerate(fenced_free(tracker), 1):
        m = ROW.match(line)
        if not m:
            continue
        if m.group(1) in ids:
            problems.append(f"{TRACKER}:{number}: duplicate id {m.group(1)}")
        ids[m.group(1)] = line
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) > 2 and cells[2] not in STATUSES:
            problems.append(f"{TRACKER}:{number}: {m.group(1)} has unknown status {cells[2]!r}")
        if len(cells) > 2 and cells[2] == "Blocked" and not re.search(r"(Waits on|Blocked by|Needs)", line):
            problems.append(f"{TRACKER}:{number}: {m.group(1)} is Blocked without naming why")

    # References must exist.
    adrs = set(re.findall(r"^\| ADR-(\d{3}) \|", (root / "docs/08-ADR-REGISTER.md").read_text("utf8"), re.M))
    registry = (root / "docs/20-ERROR-CODE-REGISTRY.md")
    codes = set(CODE_REF.findall(registry.read_text("utf8"))) if registry.exists() else set()
    if not registry.exists():
        problems.append("docs/20-ERROR-CODE-REGISTRY.md is missing")
    for p, text in texts.items():
        rel = p.relative_to(root)
        lines = fenced_free(text)
        for number, line in enumerate(lines, 1):
            for ref in ID_REF.findall(line):
                if ref not in ids:
                    problems.append(f"{rel}:{number}: unknown tracker id {ref}")
            if rel.as_posix() != "docs/08-ADR-REGISTER.md" and "its ADR-" not in line:
                for a in ADR_REF.findall(line):
                    if a not in adrs:
                        problems.append(f"{rel}:{number}: unknown ADR-{a}")
            for code in CODE_REF.findall(line):
                if code not in codes:
                    problems.append(f"{rel}:{number}: {code} is not in the registry")
        for number, line in enumerate(text.splitlines(), 1):
            if re.search(r"\blossless\b", line, re.I) and "banned" not in line and rel.as_posix() not in MD_EXEMPT and "check-docs" not in line:
                problems.append(f"{rel}:{number}: the word 'lossless' is banned")

    # The tool's refusal codes must all be in the spec and the registry.
    spec = (root / "docs/36-EXPORT-FORMAT.md").read_text("utf8")
    reader = root / "tools/cnotepack.py"
    if reader.exists():
        for code in set(CODE_REF.findall(reader.read_text("utf8"))):
            if code not in spec:
                problems.append(f"tools/cnotepack.py uses {code}, which docs/36 does not define")

    # A status line that says built, in a design that is not.
    for p, text in texts.items():
        if re.search(r"^\*\*Status:[^\n]*\b(built|shipped)\b(?! )", text, re.M | re.I) and not re.search(r"nothing (here )?(is )?built|not built", text, re.I):
            problems.append(f"{p.relative_to(root)}: its status claims something is built")
    return problems


def selftest():
    seeds = {
        "broken link": lambda r: append(r / "docs/02-ARCHITECTURE.md", "\n[x](nope.md)\n"),
        "unknown tracker id": lambda r: append(r / "docs/02-ARCHITECTURE.md", "\n`SPK-99`\n"),
        "unknown ADR": lambda r: append(r / "docs/02-ARCHITECTURE.md", "\nADR-999\n"),
        "unknown error code": lambda r: append(r / "docs/02-ARCHITECTURE.md", "\nCN-XXX-0001\n"),
        "banned word": lambda r: append(r / "docs/02-ARCHITECTURE.md", "\nThis is lossless.\n"),
        "duplicate id": lambda r: append(r / TRACKER, "\n| DOC-001 | Copy | Open | P1 | x | y |\n"),
        "bad status": lambda r: append(r / TRACKER, "\n| ZZZ-01 | Copy | Maybe | P1 | x | y |\n"),
        "blocked without reason": lambda r: append(r / TRACKER, "\n| ZZZ-02 | Copy | Blocked | P1 | x | y |\n"),
        "doc not indexed": lambda r: (r / "docs/99-NEW.md").write_text("# 99\n"),
        "second tracker": lambda r: (r / "docs/MY-TODO.md").write_text("# todo\n"),
        "missing project file": lambda r: (r / "SECURITY.md").unlink(),
    }
    failed = 0
    for name, seed in seeds.items():
        with tempfile.TemporaryDirectory() as tmp:
            copy = pathlib.Path(tmp) / "r"
            shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns("prototypes", "__pycache__", ".git"))
            (copy / "docs/prototypes").mkdir()
            (copy / "docs/prototypes/casualnote-prototype.html").write_text("x")
            if check(copy):
                print(f"selftest: the clean copy already fails, so {name} proves nothing")
                failed += 1
                continue
            seed(copy)
            caught = check(copy)
            print(f"{'caught' if caught else 'MISSED'}: {name}" + (f" -> {caught[0]}" if caught else ""))
            failed += not caught
    return 1 if failed else 0


def append(path, text):
    path.write_text(path.read_text("utf8") + text, "utf8")


def main():
    if "--selftest" in sys.argv:
        return selftest()
    problems = check(ROOT)
    for problem in problems:
        print(problem)
    if problems:
        print(f"\n{len(problems)} problems.")
        return 1
    docs = len(list(ROOT.glob("docs/*.md")))
    print(f"OK. {docs} documents checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
