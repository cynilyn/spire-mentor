"""``generate_loadout``: the random start of ``--random-loadout`` runs."""

from __future__ import annotations

import pytest
from parity_fuzz_scenario import ALL_POTIONS, ALL_RELICS, _v2_card_pool, generate_loadout


def test_same_seed_same_loadout() -> None:
    assert generate_loadout("1000", "Silent") == generate_loadout("1000", "Silent")


def test_seed_and_character_both_vary_it() -> None:
    base = generate_loadout("1000", "Silent")
    assert generate_loadout("1001", "Silent") != base
    assert generate_loadout("1000", "Defect") != base


@pytest.mark.parametrize("character", ["Ironclad", "Silent", "Defect", "Regent", "Necrobinder"])
def test_loadout_draws_from_the_v2_pools(character: str) -> None:
    pool = set(_v2_card_pool(character))
    for seed in range(50):
        lo = generate_loadout(str(seed), character)
        assert {c.removesuffix("+1") for c in lo.deck} <= pool
        assert set(lo.relics) <= set(ALL_RELICS)
        assert len(set(lo.relics)) == len(lo.relics)
        assert set(lo.potions) <= set(ALL_POTIONS)
        assert 0 <= lo.gold <= 999
