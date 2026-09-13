import json

import pytest
from workbench.timeline import TimelineStore


def test_faithful_replay_accessors(tmp_path):
    """Head meta (fixture_path / mcts_seed) and the ordered native actions are
    exposed for the faithful re-search (reload fixture + replay)."""
    records = [
        {"step": 0, "type": "meta", "data": {"fixture_path": "/abs/fx.toml", "mcts_seed": 7}},
        {"step": 0, "type": "state", "data": {"decision": "combat_play", "round": 1}},
        {
            "step": 0,
            "type": "action",
            "data": {"action": "Strike", "args": {"action": "play", "card_index": 0}},
        },
        {"step": 1, "type": "state", "data": {"decision": "combat_play", "round": 1}},
        {
            "step": 1,
            "type": "action",
            "data": {"action": "end turn", "args": {"action": "end_turn"}},
        },
        {"step": 2, "type": "state", "data": {"decision": "combat_play", "round": 2}},
        {"step": 2, "type": "meta", "data": {"summary": True, "final_won": True}},
    ]
    log = tmp_path / "run.jsonl"
    log.write_text("\n".join(json.dumps(r) for r in records))
    store = TimelineStore.load_jsonl(str(log))

    assert store.fixture_path() == "/abs/fx.toml"
    assert store.mcts_seed() == 7  # not the default 1 — read from head meta
    # Reaching step 2 replays the actions of steps 0 and 1, in order.
    assert store.native_actions_before(2) == [
        {"action": "play", "card_index": 0},
        {"action": "end_turn"},
    ]
    assert store.native_actions_before(0) == []  # decision 0 needs no replay


def test_faithful_replay_unavailable_without_fixture(fixture_log):
    """A run with no fixture_path falls back (None signals dict reconstruction)."""
    store = TimelineStore.load_jsonl(fixture_log)
    assert store.fixture_path() is None
    assert store.mcts_seed() == 1  # default


def test_load_pairs_state_and_action(fixture_log):
    store = TimelineStore.load_jsonl(fixture_log)
    assert len(store) == 2

    s0 = store.get(0)
    assert s0["idx"] == 0
    assert s0["decision"] == "combat_play"
    assert s0["state"]["round"] == 3
    assert s0["chosen_action"] == {"command": "play_card", "card_index": 0, "target_index": 0}
    assert s0["scores"] == []
    assert s0["pv"] is None
    assert s0["source"] == "log"
    assert s0["meta"]["turn"] == 3

    s1 = store.get(1)
    assert s1["decision"] == "map_select"
    assert s1["chosen_action"] is None  # run ended on this state


def test_get_out_of_range_raises(fixture_log):
    store = TimelineStore.load_jsonl(fixture_log)
    with pytest.raises(IndexError):
        store.get(99)


def test_leading_action_before_first_state_is_ignored(tmp_path, combat_state):
    """An action line before any state (e.g. start_run) must not corrupt pairing."""
    import json

    def _line(step, kind, data):
        return json.dumps({"step": step, "ts": "2026-05-24T00:00:00", "type": kind, "data": data})

    p = tmp_path / "run.jsonl"
    p.write_text(
        "\n".join(
            [
                _line(0, "action", {"command": "start_run", "character": "Ironclad"}),
                _line(1, "state", combat_state),
                _line(1, "action", {"command": "play_card", "card_index": 0, "target_index": 0}),
            ]
        )
        + "\n"
    )

    store = TimelineStore.load_jsonl(p)
    assert len(store) == 1
    s0 = store.get(0)
    # The state's own action must be correctly paired (index 0, not offset by -1).
    assert s0["chosen_action"] == {"command": "play_card", "card_index": 0, "target_index": 0}


def test_load_jsonl_attaches_persisted_mcts_scores():
    from pathlib import Path

    from workbench.timeline import TimelineStore

    src = Path(__file__).parent / "fixtures" / "sample_mcts_run.jsonl"
    store = TimelineStore.load_jsonl(str(src))
    step = store.get(0)  # the combat_play state
    assert step["scores"], "persisted mcts root_outcomes should populate scores"
    assert step["pv"] == ["End turn"]
    best = next(s for s in step["scores"] if s.get("is_best"))
    assert best["losses"] == 690
