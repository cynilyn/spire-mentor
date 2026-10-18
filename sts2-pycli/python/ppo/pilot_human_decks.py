"""Does our combat pilot handle exhaust / self-damage decks? (GH #777 follow-up)

The PPO run policy almost never takes the Ironclad exhaust / self-damage engine
(Burning Pact, Dark Embrace, Second Wind, True Grit, ...) that strong humans
take routinely (GH #778). That is right if our MCTS pilot cannot play those
cards. This tests the pilot directly on HUMAN decks and HUMAN fights:

  extract  — for spire-codex runs (v0.107.1, single-player, standard), rebuild
             every combat: deck at that floor (cards + upgrades), relics,
             the potions the human used in it, HP before, encounter, and the
             human's HP after. Deck at floor f = final-deck cards added before
             f, plus cards removed / transformed away at or after f; upgrades
             are assigned from the per-floor upgrade log. Enchantments are
             counted but NOT simulated (the scenario builder has no field).
  play     — build each fight (`sts2_rs.build_scenario_state`), install it in a
             RunManager (`debug_enter_combat`) and let the DEPLOYED pilot
             (`MctsCombatAgent`, eval-cheap 1000 / boss 4000, honest
             BeliefDraw, TexelWide, EndTurn widening) play it. Arms:
               orig     the human's deck;
               abl_tag  every exhaust/self-damage card -> Strike;
               abl_ctl  the same number of random OTHER non-basic cards
                        -> Strike (the control: what a card is worth at all).
  report   — (1) pilot HP loss minus the human's on the same fight, by how
             many tagged cards the deck holds; (2) per tagged card, the HP the
             pilot saves by having it rather than a Strike, vs the same for
             control cards.

HP loss is NET of the victory heal on both sides: the human's HP before minus
`current_hp` at that floor, and the pilot's HP before minus its run HP after
the fight (the engine applies Burning Blood there — verified +6 on identical
seeds). The human's raw `damage_taken` is printed alongside. A pilot death is
scored as losing all HP it had. Run-side post-combat relic heals are not
simulated (the scenario's relics live in the combat only).
The scenario builder fixes the act index at 1.

The same bank measures the pilot's GENERAL gap to humans (`--arms orig`).
Each rep also records `dmg`, the unblocked damage the pilot took (summed from
the combat event log, the same quantity as the human's `damage_taken`, C#
`CreatureCmd.Damage`), `turns`, and `potions_left` (given potions not drunk).
Comparing `dmg` sidesteps the post-fight heals the rebuild cannot reproduce.
Enchanted cards are rebuilt as `ID[+1]@ENCHANTMENT:amount`, from the floor of
their `cards_enchanted` event on (from the floor added if there is none).
Seeds derive from the fight key, so a fight gets the same seeds in every file
(`--seed-by index` reproduces files made before that change).

`extract --character` picks the character (default Ironclad; GH #810 runs
Regent / Necrobinder); each fight row carries it, and `play` builds the fight
as that character.

    python -m ppo.pilot_human_decks extract --index players.jsonl --users pros.json \
        --sample-others 300 --out fights.jsonl
    python -m ppo.pilot_human_decks play --fights fights.jsonl --out played.jsonl
    python -m ppo.pilot_human_decks report --played played.jsonl
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import math
import os
import random
import re
import statistics
import sys
import time
import zlib
from multiprocessing import get_context
from typing import Any

COMBAT_ROOMS = frozenset({"monster", "elite", "boss"})
# Every enchantment id the v0.107.1 game defines; any other id on a card
# (e.g. UNIVERSAL_SPIRAL) marks a modded run.
ENCHANTMENTS = frozenset(
    [
        "ADROIT",
        "CLONE",
        "CORRUPTED",
        "GLAM",
        "GOOPY",
        "IMBUED",
        "INKY",
        "INSTINCT",
        "MOMENTUM",
        "NIMBLE",
        "PERFECT_FIT",
        "ROYALLY_APPROVED",
        "SHARP",
        "SLITHER",
        "SLUMBERING_ESSENCE",
        "SOULS_POWER",
        "SOWN",
        "SPIRAL",
        "STEADY",
        "SWIFT",
        "TEZCATARAS_EMBER",
        "VIGOROUS",
    ]
)
# Per character: the starter cards (never picked as ablation controls) and the
# filler an ablated card becomes. Starter decks as `RunManager.master_deck`.
STARTERS = {
    "Ironclad": ("STRIKE_IRONCLAD", ("DEFEND_IRONCLAD", "BASH")),
    "Silent": ("STRIKE_SILENT", ("DEFEND_SILENT", "NEUTRALIZE", "SURVIVOR")),
    "Defect": ("STRIKE_DEFECT", ("DEFEND_DEFECT", "ZAP", "DUALCAST")),
    "Regent": ("STRIKE_REGENT", ("DEFEND_REGENT", "FALLING_STAR", "VENERATE")),
    "Necrobinder": ("STRIKE_NECROBINDER", ("DEFEND_NECROBINDER", "BODYGUARD", "UNLEASH")),
}


def basics(character: str) -> frozenset[str]:
    filler, rest = STARTERS[character]
    return frozenset({filler, *rest, "ASCENDERS_BANE"})


def filler(character: str) -> str:
    return STARTERS[character][0]


def fight_character(fight: dict[str, Any]) -> str:
    # Banks extracted before the `character` field existed are Ironclad-only.
    return fight.get("character", "Ironclad")


LOC_CARDS = os.path.join(os.path.dirname(__file__), "..", "..", "localization_eng", "cards.json")


def tagged_cards(pool_path: str | None = None, loc_path: str = LOC_CARDS) -> dict[str, set[str]]:
    """Exhaust-synergy and self-damage cards, read from the game's card text.

    `exhaust`: the card exhausts OTHER cards or pays off exhausting (the
    `[gold]Exhaust...` verb / `Exhaust Pile` in its description; a card's own
    Exhaust keyword is not in the description). `selfdmg`: "Lose {HpLoss}" or
    pays off losing HP.
    """
    loc = json.load(open(loc_path))
    exhaust, selfdmg = set(), set()
    for k, v in loc.items():
        if not k.endswith(".description"):
            continue
        name = k[: -len(".description")]
        if re.search(r"\[gold\]Exhaust(ed)?\[/gold\]|Exhaust Pile", v):
            exhaust.add(name)
        if "{HpLoss" in v or re.search(r"lose HP", v, re.IGNORECASE):
            selfdmg.add(name)
    if pool_path:
        # Card text matches other characters' cards and curses too; restrict
        # to the character's reward pool (`--pool`: ids offered in >= 0.3% of
        # its human combat rewards).
        pool = set(json.load(open(pool_path)))
        exhaust &= pool
        selfdmg &= pool
    return {"exhaust": exhaust, "selfdmg": selfdmg}


# ── extract ──────────────────────────────────────────────────────────────


def _bare(x: str) -> str:
    return x.split(".", 1)[1] if "." in x else x


def extract_run(
    path: str, asc: int, build: str, character: str = "Ironclad"
) -> list[dict[str, Any]]:
    with gzip.open(path) as f:
        d = json.load(f)
    players = d.get("players") or []
    if (
        d.get("build_id") != build
        or d.get("game_mode") != "standard"
        or d.get("was_abandoned")
        or len(players) != 1
        or d.get("ascension") != asc
        or players[0].get("character") != f"CHARACTER.{character.upper()}"
    ):
        return []
    p = players[0]
    deck = p.get("deck") or []
    # Modded clients (BaseLib card modifiers, non-v0.107.1 enchantments) play
    # different cards.
    if any(any(k.startswith("save_dict") for k in c) for c in deck) or any(
        _bare(c["enchantment"]["id"]) not in ENCHANTMENTS for c in deck if c.get("enchantment")
    ):
        return []
    pts = [pt for act in d["map_point_history"] for pt in act]
    acts = [ai + 1 for ai, act in enumerate(d["map_point_history"]) for _ in act]
    # Cards that left the deck, with the floor they left at.
    gone: list[tuple[dict[str, Any], int]] = []
    upg_events: dict[str, list[int]] = collections.defaultdict(list)
    # (id, floor added) -> [(floor, enchantment, amount)] from `cards_enchanted`.
    ench_events: dict[tuple[str, int], list[tuple[int, str, int]]] = collections.defaultdict(list)
    for i, pt in enumerate(pts):
        floor = i + 1
        ps = pt["player_stats"][0]
        for c in ps.get("cards_removed") or []:
            gone.append((c, floor))
        for t in ps.get("cards_transformed") or []:
            gone.append((t["original_card"], floor))
        for cid in ps.get("upgraded_cards") or []:
            upg_events[_bare(cid)].append(floor)
        for ev in ps.get("cards_enchanted") or []:
            c = ev.get("card") or {}
            e = c.get("enchantment") or {}
            ench_events[_bare(c.get("id", "")), c.get("floor_added_to_deck", 0)].append(
                (floor, _bare(ev.get("enchantment") or e.get("id", "")), int(e.get("amount", 1)))
            )

    # Each enchanted copy takes the first unclaimed event that matches its
    # (id, floor added, enchantment) and is enchanted from that floor on; a copy
    # with no such event came enchanted (reward / event card), from the floor
    # it was added.
    def enchant_stamp(c: dict[str, Any]) -> tuple[int, str, int] | None:
        e = c.get("enchantment")
        if not e:
            return None
        eid = _bare(e["id"])
        evs = ench_events.get((_bare(c["id"]), c.get("floor_added_to_deck", 0)), [])
        for j, (fl, eid_ev, amount) in enumerate(evs):
            if eid_ev == eid:
                evs.pop(j)
                return fl, eid, amount
        return c.get("floor_added_to_deck", 0), eid, int(e.get("amount", 1))

    stamps = {id(c): enchant_stamp(c) for c in [*deck, *(g for g, _ in gone)]}

    def enchant_suffix(c: dict[str, Any], floor: int) -> str:
        st = stamps.get(id(c))
        return f"@{st[1]}:{st[2]}" if st is not None and st[0] < floor else ""

    relics = p.get("relics") or []
    fights = []
    for i, pt in enumerate(pts):
        if i == 0:
            continue
        floor = i + 1
        rooms = pt.get("rooms") or []
        room_type = rooms[0].get("room_type") if rooms else None
        if room_type not in COMBAT_ROOMS:
            continue
        prev = pts[i - 1]["player_stats"][0]
        ps = pt["player_stats"][0]
        present = [c for c in deck if c.get("floor_added_to_deck", 0) < floor]
        present_gone = [c for c, g in gone if c.get("floor_added_to_deck", 0) < floor <= g]
        # Upgrades: an upgraded final copy counts as upgraded at `floor` unless
        # an upgrade of that id is still to come (event at a floor >= `floor`).
        by_id: dict[str, list[dict[str, Any]]] = collections.defaultdict(list)
        for c in present:
            by_id[_bare(c["id"])].append(c)
        cards: list[str] = []
        for cid, copies in by_id.items():
            copies.sort(key=lambda c: c.get("floor_added_to_deck", 0))
            n_up_final = sum(1 for c in copies if c.get("current_upgrade_level", 0) >= 1)
            later = sum(1 for g in upg_events.get(cid, []) if g >= floor)
            n_up = max(0, n_up_final - later)
            for j, c in enumerate(copies):
                cards.append(cid + ("+1" if j < n_up else "") + enchant_suffix(c, floor))
        cards += [_bare(c["id"]) + enchant_suffix(c, floor) for c in present_gone]
        fights.append(
            {
                "file": os.path.basename(path),
                "character": character,
                "username": d.get("username"),
                "won_run": bool(d.get("win")),
                "ascension": asc,
                "floor": floor,
                "act": acts[i],
                "room_type": room_type,
                "encounter": _bare(rooms[0].get("model_id", "")),
                "deck": cards,
                "n_enchanted": sum("@" in c for c in cards),
                "relics": [
                    _bare(r["id"]) for r in relics if r.get("floor_added_to_deck", 0) < floor
                ],
                "potions": [_bare(x) for x in ps.get("potion_used") or []],
                "hp_before": prev.get("current_hp"),
                "max_hp": prev.get("max_hp"),
                "human_hp_after": ps.get("current_hp"),
                "human_damage_taken": ps.get("damage_taken"),
                "human_hp_healed": ps.get("hp_healed"),
                "human_max_hp_gained": ps.get("max_hp_gained"),
                "human_max_hp_lost": ps.get("max_hp_lost"),
                "human_turns": rooms[0].get("turns_taken"),
            }
        )
    return fights


def extract(args: argparse.Namespace) -> int:
    rows = [json.loads(line) for line in open(args.index)]
    rows = [r for r in rows if r["build"] == args.build and r["ascension"] == args.ascension]
    users = set(json.load(open(args.users))) if args.users else set()
    pro = [r for r in rows if r["username"] in users]
    rest = [r for r in rows if r["username"] not in users]
    rng = random.Random(args.sample_seed)
    others = rng.sample(rest, min(args.sample_others, len(rest)))
    n_runs = n_fights = 0
    with open(args.out, "w") as sink:
        for cohort, rs in (("pro", pro), ("other", others)):
            for r in rs:
                fights = extract_run(r["file"], args.ascension, args.build, args.character)
                if fights:
                    n_runs += 1
                for f in fights:
                    f["cohort"] = cohort
                    sink.write(json.dumps(f) + "\n")
                    n_fights += 1
    print(f"[pilot_human_decks] {n_runs} runs, {n_fights} fights -> {args.out}")
    return 0


# ── play ─────────────────────────────────────────────────────────────────


def _arms(fight: dict[str, Any], tags: set[str], rng: random.Random) -> dict[str, list[str]]:
    deck = fight["deck"]
    character = fight_character(fight)
    fill, basic = filler(character), basics(character)
    base = [c.split("+")[0] for c in deck]
    tagged_idx = [i for i, c in enumerate(base) if c in tags]
    arms = {"orig": deck}
    if tagged_idx:
        arms["abl_tag"] = [fill if i in tagged_idx else c for i, c in enumerate(deck)]
        ctl_pool = [i for i, c in enumerate(base) if c not in tags and c not in basic]
        if len(ctl_pool) >= len(tagged_idx):
            ctl = set(rng.sample(ctl_pool, len(tagged_idx)))
            arms["abl_ctl"] = [fill if i in ctl else c for i, c in enumerate(deck)]
    return arms


def _fight_seed(key: str) -> int:
    """Stable per-fight seed, so a fight gets the same seeds in every bank file."""
    return zlib.crc32(key.encode()) % 1_000_000 + 1


def _combat_record(rm: Any, given_potions: list[str]) -> dict[str, Any]:
    """What the finished fight's event log says: damage taken, turns, potions."""
    log = rm.combat_log()
    drunk = collections.Counter(r.get("potion") for r in log if r["kind"] == "potion")
    rec = {
        "dmg": sum(
            r.get("unblocked", 0)
            for r in log
            if r["kind"] == "damage" and (r.get("receiver") or {}).get("kind") == "player"
        ),
        "turns": max((r.get("round", 0) for r in log), default=0),
        "potions_left": sum((collections.Counter(given_potions) - drunk).values()),
        "drunk": sum(drunk.values()),
    }
    if rm.combat_log_dropped():
        rec["log_dropped"] = rm.combat_log_dropped()
    return rec


def _resolution(search: dict[str, Any], rule: str) -> float:
    """How resolved a finished root search is; LOWER = less certain.

    `z`: the gap between the best action and the best NON-equivalent runner-up
    (actions with >= 10 visits), in standard errors from each child's return
    variance (`value_var`). Two copies of a card lead to the same node through
    the transposition table and show identical visits and Q; they are one
    choice, not a tie. `share`: the top visit share.
    """
    acts = search["actions"]
    if rule == "share":
        return max(a["visits"] for a in acts) / max(1, sum(a["visits"] for a in acts))
    top = sorted((a for a in acts if a["visits"] >= 10), key=lambda a: -a["q"])
    if not top:
        return math.inf
    best = top[0]
    alt = next((a for a in top[1:] if (a["visits"], a["q"]) != (best["visits"], best["q"])), None)
    if alt is None:
        return math.inf
    se2 = sum(a.get("value_var", 1.0) / a["visits"] for a in (best, alt))
    return (best["q"] - alt["q"]) / math.sqrt(max(se2, 1e-12))


def play_fight(job: tuple[Any, ...]) -> dict[str, Any]:
    fight, reps, tags, seed, pilot, arms_sel = job
    import sts2_rs

    # `pilot`: (sims, boss_sims, clairvoyant, escalate). Clairvoyance is read
    # from the env on EVERY search, so it stays set for the worker's lifetime
    # (one pilot per `play` invocation). `escalate`: None, or (rule, threshold,
    # sims, boss_sims) — a decision whose search resolves below the threshold
    # is re-searched by a second, stronger agent, whose action is played.
    sims, boss_sims, clair, escalate = pilot
    if clair:
        os.environ["STS2_RUNAI_HONEST"] = "0"

    rng = random.Random(seed)
    out: dict[str, Any] = {"key": f"{fight['file']}:{fight['floor']}", "arms": {}}
    try:
        for arm, deck in _arms(fight, tags, rng).items():
            if arms_sel and arm not in arms_sel:
                continue
            res = []
            for rep in range(reps):
                s = seed * 100 + rep
                state = sts2_rs.build_scenario_state(
                    s,
                    fight_character(fight),
                    deck,
                    int(fight["hp_before"]),
                    int(fight["max_hp"]),
                    fight["relics"],
                    fight["encounter"],
                    ascension=int(fight["ascension"]),
                    potions=fight["potions"][:3],
                )
                rm = sts2_rs.RunManager(
                    seed=s,
                    character=fight_character(fight),
                    ascension_level=int(fight["ascension"]),
                )
                rm.debug_enter_combat(state, fight["room_type"])
                # Display-side log only: search never sees it (`MctsTree::new`
                # disarms its root clone), so it cannot change the play.
                rm.set_record_combat_log(True)
                agent = sts2_rs.MctsCombatAgent(seed=s, sims=sims, boss_sims=boss_sims)
                strong = (
                    sts2_rs.MctsCombatAgent(
                        seed=s + 7_000_000, sims=escalate[2], boss_sims=escalate[3]
                    )
                    if escalate
                    else None
                )
                steps = searches = escalated = 0
                while rm.phase() == "combat" and not rm.is_over() and steps < 3000:
                    # With escalation the base agent searches a clone; its
                    # move is kept (the clone becomes the fight) unless the
                    # search is unresolved, when the strong agent decides.
                    trial = rm.clone() if strong else rm
                    rec = agent.step(trial)
                    if rec is None:
                        break
                    search = rec.get("combat_search")
                    searches += search is not None
                    if strong:
                        if search and _resolution(search, escalate[0]) < escalate[1]:
                            escalated += 1
                            if strong.step(rm) is None:
                                break
                        else:
                            rm = trial
                    steps += 1
                died = rm.is_over() and not rm.player_won
                hp_after = 0 if died else int(rm.current_hp)
                res.append(
                    {"hp_after": hp_after, "died": died, "steps": steps, "searches": searches}
                    | ({"escalated": escalated} if strong else {})
                    | _combat_record(rm, fight["potions"][:3])
                )
            out["arms"][arm] = res
    except Exception as e:
        out["error"] = repr(e)
    return out


def play(args: argparse.Namespace) -> int:
    t = tagged_cards(args.pool)
    tags = set(args.focus.split(",")) if args.focus else t["exhaust"] | t["selfdmg"]
    fights = [json.loads(line) for line in open(args.fights)]
    if args.only_tagged:
        fights = [
            f
            for f in fights
            if f["n_enchanted"] == 0 and any(c.split("+")[0] in tags for c in f["deck"])
        ]
    escalate = None
    if args.escalate:
        rule, thr, hi, hi_boss = args.escalate.split(":")
        escalate = (rule, float(thr), int(hi), int(hi_boss))
    pilot = (args.sims, args.boss_sims, args.clair, escalate)
    arms_sel = set(args.arms.split(",")) if args.arms else None
    done = set()
    if os.path.exists(args.out):
        done = {json.loads(line)["key"] for line in open(args.out)}

    def seed_of(i: int, f: dict[str, Any]) -> int:
        return i + 1 if args.seed_by == "index" else _fight_seed(f"{f['file']}:{f['floor']}")

    jobs = [
        (f, args.reps, tags, seed_of(i, f), pilot, arms_sel)
        for i, f in enumerate(fights)
        if f"{f['file']}:{f['floor']}" not in done
    ]
    t0 = time.time()
    n = err = 0
    with get_context("spawn").Pool(args.procs) as pool, open(args.out, "a") as sink:
        for row in pool.imap_unordered(play_fight, jobs):
            sink.write(json.dumps(row) + "\n")
            sink.flush()
            n += 1
            err += "error" in row
            if n % 50 == 0 or n == len(jobs):
                print(
                    f"[pilot_human_decks] {n}/{len(jobs)} fights, {err} unbuildable, "
                    f"{time.time() - t0:.0f}s",
                    flush=True,
                )
    return 0


# ── report ───────────────────────────────────────────────────────────────


def report(args: argparse.Namespace) -> int:
    t = tagged_cards(args.pool)
    tags = set(args.focus.split(",")) if args.focus else t["exhaust"] | t["selfdmg"]
    fights = {f"{f['file']}:{f['floor']}": f for f in map(json.loads, open(args.fights))}
    played = [r for r in map(json.loads, open(args.played)) if "error" not in r]
    errors = sum(1 for r in map(json.loads, open(args.played)) if "error" in r)
    print(f"fights played {len(played)}, unbuildable {errors}")
    print(f"exhaust ({len(t['exhaust'])}): {sorted(t['exhaust'])}")
    print(f"self-damage ({len(t['selfdmg'])}): {sorted(t['selfdmg'])}\n")

    def loss(f: dict[str, Any], res: list[dict[str, Any]]) -> float:
        return statistics.mean(f["hp_before"] - r["hp_after"] for r in res)

    rows = []
    for r in played:
        f = fights[r["key"]]
        if f.get("human_hp_after") is None or f.get("hp_before") is None:
            continue
        n_tag = sum(1 for c in f["deck"] if c.split("+")[0] in tags)
        rows.append((f, r, n_tag))

    def deficit_table(sel: list[tuple[dict[str, Any], dict[str, Any], int]]) -> None:
        print(
            f"  {'tagged':>8s} {'fights':>7s} {'human loss':>11s} {'(dmg taken)':>11s} "
            f"{'pilot loss':>11s} {'deficit':>8s} {'pilot death':>12s}"
        )
        for lo, hi in ((0, 0), (1, 1), (2, 3), (4, 99)):
            b = [x for x in sel if lo <= x[2] <= hi]
            if not b:
                continue
            h = [f["hp_before"] - f["human_hp_after"] for f, _, _ in b]
            hd = [f["human_damage_taken"] or 0 for f, _, _ in b]
            p = [loss(f, r["arms"]["orig"]) for f, r, _ in b]
            dd = statistics.mean(
                sum(x["died"] for x in r["arms"]["orig"]) / len(r["arms"]["orig"]) for _, r, _ in b
            )
            lab = f"{lo}" if lo == hi else f"{lo}-{hi if hi < 99 else '+'}"
            print(
                f"  {lab:>8s} {len(b):7d} {statistics.mean(h):11.1f} {statistics.mean(hd):11.1f} "
                f"{statistics.mean(p):11.1f} "
                f"{statistics.mean(p) - statistics.mean(h):+8.1f} {dd:12.1%}"
            )

    # Decks with more tagged cards come later in a run and meet harder fights,
    # so the trend is read WITHIN a room type, not pooled.
    for cohort in ("pro", "other", None):
        for room in ("monster", "elite", "boss"):
            sel = [
                x
                for x in rows
                if (cohort is None or x[0]["cohort"] == cohort)
                and x[0]["room_type"] == room
                and x[0]["n_enchanted"] == 0
            ]
            if sel:
                print(
                    f"=== cohort {cohort or 'all'}, {room}: pilot HP loss minus human's, same fight ==="
                )
                deficit_table(sel)
        print()
    # GH #810: Regent / Necrobinder lose their runs in act-2 hallway fights,
    # so read the general gap by act as well as by room. `dmg` vs the human's
    # `damage_taken` is the heal-free comparison.
    print("=== by act x room (all cohorts, un-enchanted): pilot vs human, same fight ===")
    print(
        f"  {'act':>3s} {'room':>8s} {'fights':>7s} {'human loss':>11s} {'pilot loss':>11s} "
        f"{'deficit':>8s} {'human dmg':>10s} {'pilot dmg':>10s} {'pilot death':>12s}"
    )
    for act in (1, 2, 3):
        for room in ("monster", "elite", "boss"):
            b = [
                (f, r)
                for f, r, _ in rows
                if f["act"] == act and f["room_type"] == room and f["n_enchanted"] == 0
            ]
            if not b:
                continue
            h = statistics.mean(f["hp_before"] - f["human_hp_after"] for f, _ in b)
            p = statistics.mean(loss(f, r["arms"]["orig"]) for f, r in b)
            hd = statistics.mean(f["human_damage_taken"] or 0 for f, _ in b)
            pd = statistics.mean(statistics.mean(x["dmg"] for x in r["arms"]["orig"]) for _, r in b)
            dd = statistics.mean(
                sum(x["died"] for x in r["arms"]["orig"]) / len(r["arms"]["orig"]) for _, r in b
            )
            print(
                f"  {act:3d} {room:>8s} {len(b):7d} {h:11.1f} {p:11.1f} {p - h:+8.1f} "
                f"{hd:10.1f} {pd:10.1f} {dd:12.1%}"
            )
    print()
    print(
        "=== robustness: all cohorts, all rooms, INCLUDING enchanted decks (enchantments not simulated) ==="
    )
    deficit_table(rows)
    print()

    print("=== within-deck value to the pilot: HP saved per card vs a Strike (orig vs ablated) ===")
    for arm in ("abl_tag", "abl_ctl"):
        vals = []
        for f, r, n_tag in rows:
            if arm in r["arms"] and f["n_enchanted"] == 0 and n_tag:
                vals.append((loss(f, r["arms"][arm]) - loss(f, r["arms"]["orig"])) / n_tag)
        if vals:
            se = statistics.stdev(vals) / len(vals) ** 0.5 if len(vals) > 1 else float("nan")
            print(
                f"{arm:8s} n={len(vals):5d}  HP saved per card: {statistics.mean(vals):+.2f} (SE {se:.2f})"
            )
    paired = [
        (loss(f, r["arms"]["abl_tag"]) - loss(f, r["arms"]["abl_ctl"])) / n_tag
        for f, r, n_tag in rows
        if "abl_tag" in r["arms"] and "abl_ctl" in r["arms"] and f["n_enchanted"] == 0
    ]
    if len(paired) > 1:
        se = statistics.stdev(paired) / len(paired) ** 0.5
        print(
            f"paired (abl_tag - abl_ctl) per card, n={len(paired)}: {statistics.mean(paired):+.2f} "
            f"(SE {se:.2f})  [>0: tagged cards worth MORE to the pilot than other cards]"
        )
    by_card: dict[str, list[float]] = collections.defaultdict(list)
    for f, r, n_tag in rows:
        if "abl_tag" in r["arms"] and f["n_enchanted"] == 0 and n_tag == 1:
            card = next(c.split("+")[0] for c in f["deck"] if c.split("+")[0] in tags)
            by_card[card].append(loss(f, r["arms"]["abl_tag"]) - loss(f, r["arms"]["orig"]))
    print("\nsingle-tagged-card decks: HP saved by the card vs a Strike")
    for card, v in sorted(by_card.items(), key=lambda kv: -len(kv[1])):
        if len(v) >= 10:
            print(f"  {card:22s} n={len(v):4d}  {statistics.mean(v):+.2f}")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("extract")
    e.add_argument("--index", required=True, help="ppo.human_players index JSONL")
    e.add_argument("--users", default=None, help="JSON list of usernames (cohort 'pro')")
    e.add_argument("--sample-others", type=int, default=300)
    e.add_argument("--sample-seed", type=int, default=0)
    e.add_argument("--ascension", type=int, default=10)
    e.add_argument("--character", default="Ironclad", choices=sorted(STARTERS))
    e.add_argument("--build", default="v0.107.1")
    e.add_argument("--out", required=True)
    p = sub.add_parser("play")
    p.add_argument("--fights", required=True)
    p.add_argument("--reps", type=int, default=2)
    p.add_argument("--procs", type=int, default=40)
    p.add_argument("--out", required=True)
    p.add_argument("--pool", default=None, help="JSON list: the character's card-reward pool")
    p.add_argument(
        "--focus", default=None, help="comma list: ablate THESE cards instead of the tags"
    )
    p.add_argument("--only-tagged", action="store_true", help="only un-enchanted decks holding one")
    p.add_argument("--sims", type=int, default=1000)
    p.add_argument("--boss-sims", type=int, default=4000)
    p.add_argument("--clair", action="store_true", help="clairvoyant search (STS2_RUNAI_HONEST=0)")
    p.add_argument(
        "--arms", default=None, help="comma list of arms to play (default: all the deck has)"
    )
    p.add_argument(
        "--escalate",
        default=None,
        help="RULE:THRESHOLD:SIMS:BOSS_SIMS — re-search decisions resolved below THRESHOLD "
        "(rule z or share, see _resolution) with a stronger agent",
    )
    p.add_argument(
        "--seed-by",
        choices=("key", "index"),
        default="key",
        help="per-fight seed from the fight key (stable across files) or its index",
    )
    r = sub.add_parser("report")
    r.add_argument("--fights", required=True)
    r.add_argument("--played", required=True)
    r.add_argument("--pool", default=None, help="same --pool as play")
    r.add_argument("--focus", default=None, help="same --focus as play")
    args = ap.parse_args(argv)
    return {"extract": extract, "play": play, "report": report}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
