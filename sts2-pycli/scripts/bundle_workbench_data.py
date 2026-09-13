#!/usr/bin/env python3
"""Bundle the workbench's runtime data for the Rust ``sts2-workbench`` crate.

The Python workbench (``python/workbench/localization.py`` / ``assets.py`` and
``server.create_app``) reads its display data from three places at runtime:
the vendored ``localization_eng/*.json`` dumps, the sibling wiki's
``sts2-wiki/data/v0.107.1/`` manifests, and the art manifest that
``scripts/build_asset_manifest.py`` derives from the extracted art tree. The
Rust port runs in the browser (wasm) with no filesystem, so it ``include_str!``s
committed copies instead. This script produces those copies under
``sts2-rs/crates/sts2-workbench/data/`` — self-contained (no include path
leaves ``sts2-rs``), deterministic (sorted keys, fixed indent, sorted globs)
and minimal (the wiki manifests are trimmed to exactly the fields the Python
loaders read).

Regenerate with ``just workbench-data``; ``just check-workbench-data`` fails
on drift between the sources and the committed bundle.

Usage:
    python scripts/bundle_workbench_data.py [-o OUT_DIR]

Stdlib only — it is a build step, not part of the package.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path
from typing import Any

_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parent
_MONO_ROOT = _REPO_ROOT.parent

# Keep in lockstep with ``workbench.localization._ENGINE_WIKI_VERSION``.
_ENGINE_WIKI_VERSION = "v0.107.1"

_LOC_DIR = _REPO_ROOT / "localization_eng"
_WIKI_DATA = _MONO_ROOT / "sts2-wiki" / "data" / _ENGINE_WIKI_VERSION
_IMAGES = _MONO_ROOT / "sts2-wiki" / "site" / "public" / "images"
_AUDIO = _MONO_ROOT / "sts2-wiki" / "site" / "public" / "audio"
_DEFAULT_OUT = _MONO_ROOT / "sts2-rs" / "crates" / "sts2-workbench" / "data"

# ``localization_eng`` files copied VERBATIM (byte-for-byte). Every one is a
# flat ``{"KEY.field": "string"}`` dict; the Rust side parses them as
# ``BTreeMap<String, String>`` (``powers.json`` additionally in file order).
_LOC_FILES = (
    "BUILD.txt",
    "afflictions.json",
    "card_keywords.json",
    "cards.json",
    "characters.json",
    "events.json",
    "monsters.json",
    "orbs.json",
    "potions.json",
    "powers.json",
    "relics.json",
    "rest_site_ui.json",
)

# The wiki card fields ``localization.load_card_localization`` /
# ``load_card_meta`` read. Nested ``upgrades`` / ``vars`` entries are trimmed
# to their read keys below.
_CARD_FIELDS = (
    "loc_key",
    "title",
    "character",
    "star_cost",
    "x_star_cost",
    "description_template",
    "description_plain",
    "upgraded_description_plain",
    "keywords",
    "type",
    "rarity",
    "energy_cost",
    "upgraded_cost",
)

# The wiki monster fields ``localization.load_monster_hp_ranges`` reads.
_MONSTER_FIELDS = ("class_name", "min_hp", "max_hp", "min_hp_base", "max_hp_base", "hp_ascension")


def _is_int(value: Any) -> bool:
    """``isinstance(x, int)`` minus ``bool`` — the loaders' integer test."""
    return isinstance(value, int) and not isinstance(value, bool)


def _dumps(obj: Any) -> str:
    return json.dumps(obj, indent=1, sort_keys=True, ensure_ascii=False) + "\n"


def _trim_card(card: dict[str, Any]) -> dict[str, Any] | None:
    """Keep only what the Python loaders read, dropping values they ignore.

    A missing key and a falsy value are the same thing to ``load_card_
    localization`` (``if c.get("title")``), EXCEPT for numbers — ``energy_cost
    == 0`` and ``star_cost == 0`` are real values (``is not None`` tests) — so
    ``0`` is kept while ``None`` / ``""`` / ``[]`` / ``False`` are dropped.
    """
    key = card.get("loc_key")
    if not key:
        return None
    out: dict[str, Any] = {}
    for field in _CARD_FIELDS:
        value = card.get(field)
        if value is None or value == "" or value == [] or value is False:
            continue
        if field in ("energy_cost", "upgraded_cost", "star_cost"):
            if not _is_int(value):
                raise SystemExit(f"{key}: {field} is {value!r}, expected an int")
        elif field == "x_star_cost":
            if value is not True:
                raise SystemExit(f"{key}: x_star_cost is {value!r}, expected a bool")
        elif field == "keywords":
            if not all(isinstance(k, str) for k in value):
                raise SystemExit(f"{key}: keywords must be strings: {value!r}")
        elif not isinstance(value, str):
            raise SystemExit(f"{key}: {field} is {value!r}, expected a string")
        out[field] = value
    # Keyword-only upgrade deltas (``add_keyword`` / ``remove_keyword``).
    deltas = []
    for up in card.get("upgrades") or []:
        keyword = up.get("keyword")
        if not keyword:
            continue
        entry = {"keyword": keyword}
        if up.get("action") is not None:
            entry["action"] = up["action"]
        deltas.append(entry)
    if deltas:
        out["upgrades"] = deltas
    # Per-variable base / upgraded values.
    var_entries = []
    for var in card.get("vars") or []:
        if not var.get("type"):
            continue
        entry: dict[str, Any] = {"type": var["type"]}
        for vfield in ("base_value", "upgraded_value"):
            if var.get(vfield) is not None:
                if not _is_int(var[vfield]):
                    raise SystemExit(f"{key}: var {var['type']} {vfield} is not an int")
                entry[vfield] = var[vfield]
        var_entries.append(entry)
    if var_entries:
        out["vars"] = var_entries
    return out


def _trim_monster(row: dict[str, Any]) -> dict[str, Any] | None:
    kind = row.get("class_name")
    if not isinstance(kind, str):
        return None
    out: dict[str, Any] = {"class_name": kind}
    for field in _MONSTER_FIELDS[1:]:
        if _is_int(row.get(field)):
            out[field] = row[field]
    return out


def _owned_items(kind: str) -> list[dict[str, str]]:
    """``{class_name, description}`` per relic / potion, by class name.

    Read from the regex-extracted aggregate (``relics.json`` / ``potions.json``,
    ``just extract`` in sts2-wiki), whose ``description_resolved`` has every
    DynamicVar substituted — the same text the LLM-extracted per-item files
    carried, minus the LLM. Sorted by class name, which is the file-name order
    the per-item loader used, so the Rust ``load_owned_localization`` twin
    keeps its first-wins semantics.
    """
    rows: Any = json.loads((_WIKI_DATA / f"{kind}.json").read_text(encoding="utf-8"))
    if not isinstance(rows, list):
        raise SystemExit(f"{kind}.json: expected a list")
    out = []
    for item in sorted(
        (r for r in rows if isinstance(r, dict)), key=lambda r: r.get("class_name", "")
    ):
        class_name = item.get("class_name")
        desc = item.get("description_resolved")
        if not class_name or not desc:
            continue
        out.append({"class_name": class_name, "description": desc})
    return out


def _check_loc_file(path: Path) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise SystemExit(f"{path}: expected a flat JSON object")
    bad = {k: v for k, v in data.items() if not isinstance(v, str)}
    if bad:
        raise SystemExit(f"{path}: non-string localization values: {sorted(bad)[:5]}")


def build(out_dir: Path) -> dict[str, int]:
    """Write every bundle file under ``out_dir``; return ``{name: bytes}``."""
    sys.path.insert(0, str(_HERE))
    from build_asset_manifest import build_manifest

    for required in (_LOC_DIR, _WIKI_DATA, _IMAGES):
        if not required.exists():
            raise SystemExit(f"error: source not found: {required}")

    # Game-derived files go under ``game/`` (never published; ``build.rs``
    # stubs them when absent); only the curated ``card_tags.json`` sits at the
    # top level and is committed everywhere.
    game_out = out_dir / "game"
    loc_out = game_out / "localization_eng"
    loc_out.mkdir(parents=True, exist_ok=True)
    sizes: dict[str, int] = {}

    for name in _LOC_FILES:
        src = _LOC_DIR / name
        if name.endswith(".json"):
            _check_loc_file(src)
        dst = loc_out / name
        shutil.copyfile(src, dst)
        sizes[f"game/localization_eng/{name}"] = dst.stat().st_size

    cards: Any = json.loads((_WIKI_DATA / "cards.json").read_text(encoding="utf-8"))
    if isinstance(cards, dict):
        cards = cards.get("cards", [])
    trimmed_cards = [t for t in (_trim_card(c) for c in cards or []) if t is not None]

    monsters: Any = json.loads((_WIKI_DATA / "monsters.json").read_text(encoding="utf-8"))
    if not isinstance(monsters, list):
        raise SystemExit("monsters.json: expected a list")
    trimmed_monsters = [
        t for t in (_trim_monster(r) for r in monsters if isinstance(r, dict)) if t is not None
    ]

    owned = _owned_items("relics") + _owned_items("potions")

    manifest = build_manifest(_IMAGES, _AUDIO)

    scales_src = _IMAGES / "creature_scales.json"
    if not scales_src.is_file():
        raise SystemExit(f"error: {scales_src} not found")
    scales = json.loads(scales_src.read_text(encoding="utf-8"))

    # The deck analyzer's two tables (`workbench/deck_analysis.py`), copied
    # as-is: the Rust twin reads the same keys off the same JSON.
    for name, dst_dir in (("card_db.json", game_out), ("card_tags.json", out_dir)):
        src = _REPO_ROOT / "python" / "workbench" / "data" / name
        if not src.is_file():
            raise SystemExit(f"error: {src} not found")
        shutil.copyfile(src, dst_dir / name)
        sizes[str((dst_dir / name).relative_to(out_dir))] = (dst_dir / name).stat().st_size

    for name, obj in (
        ("wiki_cards.json", trimmed_cards),
        ("wiki_monsters.json", trimmed_monsters),
        ("wiki_owned.json", owned),
        ("asset_manifest.json", manifest),
        ("creature_scales.json", scales),
    ):
        path = game_out / name
        path.write_text(_dumps(obj), encoding="utf-8")
        sizes[f"game/{name}"] = path.stat().st_size
    return sizes


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-o",
        "--out-dir",
        default=str(_DEFAULT_OUT),
        help="Output directory (default: sts2-rs/crates/sts2-workbench/data).",
    )
    args = parser.parse_args(argv)
    sizes = build(Path(args.out_dir))
    total = 0
    for name, size in sorted(sizes.items()):
        print(f"{size:>8}  {name}")
        total += size
    print(f"{total:>8}  total ({total / 1024:.0f} KB) -> {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
