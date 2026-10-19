"""`RunManager.rng_dump` contract (v0.111.0 seed-hash port, stage 1).

Shape, read-only guarantee and two oracle anchors. Oracle vectors: sts2_bot
``logs/engine/p0/stage1_vectors.json`` (sha256
5267538d84c1a1468b68656b82bdd270e4a4e21b60a251366beb475786aa8976, oracle f79d3a9, game 0.111.0).
The full 20-seed x 22-stream x 10-draw comparison lives in
``logs/engine/p0/tools/compare_vectors.py``.
"""

from __future__ import annotations

import pytest
import sts2_rs

# (seed string, run_seed, player_seed, first NextInt(0,100) of up_front/rewards on the initial state)
ANCHORS = [
    ("P0A0", 10883704133362413376, 10883704133362413376, 59),
    ("oldP0A0", 273692109, 15676685915450073139, None),
]

STREAMS_22 = {
    "up_front", "shuffle", "unknown_map_point", "combat_card_generation", "combat_potion_generation",
    "combat_card_selection", "combat_energy_costs", "combat_targets", "monster_ai", "niche", "combat_orbs",
    "treasure_room_relics", "rewards", "shops", "transformations", "act_selection", "act_1_map", "act_2_map",
    "act_3_map", "event:NEOW",
}


def make(seed: str) -> sts2_rs.RunManager:
    return sts2_rs.RunManager(
        string_seed=seed, character="Ironclad", ascension_level=0, neow_start=True,
        interactive_card_removal=True, interactive_card_upgrade=True, interactive_card_transform=True,
        act1="overgrowth",
    )


def _v(x):
    return x() if callable(x) else x


def observable(rm: sts2_rs.RunManager) -> dict:
    return {
        "counters": rm.rng_counters(), "phase": _v(rm.phase), "deck": _v(rm.master_deck),
        "relics": _v(rm.relics), "neow": _v(rm.neow_offers), "hp": _v(rm.current_hp), "gold": _v(rm.gold),
        "floor": _v(rm.total_floor), "event": rm.current_event_name(), "actions": rm.available_actions(),
        "streams": {k: v["current"] for k, v in rm.rng_dump(n=0)["streams"].items()},
    }


@pytest.mark.parametrize("seed,run_seed,player_seed,_", ANCHORS)
def test_bases_match_oracle(seed, run_seed, player_seed, _):
    rm = make(seed)
    d = rm.rng_dump()
    assert d["type"] == "rng_trace"
    assert d["seed_string"] == seed
    assert d["run_seed"] == run_seed == rm.run_seed
    assert d["player_seed"] == player_seed == rm.player_seed
    assert d["prefix"] == ("old" if seed.startswith("old") else "")
    assert d["input_seed"] == seed.removeprefix("old")
    assert d["src_hash"] == sts2_rs.src_hash()
    b = sts2_rs.seed_bases(seed)
    assert (b["run_seed"], b["player_seed"]) == (run_seed, player_seed)
    assert sts2_rs.xxhash64_utf8("a") == 0xD24EC4F1A98C6E5B


def test_stream_set_models_and_draw_shape():
    d = make("P0A0").rng_dump(n=10)
    names = set(d["streams"])
    assert STREAMS_22 <= names
    dynamic = names - STREAMS_22 - {"run_manager"}
    assert len(dynamic) == 2 and {s.split(":")[0] for s in dynamic} == {"event", "encounter"}
    assert d["streams"]["run_manager"]["scope"] == "rust_only"
    assert len(d["models"]) == 10
    assert [m["model"] for m in d["models"]][0] == "neow"
    for m in d["models"]:
        assert set(m) == {"model", "streams", "note"}
        for s in m["streams"]:
            assert s in names
    up = d["streams"]["up_front"]
    assert up["initial"]["counter"] == 0
    assert [c["value"] for c in up["draws"][:1]] == [59]  # oracle: P0A0 up_front NextInt(0,100)
    assert len(up["draws"]) == 10
    assert [c["call"] for c in up["draws"][:3]] == ["NextInt(min,max)", "NextFloat()", "NextInt(n)"]
    assert up["next_draws"] is not None and up["current"]["counter"] > 0  # live, already advanced by setup
    # Streams the game builds and drops carry no live state.
    for name in ("act_selection", "act_1_map", "act_2_map", "act_3_map"):
        assert d["streams"][name]["current"] is None and d["streams"][name]["next_draws"] is None
    # Neow screen: the event Rng is retained and sits past its offer rolls.
    neow = d["streams"]["event:NEOW"]
    assert neow["current"] is not None and neow["current"]["counter"] > 0


def test_dump_is_read_only():
    rm = make("oldP0A0")
    before = observable(rm)
    d1 = rm.rng_dump(n=10)
    d2 = rm.rng_dump(n=3, schedule=[{"call": "NextInt", "args": [0, 7]}, {"call": "NextBool", "args": []}])
    after = observable(rm)
    assert before == after
    # Repeated dumps agree (same live state, same draws).
    assert d1["streams"]["shuffle"]["next_draws"][:3] == rm.rng_dump(n=3)["streams"]["shuffle"]["next_draws"]
    assert len(d2["streams"]["shuffle"]["draws"]) == 3
    assert d2["schedule"][0] == {"call": "NextInt(min,max)", "args": [0, 7]}


def test_next_draws_continue_from_the_live_cursor():
    rm = make("P0A0")
    d = rm.rng_dump(n=10)
    cur = d["streams"]["rewards"]["current"]["counter"]
    # Advance the real stream via a counter restore, then the next dump's `current` follows.
    rm.debug_set_rng_counters(rm.run_seed, {"rewards": cur + 2})
    d2 = rm.rng_dump(n=10)
    assert d2["streams"]["rewards"]["current"]["counter"] == cur + 2
    assert d2["streams"]["rewards"]["initial"] == d["streams"]["rewards"]["initial"]


def test_neow_stream_is_live_only_while_the_event_is_open():
    rm = make("P0A0")
    assert rm.rng_dump(n=0)["streams"]["event:NEOW"]["current"] is not None
    # Leave the Neow event by taking an option (and any follow-up choices).
    for _ in range(10):
        if _v(rm.phase) != "event" or rm.current_event_name() != "Neow":
            break
        rm.take_action(rm.available_actions()[0])
    assert rm.current_event_name() != "Neow"
    d = rm.rng_dump(n=10)["streams"]["event:NEOW"]
    assert d["current"] is None and d["next_draws"] is None
    assert d["draws"][0]["value"] == 5  # stage1 vector: P0A0 event:NEOW NextInt(0,100) on the initial state


def test_bad_schedule_and_n_raise():
    rm = make("P0A0")
    with pytest.raises(ValueError):
        rm.rng_dump(schedule=[{"call": "NextInt", "args": [5, 5]}])
    with pytest.raises(ValueError):
        rm.rng_dump(schedule=[{"call": "Bogus", "args": []}])
    with pytest.raises(ValueError):
        rm.rng_dump(n=10_001)


def test_python_combat_constructor_keeps_the_high_32_bits_of_the_seed():
    """Two seeds that differ only in the high word must not collide (the constructor used to truncate)."""
    lo = 7
    hi = lo + (1 << 32)
    a = sts2_rs.state_to_dict(sts2_rs.CombatState(seed=lo))["rng"]
    b = sts2_rs.state_to_dict(sts2_rs.CombatState(seed=hi))["rng"]
    assert (a["run_seed"], b["run_seed"]) == (lo, hi)
    assert a["player_seed"] != b["player_seed"]


def test_python_rollouts_keep_the_high_32_bits_of_the_seed():
    """`sts2_rs.rollouts` seeds playout k with seed+k; two seeds differing only in the high word must not replay each other."""
    lo = 7
    hi = lo + (1 << 32)
    a = sts2_rs.rollouts(sts2_rs.CombatState(seed=lo, encounter="Nibbit"), n=300)
    b = sts2_rs.rollouts(sts2_rs.CombatState(seed=hi, encounter="Nibbit"), n=300)
    assert (a["wins"], a["avg_turns"]) != (b["wins"], b["avg_turns"])
