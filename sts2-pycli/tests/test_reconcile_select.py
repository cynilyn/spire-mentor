"""p0 stage 0d: step-0 Neow comparison + one-sided card_select reconciliation.

Pure-python tests (no oracle, no sts2_rs). The recorded-log test replays the
Rust views of the four P0-A a2 R1 seeds whose ``skip_select`` the engine
rejected (P0A11/24/25/26) and checks that the new rule issues no
``skip_select`` on a mandatory screen. It skips when the recorded request logs
are not on this machine (override with STS2_P0A_A2_R1_REC).
"""

from __future__ import annotations

import glob
import json
import os

import pytest
import run_parity as rp

MANDATORY = {
    "decision": "card_select",
    "min_select": 1,
    "max_select": 1,
    "can_skip": False,
    "cards": [{"index": 0, "id": "STRIKE_IRONCLAD"}, {"index": 1, "id": "STRIKE_IRONCLAD"}],
}
OPTIONAL = {**MANDATORY, "min_select": 0, "can_skip": True}
MAP = {"decision": "map_select", "player": {}}


class FakeRust:
    def __init__(self, after_each=None):
        self.calls: list[tuple[str, dict]] = []
        self.after_each = after_each or [MAP]

    def act(self, verb, **kw):
        self.calls.append((verb, kw))
        i = min(len(self.calls) - 1, len(self.after_each) - 1)
        return self.after_each[i]


def test_mandatory_picks_first_legal_never_skip():
    assert rp.reconcile_select_action(MANDATORY) == ("select_cards", {"indices": "0"})
    multi = {**MANDATORY, "min_select": 2, "max_select": 2}
    assert rp.reconcile_select_action(multi) == ("select_cards", {"indices": "0,1"})


def test_optional_skips():
    assert rp.reconcile_select_action(OPTIONAL) == ("skip_select", {})


def test_mandatory_without_enough_cards_is_none():
    assert rp.reconcile_select_action({**MANDATORY, "min_select": 3}) is None


def test_drive_rs_alone_mandatory_issues_no_skip_select():
    r = FakeRust()
    view, snap, sent = rp.drive_rs_alone_through_select(r, MANDATORY, rp.Snap.of(MANDATORY, "rs"))
    assert [v for v, _ in r.calls] == ["select_cards"]
    assert sent == r.calls and snap.phase == "map"


def test_drive_rs_alone_rejection_is_left_to_recorder():
    r = FakeRust(after_each=[{"type": "error", "message": "no"}])
    _, snap, sent = rp.drive_rs_alone_through_select(r, MANDATORY, rp.Snap.of(MANDATORY, "rs"))
    assert len(sent) == 1 and snap.phase == "card_select"  # stops, no retry loop


NEOW_CS = {
    "decision": "event_choice",
    "options": [
        {"index": 0, "text_key": "NEOW.pages.INITIAL.options.LOST_COFFER"},
        {"index": 1, "text_key": "NEOW.pages.INITIAL.options.WINGED_BOOTS"},
        {"index": 2, "text_key": "NEOW.pages.INITIAL.options.CURSED_PEARL"},
    ],
}
NEOW_RS_VIEW = {
    "decision": "event_choice",
    "options": [{"text_key": "FISHING_ROD"}, {"text_key": "LOST_COFFER"}, {"text_key": "LARGE_CAPSULE"}],
}


class _RM:
    def __init__(self, offers):
        self._o = offers

    def neow_offers(self):
        return self._o


class _RB:
    def __init__(self, offers):
        self.rm = _RM(offers)

    def native_run(self):
        return self.rm


def test_neow_cs_ids_ordered():
    assert rp.neow_offers_cs(NEOW_CS) == ("LOST_COFFER", "WINGED_BOOTS", "CURSED_PEARL")
    assert rp.neow_offers_cs({"decision": "map_select"}) is None


def test_neow_rs_prefers_native_then_view():
    assert rp.neow_offers_rs(NEOW_RS_VIEW, _RB(("A_B", "C", "D"))) == ("A_B", "C", "D")
    assert rp.neow_offers_rs(NEOW_RS_VIEW, _RB(None)) == ("FISHING_ROD", "LOST_COFFER", "LARGE_CAPSULE")


def test_step0_diff_has_neow_field_unsuppressed():
    cs, rs = rp.Snap.of(NEOW_CS, "cs"), rp.Snap.of(NEOW_RS_VIEW, "rs")
    rp.attach_step0_neow(cs, rs, NEOW_CS, NEOW_RS_VIEW, _RB(("FISHING_ROD", "LOST_COFFER", "LARGE_CAPSULE")))
    fields = rp.diff_snaps(cs, rs)
    assert fields[0].startswith("neow_offers: cs=['LOST_COFFER', 'WINGED_BOOTS', 'CURSED_PEARL'] rs=")
    # identical lists -> no field
    cs2, rs2 = rp.Snap.of(NEOW_CS, "cs"), rp.Snap.of(NEOW_CS, "rs")
    rp.attach_step0_neow(cs2, rs2, NEOW_CS, NEOW_CS, _RB(("LOST_COFFER", "WINGED_BOOTS", "CURSED_PEARL")))
    assert not any(f.startswith("neow_offers") for f in rp.diff_snaps(cs2, rs2))


def test_continue_past_neow_is_opt_in(monkeypatch):
    f = ["neow_offers: cs=[1] rs=[2]", "hp: cs=1 rs=2"]
    monkeypatch.delenv("STS2_RUNPARITY_CONTINUE_PAST_NEOW", raising=False)
    assert rp.split_step0_neow(f, 0) == (f, [])
    monkeypatch.setenv("STS2_RUNPARITY_CONTINUE_PAST_NEOW", "1")
    assert rp.split_step0_neow(f, 0) == (["hp: cs=1 rs=2"], ["neow_offers: cs=[1] rs=[2]"])
    assert rp.split_step0_neow(f, 1) == (f, [])


REC = os.environ.get(
    "STS2_P0A_A2_R1_REC",
    "/mnt/c/Users/cynilyn/fork_repository/sts2_bot/logs/engine/p0a/a2/r1/rec"
    if os.name != "nt"
    else r"C:\Users\cynilyn\fork_repository\sts2_bot\logs\engine\p0a\a2\r1\rec",
)
FOUR = {"P0A11", "P0A24", "P0A25", "P0A26"}


def _rejected_skip_views():
    """(seed, rust card_select view before the rejected skip_select) per a2 record."""
    out = []
    for f in sorted(glob.glob(os.path.join(REC, "*.jsonl"))):
        seed, prev = None, None
        for line in open(f, encoding="utf-8"):
            r = json.loads(line)
            if r.get("kind") != "rs_game":
                continue
            if r["method"] == "start":
                seed = r["kwargs"].get("seed")
            elif r["method"] == "act":
                verb = r["args"][0] if r["args"] else r["kwargs"].get("action")
                res = r.get("result") or {}
                if verb == "skip_select" and (res.get("type") == "error" or res.get("error")):
                    out.append((seed, prev))
                prev = res
    return out


@pytest.mark.skipif(not glob.glob(os.path.join(REC, "*.jsonl")), reason="recorded a2 R1 logs absent")
def test_recorded_four_seeds_no_skip_on_mandatory_screen():
    got = _rejected_skip_views()
    assert {s for s, _ in got} == FOUR
    for seed, view in got:
        assert view["decision"] == "card_select" and view["min_select"] >= 1, seed
        r = FakeRust()
        rp.drive_rs_alone_through_select(r, view, rp.Snap.of(view, "rs"))
        verbs = [v for v, _ in r.calls]
        assert verbs and "skip_select" not in verbs, (seed, verbs)
