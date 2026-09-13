"""``synergy.gen_card_text`` must describe the engine-pinned build.

The catalog page reads the checked-in ``web/public/card_text.json``. That file
was generated from v0.103.2 while the engine runs v0.107.1, so 57 cards carried
the wrong build's rules text — Pact's End claimed it "Can only be played if you
have 3 or more cards in your Exhaust Pile", a v0.103.2 ``IsPlayable`` gate that
v0.107.1 replaced with an on-play check. Bug report ``wb-66e36e372eef4bea``.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from synergy import gen_card_text
from workbench.localization import _ENGINE_WIKI_VERSION

_WIKI_DATA = Path(__file__).resolve().parents[2] / "sts2-wiki" / "data"
_SHIPPED = Path(__file__).resolve().parents[1] / "web" / "public" / "card_text.json"


def _write(path: Path, cards: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(cards), encoding="utf-8")


def test_pinned_build_overrides_older_manifests(tmp_path: Path):
    """Where two builds describe the same card, the pinned build wins — even
    though it does not sort last (the decoy version is numerically higher)."""
    _write(tmp_path / "v0.1.0" / "cards.json", [{"loc_key": "BASH", "description_plain": "old"}])
    _write(
        tmp_path / _ENGINE_WIKI_VERSION / "cards.json",
        [{"loc_key": "BASH", "title": "Bash", "description_plain": "pinned"}],
    )
    _write(tmp_path / "v0.999.0" / "cards.json", [{"loc_key": "BASH", "description_plain": "next"}])

    out = tmp_path / "card_text.json"
    gen_card_text.generate(tmp_path, out)

    assert json.loads(out.read_text(encoding="utf-8"))["BASH"]["description"] == "pinned"


def test_cards_the_pinned_build_dropped_survive(tmp_path: Path):
    """An older build is still the only source for a retired card, so the
    catalog can name FOLLOW_THROUGH without mis-describing a live card."""
    _write(
        tmp_path / "v0.1.0" / "cards.json",
        [{"loc_key": "FOLLOW_THROUGH", "title": "Follow Through", "description_plain": "gone"}],
    )
    _write(tmp_path / _ENGINE_WIKI_VERSION / "cards.json", [{"loc_key": "BASH", "title": "Bash"}])

    out = tmp_path / "card_text.json"
    gen_card_text.generate(tmp_path, out)

    result = json.loads(out.read_text(encoding="utf-8"))
    assert result["FOLLOW_THROUGH"]["title"] == "Follow Through"
    assert result["BASH"]["title"] == "Bash"


def test_markup_tags_are_stripped(tmp_path: Path):
    _write(
        tmp_path / _ENGINE_WIKI_VERSION / "cards.json",
        [{"loc_key": "BASH", "description_plain": "Apply [gold]Vulnerable[/gold]."}],
    )
    out = tmp_path / "card_text.json"
    gen_card_text.generate(tmp_path, out)

    assert json.loads(out.read_text(encoding="utf-8"))["BASH"]["description"] == "Apply Vulnerable."


@pytest.mark.skipif(not _WIKI_DATA.is_dir(), reason="sibling sts2-wiki checkout absent")
def test_shipped_card_text_matches_a_fresh_regeneration(tmp_path: Path):
    """The checked-in file must be what the generator produces today. Catches a
    wiki-data bump landing without a regenerated ``card_text.json``."""
    fresh = tmp_path / "card_text.json"
    gen_card_text.generate(_WIKI_DATA, fresh)

    expected = json.loads(fresh.read_text(encoding="utf-8"))
    shipped = json.loads(_SHIPPED.read_text(encoding="utf-8"))

    stale = sorted(k for k in expected if shipped.get(k) != expected[k])
    assert not stale, (
        f"{len(stale)} entries in web/public/card_text.json are stale "
        f"(e.g. {stale[:5]}). Regenerate: "
        "PYTHONPATH=python uv run python python/synergy/gen_card_text.py"
    )
    assert shipped["PACTS_END"]["description"].startswith("If you have")


def test_catalog_prints_the_keyword_lines_the_card_face_prints(tmp_path: Path):
    """A card's keywords are part of its rules text — the game renders them as
    their own lines above or below the description. ``description_plain`` never
    carries them, so the catalog described Tremble as "Apply 3 Vulnerable." with
    no "Exhaust."."""
    _write(
        tmp_path / _ENGINE_WIKI_VERSION / "cards.json",
        [
            {
                "loc_key": "TREMBLE",
                "title": "Tremble",
                "description_plain": "Apply 3 Vulnerable.",
                "keywords": ["Exhaust"],
            },
            {
                "loc_key": "ASCENDERS_BANE",
                "title": "Ascender's Bane",
                "keywords": ["Eternal", "Unplayable", "Ethereal"],
            },
        ],
    )
    out = tmp_path / "card_text.json"
    gen_card_text.generate(tmp_path, out)
    text = json.loads(out.read_text())

    assert text["TREMBLE"]["description"] == "Apply 3 Vulnerable.\nExhaust."
    # Before-lines print in REVERSE declaration order, and a card with no
    # description of its own is nothing but its keyword lines.
    assert text["ASCENDERS_BANE"]["description"] == "Unplayable.\nEthereal.\nEternal."


def test_shipped_card_text_carries_keyword_lines():
    """Guards the shipped artifact, not just the generator."""
    if not _SHIPPED.is_file():
        pytest.skip("card_text.json not present")
    # encoding= is not optional: the shipped artifact is UTF-8 and carries
    # bytes cp1252 cannot decode, so a default read fails on Windows only.
    text = json.loads(_SHIPPED.read_text(encoding="utf-8"))
    assert text["TREMBLE"]["description"].endswith("\nExhaust.")
    # A card that Exhausts OTHER cards must NOT claim the keyword itself.
    assert "Exhaust." not in text["BURNING_PACT"]["description"].splitlines()
