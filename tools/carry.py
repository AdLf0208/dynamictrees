#!/usr/bin/env python3
"""Carry the oak family out of the Dynamic Trees jar, and prove it.

Every file this package carries is listed in ``carry/MANIFEST.tsv`` with where it came
from. Three provenances exist:

* ``jar:<entry>``     the packaged bytes are byte-identical to that entry in
  ``var/dynamictrees/dynamictrees.jar`` (a texture, a blockstate, a loot table);
* ``vanilla:<path>``  the packaged bytes are byte-identical to a file in the prepared
  26.3 resources (never used here — the port has no vanilla copies — but kept so a
  later carry can say so honestly);
* ``derived:<entry>`` the file was written for this port from that jar entry (or ``-``
  for one with no jar source), and ``tools/carry.py`` holds the note saying what changed
  and why.

``--check`` re-reads the jar and the prepared resources and verifies all of it, plus
that no file under ``assets/`` or ``data/`` is missing from the manifest and that the
jar's own licence is still the MIT the notice claims. Exit 0 only when every check
passes.

    python3 tools/carry.py --write   # regenerate the manifest from the tree
    python3 tools/carry.py --check   # verify the tree against it
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import zipfile
from pathlib import Path

PORT = Path(__file__).resolve().parents[1]
ROOT = PORT.parents[1]
JAR = ROOT / "var" / "dynamictrees" / "dynamictrees.jar"
RES = Path("/home/adlf/Desktop/horizon/.local/horizon-client/26.3")
MANIFEST = PORT / "carry" / "MANIFEST.tsv"
CARRIED_ROOTS = ("assets", "data")

# Every derived file, with the jar entry it was modelled on and what changed.
DERIVED: dict[str, tuple[str, str]] = {
    "assets/dynamictrees/blockstates/oak_sapling.json": (
        "assets/dynamictrees/blockstates/oak_sapling.json",
        "same single-variant shape; the model path is this port's own sapling model",
    ),
    "assets/dynamictrees/blockstates/oak_branch.json": (
        "assets/dynamictrees/blockstates/oak_branch.json",
        "same shape; the model is this port's log-shaped branch model",
    ),
    "assets/dynamictrees/models/block/saplings/oak.json": (
        "assets/dynamictrees/models/block/saplings/oak.json",
        "upstream's dynamictrees:smartmodel/sapling is not portable; a vanilla cross model "
        "with the oak sapling texture",
    ),
    "assets/dynamictrees/models/block/oak_branch.json": (
        "assets/dynamictrees/models/block/oak_branch.json",
        "upstream's dynamictrees:branch model loader is not portable; vanilla's oak log model",
    ),
    "assets/dynamictrees/items/oak_seed.json": (
        "-",
        "26.3 item definition (assets/<ns>/items/) naming this port's item model",
    ),
    "assets/dynamictrees/models/item/oak_seed.json": (
        "assets/dynamictrees/models/item/oak_seed.json",
        "upstream parents dynamictrees:item/standard_seed, which is item/generated; inlined",
    ),
    "assets/dynamictrees/lang/en_us.json": (
        "assets/dynamictrees/lang/en_us.json",
        "only the oak family's keys; the sapling and leaves names are added (the jar has none)",
    ),
    "data/dynamictrees/horizon/block/oak_sapling.json": (
        "-",
        "content declaration: template minecraft:poppy, random_ticks true, no item",
    ),
    "data/dynamictrees/horizon/block/oak_branch.json": (
        "-",
        "content declaration: template minecraft:oak_planks (stateless solid), no item",
    ),
    "data/dynamictrees/horizon/block/oak_leaves.json": (
        "-",
        "content declaration: template minecraft:oak_leaves, no item",
    ),
    "data/dynamictrees/horizon/item/oak_seed.json": (
        "-",
        "content declaration: template minecraft:stick (a plain 64-stack item, no use)",
    ),
    "data/dynamictrees/horizon/creative_tab/dynamictrees.json": (
        "-",
        "the jar's itemGroup.dynamictrees tab, with the one item this slice carries",
    ),
    "data/dynamictrees/loot_table/blocks/oak_sapling.json": (
        "data/dynamictrees/loot_table/blocks/oak_leaves.json",
        "upstream drops the species' seed (DynamicSaplingBlock.getDrops); 26.3 shape",
    ),
    "data/dynamictrees/loot_table/blocks/oak_branch.json": (
        "data/dynamictrees/loot_table/trees/branches/oak.json",
        "upstream's multiply_logs_count/multiply_sticks_count are its own code; one oak log "
        "plus 1-2 sticks, 26.3 shape",
    ),
    "data/dynamictrees/loot_table/blocks/oak_leaves.json": (
        "data/dynamictrees/loot_table/blocks/oak_leaves.json",
        "upstream's seed_item entry type and seasonal_seed_drop_chance condition are its own "
        "code; oak_seed at the same table_bonus chances, sticks as upstream",
    ),
    "data/dynamictrees/tags/block/branches.json": (
        "data/dynamictrees/tags/block/branches.json",
        "only the branches_that_burn branch (no fungus branches in this slice)",
    ),
    "data/dynamictrees/tags/block/branches_that_burn.json": (
        "data/dynamictrees/tags/block/branches_that_burn.json",
        "the oak branch alone",
    ),
    "data/dynamictrees/tags/block/leaves.json": (
        "data/dynamictrees/tags/block/leaves.json",
        "the oak leaves alone",
    ),
    "data/dynamictrees/tags/block/saplings.json": (
        "data/dynamictrees/tags/block/saplings.json",
        "the oak sapling alone",
    ),
    "data/dynamictrees/tags/item/seeds.json": (
        "data/dynamictrees/tags/item/seeds.json",
        "the oak seed alone",
    ),
    "data/minecraft/tags/block/logs.json": (
        "data/dynamictrees/tags/block/branches.json",
        "upstream adds #dynamictrees:branches to minecraft:logs; the oak branch keeps leaves "
        "alive (prevents_nearby_leaf_decay is #minecraft:logs)",
    ),
    "data/minecraft/tags/block/logs_that_burn.json": (
        "data/dynamictrees/tags/block/branches_that_burn.json",
        "upstream adds #dynamictrees:branches_that_burn to minecraft:logs_that_burn",
    ),
    "data/minecraft/tags/block/leaves.json": (
        "data/dynamictrees/tags/block/leaves.json",
        "upstream adds #dynamictrees:leaves to minecraft:leaves",
    ),
    "data/minecraft/tags/block/saplings.json": (
        "data/dynamictrees/tags/block/saplings.json",
        "upstream adds #dynamictrees:saplings to minecraft:saplings",
    ),
    "data/minecraft/tags/item/saplings.json": (
        "data/dynamictrees/tags/item/seeds.json",
        "upstream adds #dynamictrees:seeds to minecraft:saplings (item tag)",
    ),
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def jar_entries(jar: zipfile.ZipFile) -> dict[str, bytes]:
    return {name: jar.read(name) for name in jar.namelist() if not name.endswith("/")}


def carried_files() -> list[str]:
    files = []
    for root in CARRIED_ROOTS:
        for path in sorted((PORT / root).rglob("*")):
            if path.is_file():
                files.append(path.relative_to(PORT).as_posix())
    return files


def provenance(path: str, jar: dict[str, bytes]) -> tuple[str, str]:
    """(source, note) for one packaged file."""
    data = (PORT / path).read_bytes()
    if path in jar and jar[path] == data:
        return f"jar:{path}", "carried byte-identical"
    if path in DERIVED:
        source, note = DERIVED[path]
        return f"derived:{source}", note
    return "unprovenanced:-", ""


def write_manifest() -> int:
    with zipfile.ZipFile(JAR) as jar_file:
        jar = jar_entries(jar_file)
    rows = ["path\tsource\tnote"]
    bad = 0
    for path in carried_files():
        source, note = provenance(path, jar)
        if source.startswith("unprovenanced"):
            print(f"FAIL: {path} has no provenance; add it to DERIVED", file=sys.stderr)
            bad += 1
        rows.append(f"{path}\t{source}\t{note}")
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text("\n".join(rows) + "\n")
    print(f"wrote {MANIFEST.relative_to(PORT)}: {len(rows) - 1} files")
    return 1 if bad else 0


def check() -> int:
    if not JAR.is_file():
        print(f"FAIL: missing jar {JAR}", file=sys.stderr)
        return 1
    if not MANIFEST.is_file():
        print(f"FAIL: missing {MANIFEST.relative_to(PORT)}; run --write", file=sys.stderr)
        return 1
    with zipfile.ZipFile(JAR) as jar_file:
        jar = jar_entries(jar_file)

    failures = 0

    def say(good: bool, label: str, detail: str = "") -> None:
        nonlocal failures
        if not good:
            failures += 1
        print(f"  {'ok  ' if good else 'FAIL'} {label}{('  ' + detail) if detail else ''}")

    # The licence the notice claims is still the jar's own.
    toml = jar.get("META-INF/neoforge.mods.toml", b"").decode("utf-8", "replace")
    say('license = "MIT"' in toml, "licence", "META-INF/neoforge.mods.toml says MIT")

    rows = [line for line in MANIFEST.read_text().splitlines() if line.strip()]
    say(rows and rows[0] == "path\tsource\tnote", "manifest header")
    listed: set[str] = set()
    for row in rows[1:]:
        path, source, _note = row.split("\t")
        listed.add(path)
        packaged = PORT / path
        if not packaged.is_file():
            say(False, path, "listed but missing")
            continue
        data = packaged.read_bytes()
        if source.startswith("jar:"):
            entry = source.removeprefix("jar:")
            say(jar.get(entry) == data, path, f"byte-identical to {entry}")
        elif source.startswith("vanilla:"):
            reference = RES / source.removeprefix("vanilla:")
            say(reference.is_file() and reference.read_bytes() == data, path, source)
        elif source.startswith("derived:"):
            origin = source.removeprefix("derived:")
            exists = origin == "-" or origin in jar
            say(exists, path, f"derived from {origin}")
            if path.endswith(".json"):
                try:
                    json.loads(data)
                except json.JSONDecodeError as error:
                    say(False, path, f"invalid JSON: {error}")
        else:
            say(False, path, f"unknown provenance {source!r}")

    for path in carried_files():
        say(path in listed, path, "in the manifest")
    for path in sorted(DERIVED):
        say((PORT / path).is_file(), path, "derived file present")

    print(f"\n{'PASS' if failures == 0 else 'FAIL'}: carry check, {len(listed)} files, {failures} failure(s)")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true", help="regenerate the manifest")
    group.add_argument("--check", action="store_true", help="verify the tree")
    args = parser.parse_args()
    return write_manifest() if args.write else check()


if __name__ == "__main__":
    raise SystemExit(main())
