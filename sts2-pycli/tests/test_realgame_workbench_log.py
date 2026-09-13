"""WorkbenchRunLogger -> TimelineStore round trip (live-run mirroring).

The advisor mirrors live runs into GameLogger-schema JSONLs the workbench
replays natively; this pins the round trip that validated the lane before it
shipped 2026-08-21: a decision + human action + run end written by the
logger must come back out of ``TimelineStore.load_jsonl`` as steps the
workbench can render.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))

from realgame_workbench_log import WorkbenchRunLogger
from workbench.timeline import TimelineStore

EVENT_MSG: dict[str, Any] = {
    "type": "advice_request",
    "advice_for": "event",
    "seed": "S1",
    "floor": 3,
    "act": 1,
    "state_id": 7,
    "run_state": {
        "character": "IRONCLAD",
        "ascension": 8,
        "hp": 60,
        "max_hp": 75,
        "gold": 120,
        "master_deck": [{"id": "STRIKE_IRONCLAD", "upgrade_level": 0}],
        "relics": [{"id": "BURNING_BLOOD"}],
        "potions": [],
    },
    "options": [
        {
            "index": 0,
            "label": "Decipher",
            "description": "Lose 3 Max HP.",
            "enabled": True,
            "event_id": "TABLET_OF_TRUTH",
        },
        {
            "index": 1,
            "label": "Smash",
            "description": "Heal 20 HP.",
            "enabled": True,
            "event_id": "TABLET_OF_TRUTH",
        },
    ],
}


def _run_log(tmp_path: Path) -> Path:
    wlog = WorkbenchRunLogger(str(tmp_path))
    assert wlog.log_decision(EVENT_MSG, "event") is True
    path = wlog.path
    assert path is not None
    wlog.log_human_action({"detail": "Smash", "action": "choose"})
    wlog.log_run_end(
        {
            "type": "game_over",
            "victory": False,
            "floor": 3,
            "act": 1,
            "run_state": EVENT_MSG["run_state"],
        }
    )
    return Path(path)


class TestRoundTrip:
    def test_decision_action_and_terminal_come_back_as_steps(self, tmp_path: Path) -> None:
        store = TimelineStore.load_jsonl(_run_log(tmp_path))
        assert len(store) == 2  # the event decision + the terminal state
        step = store.get(0)
        assert step["decision"] == "event_choice"
        assert step["state"]["floor"] == 3
        assert [o.get("title") for o in step["state"]["options"]] == ["Decipher", "Smash"]
        # The human action attached to the decision step it answered.
        assert step["chosen_action"] is not None
        assert step["chosen_action"].get("label") == "Smash"

    def test_live_context_key_never_collides_with_card_select_context(self, tmp_path: Path) -> None:
        # `context` is the card_select PURPOSE STRING in native workbench
        # logs; the live mirror's envelope must ride under `live_context`
        # (a dict under `context` crashed /api/step — shipped bug).
        store = TimelineStore.load_jsonl(_run_log(tmp_path))
        state = store.get(0)["state"]
        assert "context" not in state
        assert state["live_context"]["floor"] == 3

    def test_duplicate_board_is_not_logged_twice(self, tmp_path: Path) -> None:
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision(EVENT_MSG, "event") is True
        assert wlog.log_decision(EVENT_MSG, "event") is False
        wlog.close()

    def test_disabled_logger_writes_nothing(self, tmp_path: Path) -> None:
        wlog = WorkbenchRunLogger(None)
        assert wlog.log_decision(EVENT_MSG, "event") is False
        assert wlog.path is None


class TestCombatLog:
    """The game's own combat-log rows ride along on the state record.

    They are stored RAW on purpose: the advisor has no localization, so ids
    stay ids until the workbench server renders them. What is pinned here is
    that they survive the write/read round trip with the character id the
    renderer needs.
    """

    def test_rows_and_character_survive_the_round_trip(self, tmp_path: Path) -> None:
        rows = [{"kind": "damage", "text": "IRONCLAD dealt 6 damage to NIBBIT"}]
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision({**EVENT_MSG, "combat_log": rows}, "event") is True
        assert wlog.path is not None

        state = TimelineStore.load_jsonl(Path(wlog.path)).get(0)["state"]
        assert state["combat_log_raw"] == rows
        assert state["character_id"] == "IRONCLAD"

    def test_a_decision_with_no_rows_adds_no_key(self, tmp_path: Path) -> None:
        # An older mod sends no `combat_log`; don't write an empty key into
        # every state of every run for it.
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision(EVENT_MSG, "event") is True
        assert wlog.path is not None
        state = TimelineStore.load_jsonl(Path(wlog.path)).get(0)["state"]
        assert "combat_log_raw" not in state


class TestActName:
    """The act's NAME reaches the replayed step, not just its index.

    `pickScene` keys every painted region off `meta.act_name` and falls back to
    the act-1 Overgrowth room without it — which is why a replayed live run used
    to paint Overgrowth for all three acts (act 1 ships two models, GH #544, so
    the index alone cannot name the room). The mod supplies it through
    `ApplyEnvelope`; this pins that the logger carries it into the step.
    """

    def test_act_name_survives_the_round_trip(self, tmp_path: Path) -> None:
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision({**EVENT_MSG, "act_name": "Underdocks"}, "event") is True
        assert wlog.path is not None
        step = TimelineStore.load_jsonl(Path(wlog.path)).get(0)
        assert step["state"]["act_name"] == "Underdocks"
        # `meta` is what the frontend reads.
        assert step["meta"]["act_name"] == "Underdocks"

    def test_a_mod_that_sends_no_act_name_yields_none(self, tmp_path: Path) -> None:
        # Pre-fix mod builds send no `act_name`; it must read as absent rather
        # than as the empty string, which is not a room key either way but would
        # make the fallback look like a deliberate choice.
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision(EVENT_MSG, "event") is True
        assert wlog.path is not None
        assert TimelineStore.load_jsonl(Path(wlog.path)).get(0)["state"]["act_name"] is None


class TestEnemyKind:
    """The monster's canonical kind rides along on a combat step.

    Art resolution needs it and the display name cannot give it: the three
    Decimillipede segments all localize to "Decimillipede". It is grafted on the
    RENDER side (`_decorate_combat_for_render`), never into the dict the search
    reconstructs the fight from.
    """

    @staticmethod
    def _combat_msg() -> dict[str, Any]:
        return {
            "type": "advice_request",
            "advice_for": "combat_action",
            "seed": "S1",
            "floor": 24,
            "act": 2,
            "act_name": "Hive",
            "state_id": 11,
            "round": 1,
            "energy": 3,
            "max_energy": 3,
            "run_state": EVENT_MSG["run_state"],
            "player": {"hp": 60, "max_hp": 75, "block": 0, "powers": []},
            "hand": [],
            "draw_pile": {"cards": []},
            "discard_pile": {"cards": []},
            "exhaust_pile": {"cards": []},
            "play_pile": {"cards": []},
            "enemies": [
                {
                    "id": "DECIMILLIPEDE_SEGMENT_FRONT",
                    "name": "Decimillipede",
                    "hp": 42,
                    "max_hp": 42,
                    "block": 0,
                    "is_alive": True,
                    "powers": [],
                    "intents": [],
                },
                {
                    "id": "DECIMILLIPEDE_SEGMENT_MIDDLE",
                    "name": "Decimillipede",
                    "hp": 46,
                    "max_hp": 46,
                    "block": 0,
                    "is_alive": True,
                    "powers": [],
                    "intents": [],
                },
            ],
        }

    def test_kind_is_grafted_per_enemy(self, tmp_path: Path) -> None:
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision(self._combat_msg(), "combat_action") is True
        assert wlog.path is not None
        state = TimelineStore.load_jsonl(Path(wlog.path)).get(0)["state"]
        assert [e["kind"] for e in state["enemies"]] == [
            "DECIMILLIPEDE_SEGMENT_FRONT",
            "DECIMILLIPEDE_SEGMENT_MIDDLE",
        ]

    def test_an_enemy_the_mod_sends_no_id_for_adds_no_key(self, tmp_path: Path) -> None:
        msg = self._combat_msg()
        for enemy in msg["enemies"]:
            enemy.pop("id")
        wlog = WorkbenchRunLogger(str(tmp_path))
        assert wlog.log_decision(msg, "combat_action") is True
        assert wlog.path is not None
        state = TimelineStore.load_jsonl(Path(wlog.path)).get(0)["state"]
        assert all("kind" not in e for e in state["enemies"])
