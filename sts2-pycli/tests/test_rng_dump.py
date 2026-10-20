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


# --- R0-s: seed-path fixes (witnesses: logs/engine/rust_resume/r0/witnesses.md) ---

OLD_ANCHORS = [  # oracle f79d3a9 stage1_vectors.json: (seed, run_seed, player_seed)
    ("oldP0A0", 273692109, 15676685915450073139),
    ("oldP0A1", 3002575464, 18283884523336027873),
    ("oldP0A2", 3405859991, 17731986911216078246),
    ("oldP0A3", 1839776050, 13570675488851547012),
    ("oldP0A4", 2599290937, 3030005280704353688),
]


@pytest.mark.parametrize("seed,run_seed,player_seed", OLD_ANCHORS)
def test_old_prefix_seed_takes_the_migrated_save_rule(seed, run_seed, player_seed):
    """(a) `old` is judged before normalisation: the run base is the legacy 32-bit hash of the rest, the oracle's
    player base is the XxHash64 of the whole string (reported as `player_seed`; the engine's own player streams
    still hang off the run base, see `engine_player_seed`)."""
    rm = make(seed)
    d = rm.rng_dump()
    assert d["run_seed"] == run_seed == rm.run_seed
    assert d["player_seed"] == player_seed == rm.player_seed
    assert (d["prefix"], d["input_seed"]) == ("old", seed[3:])
    assert d["engine_player_seed"] == run_seed != player_seed


def test_old_prefix_rule_is_v0111_only_and_case_sensitive():
    h = lambda s, v: sts2_rs.RunManager(string_seed=s, game_version=v).run_seed  # noqa: E731
    assert h("oldP0A0", "v0.107.1") == sts2_rs.RunManager(string_seed="OLDP0A0", game_version="v0.107.1").run_seed
    assert h("OLDP0A0", V) == 9137717999390313649  # canonicalised to 0LDP0A0, plain hash (pristine value)
    assert h("P0A0", V) == 10883704133362413376


@pytest.mark.parametrize(
    "seed,canonical_run_seed,raw_run_seed",
    [
        ("P0A0", 10883704133362413376, 10883704133362413376),  # already canonical: the option changes nothing
        ("P0A1", 5101801426979218024, 5101801426979218024),
        ("p0a0", 10883704133362413376, 9004872484816525140),
        ("P0AO", 10883704133362413376, 1062546349297132942),
        (" P0A0", 10883704133362413376, 8344762209911765837),
        ("OLDP0A0", 9137717999390313649, 101060511391403812),
        ("oldP0A0", 273692109, 273692109),
    ],
)
def test_raw_seed_hash_option(seed, canonical_run_seed, raw_run_seed):
    assert make(seed).run_seed == canonical_run_seed  # default unchanged
    assert make(seed, raw_seed_hash=True).run_seed == raw_run_seed
    assert make(seed, raw_seed_hash=True).run_seed == sts2_rs.run_seed_hash(seed, V)


def test_old_seed_act_selection_uses_the_full_string_hash():
    """act1="roll" seeds `act_selection` from H(whole string) (StartRunLobby.cs:468): oracle roll vector for oldP0A0
    puts Overgrowth's first event at WAR_HISTORIAN_REPY and first encounter at CORPSE_SLUGS_WEAK."""
    rm = sts2_rs.RunManager(
        string_seed="oldP0A0", character="Ironclad", ascension_level=0, neow_start=True, act1="roll", game_version=V,
    )
    names = set(rm.rng_dump()["streams"])
    assert {"event:WAR_HISTORIAN_REPY", "encounter:CORPSE_SLUGS_WEAK"} <= names
