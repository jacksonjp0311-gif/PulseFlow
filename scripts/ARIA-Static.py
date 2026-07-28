#!/usr/bin/env python3
"""Dependency-free static verifier for the PulseFlow repository.

This complements the Windows ARIA compiler/smoke pipeline when Rust or
PowerShell is not installed. It does not replace cargo check/test.
"""
from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class DashboardParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ids: list[str] = []
        self.actions: list[str] = []
        self.tabs: list[str] = []
        self.views: list[str] = []

    def handle_starttag(self, _tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if values.get("id"):
            self.ids.append(str(values["id"]))
        if values.get("data-action"):
            self.actions.append(str(values["data-action"]))
        if values.get("data-tab"):
            self.tabs.append(str(values["data-tab"]))
        if values.get("data-view"):
            self.views.append(str(values["data-view"]))


def fail(message: str) -> None:
    print(f"├─ ◇  {message:<48} FAIL")
    raise SystemExit(1)


def passed(message: str) -> None:
    print(f"├─ ◆  {message:<48} PASS")


def scan_rust_delimiters(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    stack: list[tuple[str, int]] = []
    pairs = {")": "(", "]": "[", "}": "{"}
    index = 0
    line = 1
    state = "code"
    block_depth = 0
    raw_hashes = 0

    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if char == "\n":
            line += 1

        if state == "line_comment":
            if char == "\n":
                state = "code"
            index += 1
            continue
        if state == "block_comment":
            if char == "/" and next_char == "*":
                block_depth += 1
                index += 2
                continue
            if char == "*" and next_char == "/":
                block_depth -= 1
                index += 2
                if block_depth == 0:
                    state = "code"
                continue
            index += 1
            continue
        if state == "string":
            if char == "\\":
                index += 2
                continue
            if char == '"':
                state = "code"
            index += 1
            continue
        if state == "character":
            if char == "\\":
                index += 2
                continue
            if char == "'":
                state = "code"
            index += 1
            continue
        if state == "raw":
            closing = '"' + ("#" * raw_hashes)
            if text.startswith(closing, index):
                index += len(closing)
                state = "code"
                continue
            index += 1
            continue

        if char == "/" and next_char == "/":
            state = "line_comment"
            index += 2
            continue
        if char == "/" and next_char == "*":
            state = "block_comment"
            block_depth = 1
            index += 2
            continue
        if char == "r":
            match = re.match(r'r(#+)?"', text[index:])
            if match:
                raw_hashes = len(match.group(1) or "")
                index += len(match.group(0))
                state = "raw"
                continue
        if char == '"':
            state = "string"
            index += 1
            continue
        if char == "'" and "'" in text[index + 1:index + 5]:
            state = "character"
            index += 1
            continue
        if char in "([{":
            stack.append((char, line))
        elif char in ")]}":
            if not stack or stack[-1][0] != pairs[char]:
                fail(f"Rust delimiter mismatch in {path.name}:{line}")
            stack.pop()
        index += 1

    if state in {"string", "character", "raw", "block_comment"} or stack:
        fail(f"Rust lexical structure in {path.name}")


def main() -> int:
    print("◆  ARIA / PULSEFLOW STATIC VERIFICATION")
    print("│")

    required = [
        "Cargo.toml", "README.md", "config/pulseflow.json", "web/index.html",
        "schemas/ui-actions.json", "schemas/observation-frame.schema.json",
        "schemas/agent-directive.schema.json", "schemas/adaptive-suggestion.schema.json",
        "schemas/pulseflow-api.contract.json", "schemas/powershell-runtime.contract.json",
        "aria/ARIA-CONNECT.json", "scripts/ARIA-Handshake.ps1", "scripts/ARIA-Smoke.ps1",
        "src/main.rs", "src/server.rs", "tests/ui_contract_tests.rs",
    ]
    missing = [name for name in required if not (ROOT / name).exists()]
    if missing:
        fail("repository lattice: " + ", ".join(missing))
    passed("repository lattice")

    generated_roots = {"state", "target", "dist", ".cortex", "tools"}
    json_files = sorted(
        path
        for path in ROOT.rglob("*.json")
        if not generated_roots.intersection(path.relative_to(ROOT).parts)
    )
    for path in json_files:
        json.loads(path.read_text(encoding="utf-8"))
    passed(f"JSON parse ({len(json_files)} documents)")

    manifest = json.loads((ROOT / "MANIFEST.json").read_text(encoding="utf-8"))
    for entry in manifest["files"]:
        path = ROOT / entry["path"]
        if not path.is_file():
            fail(f"manifest missing {entry['path']}")
        data = path.read_bytes()
        if len(data) != entry["bytes"]:
            fail(f"manifest size {entry['path']}")
        if hashlib.sha256(data).hexdigest() != entry["sha256"]:
            fail(f"manifest hash {entry['path']}")
    passed(f"content-addressed manifest ({len(manifest['files'])} files)")

    ps_contract = json.loads((ROOT / "schemas/powershell-runtime.contract.json").read_text(encoding="utf-8"))
    ps_scripts = sorted((ROOT / "scripts").glob("*.ps1"))
    for path in ps_scripts:
        raw = path.read_bytes()
        if ps_contract["rules"]["ascii_safe"] and any(byte > 127 for byte in raw):
            fail(f"PowerShell ASCII boundary {path.name}")
        text = raw.decode("ascii")
        if ps_contract["rules"]["strict_mode_required"] and "Set-StrictMode" not in text:
            fail(f"PowerShell strict mode {path.name}")
        for rule in ps_contract["rules"]["forbidden_patterns"]:
            if re.search(rule["regex"], text):
                fail(f"PowerShell semantic rule {rule['id']} in {path.name}")
    smoke = (ROOT / ps_contract["smoke_script"]["path"]).read_text(encoding="ascii")
    for marker in ps_contract["smoke_script"]["required_markers"]:
        if marker not in smoke:
            fail(f"PowerShell smoke marker {marker}")
    passed(f"PowerShell semantic contract ({len(ps_scripts)} scripts)")

    html = (ROOT / "web/index.html").read_text(encoding="utf-8")
    server = (ROOT / "src/server.rs").read_text(encoding="utf-8")
    contract = json.loads((ROOT / "schemas/ui-actions.json").read_text(encoding="utf-8"))
    parser = DashboardParser()
    parser.feed(html)
    if len(parser.ids) != len(set(parser.ids)):
        fail("duplicate HTML ids")
    expected_actions = {entry["id"] for entry in contract["actions"]}
    if set(parser.actions) != expected_actions:
        fail("HTML/action-contract parity")
    if set(parser.tabs) != set(parser.views):
        fail("tab/view parity")
    for action in contract["actions"]:
        action_id = action["id"]
        if f'"{action_id}":' not in html:
            fail(f"JavaScript handler {action_id}")
        if action["kind"] == "http" and action["route"] not in server:
            fail(f"Rust route {action['route']}")
    passed(f"UI contract ({len(expected_actions)} actions)")

    scripts = re.findall(r"<script>(.*?)</script>", html, re.DOTALL)
    if len(scripts) != 1:
        fail("single inline browser script")
    scratch = ROOT / "state" / "aria-static-ui.js"
    scratch.parent.mkdir(parents=True, exist_ok=True)
    scratch.write_text(scripts[0], encoding="utf-8")
    node = shutil.which("node")
    if node:
        result = subprocess.run([node, "--check", str(scratch)], check=False)
        scratch.unlink(missing_ok=True)
        if result.returncode != 0:
            fail("browser JavaScript parser")
        passed("browser JavaScript parser")
    else:
        scratch.unlink(missing_ok=True)
        print("├─ ◈  browser JavaScript parser                       SKIP (node absent)")

    rust_files = sorted((ROOT / "src").glob("*.rs")) + sorted((ROOT / "tests").glob("*.rs"))
    for path in rust_files:
        scan_rust_delimiters(path)
    passed(f"Rust lexical structure ({len(rust_files)} files)")

    source = "\n".join(path.read_text(encoding="utf-8") for path in rust_files)
    forbidden = ["todo!", "unimplemented!", "panic!(\"TODO", "usize::from(args.get"]
    for marker in forbidden:
        if marker in source:
            fail(f"forbidden incomplete marker {marker}")
    passed("incomplete-code markers")

    print("└─ ◆  STATIC VERIFICATION COMPLETE                    PASS")
    print("    Run scripts\\ARIA-Verify.ps1 for cargo compile, tests, release build, and live HTTP smoke.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
