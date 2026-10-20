"""`RunManager.remove_deck_card_at` (exact-position deck edit) and `card_reward_kind` (reward-kind marker)."""

from __future__ import annotations

import sts2_rs


def make(seed: str = "P0A0") -> sts2_rs.RunManager:
    return sts2_rs.RunManager(
        string_seed=seed, character="Ironclad", ascension_level=0, neow_start=True, game_version="v0.111.0"
    )


def test_remove_deck_card_at_removes_exactly_that_position_and_keeps_the_order():
    rm = make()
    before = rm.master_deck_detailed()
    assert len(before) >= 6
    # Pick a position whose id also occurs elsewhere, so removal-by-id would be ambiguous.
    idx = next(i for i, (cid, _) in enumerate(before) if sum(1 for c, _ in before if c == cid) > 1 and i > 0)
    assert rm.remove_deck_card_at(idx) is True
    after = rm.master_deck_detailed()
    assert after == before[:idx] + before[idx + 1 :]


def test_remove_deck_card_at_rejects_an_out_of_range_position():
    rm = make()
    n = len(rm.master_deck_detailed())
    assert rm.remove_deck_card_at(n) is False
    assert len(rm.master_deck_detailed()) == n


def test_card_reward_kind_is_a_plain_card_offering_outside_a_special_reward():
    rm = make()
    assert rm.card_reward_kind == "card"
