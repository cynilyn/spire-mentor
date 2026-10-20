"""`RunManager.rng_dump` contract (v0.111.0), ported from the own-port tree (donor 3088de5 / 849e824 tests).

Shape, read-only guarantee and oracle anchors. Oracle vectors: sts2_bot ``logs/engine/p0/stage1_vectors.json``
(sha256 5267538d84c1a1468b68656b82bdd270e4a4e21b60a251366beb475786aa8976, oracle f79d3a9, game 0.111.0). The full
20-seed x 22-stream x 10-draw comparison (W1-W4) lives in ``logs/engine/p0/tools/compare_vectors.py``.
"""

from __future__ import annotations

import pytest
import sts2_rs

V = "v0.111.0"

STREAMS_20 = {
    "up_front", "shuffle", "unknown_map_point", "combat_card_generation", "combat_potion_generation",
    "combat_card_selection", "combat_energy_costs", "combat_targets", "monster_ai", "niche", "combat_orbs",
    "treasure_room_relics", "rewards", "shops", "transformations", "act_selection", "act_1_map", "act_2_map",
    "act_3_map", "event:NEOW",
}


def make(seed: str, **kw) -> sts2_rs.RunManager:
    return sts2_rs.RunManager(
        string_seed=seed, character="Ironclad", ascension_level=0, neow_start=True,
        interactive_card_removal=True, interactive_card_upgrade=True, interactive_card_transform=True,
        act1="overgrowth", game_version=V, **kw,
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


def test_bases_of_a_plain_seed_match_the_oracle():
    rm = make("P0A0")
    d = rm.rng_dump()
    assert d["type"] == "rng_trace"
    assert d["seed_string"] == "P0A0" and d["input_seed"] == "P0A0" and d["prefix"] == ""
    assert d["run_seed"] == d["player_seed"] == rm.run_seed == rm.player_seed == 10883704133362413376
    assert d["engine_player_seed"] == d["player_seed"]
    assert d["src_hash"] == sts2_rs.src_hash()


def test_stream_set_models_and_draw_shape():
    d = make("P0A0").rng_dump(n=10)
    names = set(d["streams"])
    assert STREAMS_20 <= names
    dynamic = names - STREAMS_20 - {"run_manager"}
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
    rm = make("P0A0")
    before = observable(rm)
    d1 = rm.rng_dump(n=10)
    d2 = rm.rng_dump(n=3, schedule=[{"call": "NextInt", "args": [0, 7]}, {"call": "NextBool", "args": []}])
    after = observable(rm)
    assert before == after
    # Repeated dumps agree (same live state, same draws).
    assert d1 == rm.rng_dump(n=10)
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


def test_game_version_env_is_the_default_for_calls_without_the_kwarg(monkeypatch):
    """H3: `STS2_GAME_VERSION` names the build when no `game_version=` is passed (the parity harness constructs
    RunManager / calls roll_acts directly); with the variable unset the default build is unchanged."""
    monkeypatch.delenv("STS2_GAME_VERSION", raising=False)
    assert sts2_rs.RunManager(string_seed="P0A0", neow_start=True).game_version == "v0.107.1"
    monkeypatch.setenv("STS2_GAME_VERSION", V)
    assert sts2_rs.RunManager(string_seed="P0A0", neow_start=True).game_version == V
    # an explicit kwarg wins over the environment
    assert sts2_rs.RunManager(string_seed="P0A0", neow_start=True, game_version="v0.107.1").game_version == "v0.107.1"
    monkeypatch.setenv("STS2_GAME_VERSION", "  ")
    assert sts2_rs.RunManager(string_seed="P0A0", neow_start=True).game_version == "v0.107.1"
    monkeypatch.setenv("STS2_GAME_VERSION", "v9.9.9")
    with pytest.raises(ValueError):
        sts2_rs.RunManager(string_seed="P0A0", neow_start=True)
