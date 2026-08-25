#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

TARGETS = {"parse_module", "parse_validate"}
EXPECTATIONS = {"valid", "validation-error", "parse-error"}
ID_RE = re.compile(r"^[a-z0-9][a-z0-9_-]*$")


def fail(message: str) -> None:
    raise SystemExit(message)


def decode_hex(seed_id: str, raw: str) -> bytes:
    compact = "".join(raw.split())
    if len(compact) % 2:
        fail(f"seed {seed_id}: hex payload has odd length")
    try:
        return bytes.fromhex(compact)
    except ValueError as error:
        fail(f"seed {seed_id}: invalid hex payload: {error}")


def load_rows(manifest: Path) -> list[tuple[str, set[str], str, bytes]]:
    lines = manifest.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "id\ttargets\texpectation\thex\tnote":
        fail(f"{manifest}: unexpected or missing manifest header")

    rows: list[tuple[str, set[str], str, bytes]] = []
    ids: set[str] = set()
    payloads: dict[bytes, str] = {}
    for line_number, line in enumerate(lines[1:], start=2):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 5:
            fail(f"{manifest}:{line_number}: expected exactly five tab-separated fields")
        seed_id, targets_raw, expectation, raw_hex, _note = fields
        if not ID_RE.fullmatch(seed_id):
            fail(f"{manifest}:{line_number}: invalid seed id {seed_id!r}")
        if seed_id in ids:
            fail(f"{manifest}:{line_number}: duplicate seed id {seed_id!r}")
        ids.add(seed_id)

        targets = set(targets_raw.split(","))
        if not targets or not targets <= TARGETS:
            fail(f"{manifest}:{line_number}: invalid target set {targets_raw!r}")
        if expectation not in EXPECTATIONS:
            fail(f"{manifest}:{line_number}: invalid expectation {expectation!r}")

        payload = decode_hex(seed_id, raw_hex)
        if payload in payloads:
            fail(
                f"{manifest}:{line_number}: duplicate payload shared by "
                f"{payloads[payload]!r} and {seed_id!r}"
            )
        payloads[payload] = seed_id
        rows.append((seed_id, targets, expectation, payload))

    if not rows:
        fail(f"{manifest}: no seed rows")
    return rows


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in TARGETS:
        fail("usage: python3 fuzz/materialize-seeds.py <parse_module|parse_validate>")
    target = sys.argv[1]

    fuzz_dir = Path(__file__).resolve().parent
    manifest = fuzz_dir / "seeds" / "manifest.tsv"
    corpus_dir = fuzz_dir / "corpus" / target
    corpus_dir.mkdir(parents=True, exist_ok=True)

    existing: dict[str, Path] = {}
    for path in sorted(corpus_dir.iterdir()):
        if not path.is_file():
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        existing.setdefault(digest, path)

    selected = 0
    added = 0
    for seed_id, targets, _expectation, payload in load_rows(manifest):
        if target not in targets:
            continue
        selected += 1
        digest = hashlib.sha256(payload).hexdigest()
        if digest in existing:
            print(f"present\t{seed_id}\t{digest}\t{len(payload)}\t{existing[digest]}")
            continue

        destination = corpus_dir / f"seed-{seed_id}-{digest[:12]}"
        if destination.exists() and destination.read_bytes() != payload:
            fail(f"refusing to overwrite non-matching corpus entry {destination}")
        destination.write_bytes(payload)
        existing[digest] = destination
        added += 1
        print(f"added\t{seed_id}\t{digest}\t{len(payload)}\t{destination}")

    if selected == 0:
        fail(f"no reviewed seeds selected for target {target}")
    print(f"summary\ttarget={target}\tselected={selected}\tadded={added}\tcorpus={corpus_dir}")


if __name__ == "__main__":
    main()
