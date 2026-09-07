#!/usr/bin/env python3
"""
Deterministically generate `docs/EVENTS.md` from the source of truth.

The source of truth for the event catalog is the `#[contractevent]` struct
definitions in:

    contracts/raffle-shared/src/events.rs
    contracts/raffle-factory/src/events.rs
    contracts/raffle-instance/src/events.rs

Every event struct and every field carries a `///` doc comment.  This script:

* parses those structs (plus the `///` docs),
* resolves which contract functions actually emit each event (by scanning the
  crate sources for `<EventName> { ... }.publish(...)` call sites),
* writes the generated markdown to `docs/EVENTS.md`.

The output is deterministic: for a fixed repository state the generated file
is byte-identical every run, so it can be diffed in CI.

Usage:
    python scripts/generate_event_docs.py
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS = REPO_ROOT / "docs"

# Crate metadata: (src dir relative to repo root, markdown section title).
CRATES = [
    ("contracts/raffle-factory/src", "Factory Contract Events"),
    ("contracts/raffle-instance/src", "Instance Contract Events"),
    ("contracts/raffle-shared/src", "Shared Events"),
]

# Shared events are re-exported (not defined) in these crates; their emitters
# live there too.
SHARED_EMITTER_SRC = [
    "contracts/raffle-factory/src",
    "contracts/raffle-instance/src",
]

# Files that are never scanned for emitters.
SKIP_FILES = {"events.rs", "test.rs"}

STRUCT_RE = re.compile(
    r"^(///[^\n]*\n)+(?:\s*#[^\n]*\n)*pub struct (\w+)\s*\{(.*?)^\}",
    re.MULTILINE | re.DOTALL,
)
FIELD_RE = re.compile(r"^\s*pub (\w+): ([^,]+),$")
DOC_RE = re.compile(r"^\s*/// ?(.*)$")
FUNC_RE = re.compile(r"\bfn\s+(\w+)\s*\(")


def parse_structs(source: str):
    """Parse `pub struct` event definitions with their doc comments.

    Returns a list of dicts:
        {"name", "doc": [str], "fields": [{"name", "type", "doc", "topic"}]}
    """
    events = []
    for match in STRUCT_RE.finditer(source):
        name = match.group(2)
        # Event-level doc comment: the leading `///` lines of the matched
        # block, stopping at the first non-doc line (attributes / struct).
        doc = []
        for line in match.group(0).splitlines():
            doc_m = DOC_RE.match(line)
            if doc_m:
                doc.append(doc_m.group(1).strip())
            elif doc:
                break
        fields = []
        cur_doc = []
        for line in match.group(3).splitlines():
            doc_m = DOC_RE.match(line)
            if doc_m:
                cur_doc.append(doc_m.group(1).strip())
                continue
            fld_m = FIELD_RE.match(line)
            if fld_m:
                fields.append(
                    {
                        "name": fld_m.group(1),
                        "type": fld_m.group(2).strip(),
                        "doc": cur_doc[:],
                        "topic": False,
                    }
                )
                cur_doc = []
        events.append({"name": name, "doc": doc, "fields": fields})
    return events


def collect_field_topics(source: str):
    """Return {struct_name: set(field_name, ...)} annotated with `#[topic]`."""
    result = {}
    cur_struct = None
    lines = source.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        struct_m = re.match(r"^pub struct (\w+) \{", line.strip())
        if struct_m:
            cur_struct = struct_m.group(1)
            result.setdefault(cur_struct, set())
            i += 1
            continue
        if line.strip() == "}":
            cur_struct = None
            i += 1
            continue
        if cur_struct is not None and re.match(r"^#\[topic\]$", line.strip()):
            next_line = lines[i + 1] if i + 1 < len(lines) else ""
            fld_m = re.match(r"^pub (\w+):", next_line.strip())
            if fld_m:
                result[cur_struct].add(fld_m.group(1))
        i += 1
    return result


def find_functions(source: str):
    """Return list of (name, body_start, body_end_inclusive) via brace matching."""
    functions = []
    for m in FUNC_RE.finditer(source):
        name = m.group(1)
        # Match the parameter-list parens (handles nested generics/tuples).
        depth = 0
        pos = m.end() - 1
        while pos < len(source):
            ch = source[pos]
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    break
            pos += 1
        brace_pos = source.find("{", pos)
        if brace_pos == -1:
            continue
        depth = 0
        i = brace_pos
        while i < len(source):
            ch = source[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        functions.append((name, brace_pos, i + 1))
    return functions


def find_emitters(src_dir: Path, event_name: str):
    """Return a sorted list of function names that publish `event_name`."""
    emitters = set()
    for rs in sorted(src_dir.glob("*.rs")):
        if rs.name in SKIP_FILES:
            continue
        source = rs.read_text(encoding="utf-8")
        functions = find_functions(source)
        pattern = re.compile(r"\b" + re.escape(event_name) + r"\s*\{")
        for lit in pattern.finditer(source):
            # Brace-match the literal, then require a `.publish` call nearby.
            depth = 1
            j = lit.end()
            while j < len(source) and depth > 0:
                if source[j] == "{":
                    depth += 1
                elif source[j] == "}":
                    depth -= 1
                j += 1
            if ".publish" not in source[j: j + 40]:
                continue
            for (name, start, end) in functions:
                if start <= lit.start() < end:
                    emitters.add(name)
                    break
    return sorted(emitters)


def md_table(fields):
    lines = [
        "| Field | Type | Flags | Description |",
        "|-------|------|-------|-------------|",
    ]
    for f in fields:
        flags = "topic" if f["topic"] else ""
        doc = " ".join(f["doc"]).strip().replace("|", "\\|")
        lines.append(f"| `{f['name']}` | `{f['type']}` | {flags} | {doc} |")
    return "\n".join(lines)


def camel_to_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def build_header() -> str:
    return f"""# Raffle Contract Events

This document is **auto-generated** from the `#[contractevent]` struct
definitions in `contracts/*/src/events.rs`. **Do not edit by hand.**
Regenerate it whenever event structs or their field docs change:

```bash
python scripts/generate_event_docs.py
```

## Event Topic Scheme

All events use a two-symbol Soroban event topic:

```text
("tikka", "<event_topic>")
```

- First symbol: `"tikka"` (constant namespace).
- Second symbol: the event struct name in **snake_case** (e.g. `ticket_purchased`,
  `raffle_created`).

Fields marked `topic` in the tables below are part of the event topic rather
than the event body.

## Index-vs-ID convention

To avoid the drift that silently breaks indexers:

- `ticket_id` / `ticket_ids` / `ticket_number` are **1-based ticket IDs**.
- `winning_ticket_ids` are **0-based positions** within the ticket pool (the
  corresponding 1-based ticket ID is `winning_ticket_ids[i] + 1`).
- `*_index` fields are **0-based positions** into the array referenced by the
  field name.
- `*_id` / `*_count` / `round` fields state their base explicitly in the field
  docs.

---

"""


def main():
    sections = []
    for src_rel, title in CRATES:
        src_dir = REPO_ROOT / src_rel
        events_file = src_dir / "events.rs"
        if not events_file.exists():
            print(f"Error: expected {events_file} (relative to {REPO_ROOT})", file=sys.stderr)
            sys.exit(1)
        source = events_file.read_text(encoding="utf-8")
        events = parse_structs(source)
        topic_map = collect_field_topics(source)
        for ev in events:
            topics = topic_map.get(ev["name"], set())
            for f in ev["fields"]:
                f["topic"] = f["name"] in topics

        blocks = [f"# {title}", ""]
        if src_rel == "contracts/raffle-shared/src":
            intro = (
                "These events are defined once in "
                "`contracts/raffle-shared/src/events.rs` and re-exported by both "
                "the factory and the instance contracts. They are emitted with "
                "identical payloads from either contract."
            )
        else:
            intro = f"Defined in `{events_file.relative_to(REPO_ROOT)}`."
        blocks.append(intro)
        blocks.append("")

        for ev in sorted(events, key=lambda e: e["name"]):
            blocks.append(f"## {ev['name']}")
            blocks.append("")
            if ev["doc"]:
                blocks.append(" ".join(ev["doc"]).strip())
                blocks.append("")
            blocks.append(f"Topic: `tikka:{camel_to_snake(ev['name'])}`")
            blocks.append("")
            blocks.append(md_table(ev["fields"]))
            blocks.append("")
            if src_rel == "contracts/raffle-shared/src":
                emitters = []
                for other_rel in SHARED_EMITTER_SRC:
                    emitters = sorted(set(emitters) | set(find_emitters(REPO_ROOT / other_rel, ev["name"])))
            else:
                emitters = find_emitters(src_dir, ev["name"])
            if emitters:
                block = "**Emitted by:** `" + "`, `".join(emitters) + "`"
            else:
                block = "**Emitted by:** *(no live call sites — defined but not currently published)*"
            blocks.append(block)
            blocks.append("")
            blocks.append("---")
            blocks.append("")

        sections.append("\n".join(blocks))

    docs = build_header() + "\n".join(sections) + "\n"

    out = DOCS / "EVENTS.md"
    out.write_text(docs, encoding="utf-8", newline="\n")
    print(f"Wrote {out.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()