"""Human-readable diff between two consecutive game states.

The workbench event log (Play and Replay) shows what an action *did* — damage
dealt/taken, block, buffs, deaths, and resource changes — by comparing the state
before an action with the state after it. This module is the single place those
effect lines are derived. Functions are pure and defensive: a missing key skips
a line rather than raising.
"""

from __future__ import annotations

from collections import Counter
from typing import Any

_MINUS = "−"  # real minus glyph, for aligned signs
_DEAD = "☠ defeated"


def _signed(delta: int) -> str:
    """Signed magnitude like ``(-6)`` / ``(+16)`` using a real minus glyph."""
    return f"({_MINUS if delta < 0 else '+'}{abs(delta)})"


def _player_hp_block(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    p0 = prev.get("player") or {}
    p1 = nxt.get("player") or {}
    h0, h1 = p0.get("hp"), p1.get("hp")
    if isinstance(h0, int) and isinstance(h1, int) and h0 != h1:
        out.append(f"You HP {h0}→{h1} {_signed(h1 - h0)}")
    b0, b1 = p0.get("block", 0), p1.get("block", 0)
    if b0 != b1:
        out.append(f"You Block {b0}→{b1}")


def _enemy_lines(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    nxt_by_idx = {e.get("index"): e for e in (nxt.get("enemies") or [])}
    for e0 in prev.get("enemies") or []:
        idx = e0.get("index")
        name = e0.get("name", f"enemy {idx}")
        h0 = e0.get("hp", 0)
        e1 = nxt_by_idx.get(idx)
        if e1 is None:
            # A live enemy vanishing = defeated; an already-0 corpse being cleaned up is silent.
            if h0 > 0:
                out.append(f"{name} HP {h0}→0  {_DEAD}")
            continue
        h1 = e1.get("hp", 0)
        if h0 != h1:
            line = f"{name} HP {h0}→{h1} {_signed(h1 - h0)}"
            if h1 <= 0:
                line += f"  {_DEAD}"
            out.append(line)
        b0, b1 = e0.get("block", 0), e1.get("block", 0)
        if b0 != b1:
            out.append(f"{name} Block {b0}→{b1}")


def _diff_powers(
    who: str,
    prev_powers: list[dict[str, Any]] | None,
    nxt_powers: list[dict[str, Any]] | None,
    out: list[str],
) -> None:
    p0 = {p.get("name"): p.get("amount", 0) for p in (prev_powers or [])}
    p1 = {p.get("name"): p.get("amount", 0) for p in (nxt_powers or [])}
    for name, amt in p1.items():
        if name not in p0:
            if amt:
                # Render the magnitude's sign properly so a debuff gained at a
                # negative amount reads "You −1 Shrink", not "You +-1 Shrink".
                sign = "+" if amt > 0 else _MINUS
                out.append(f"{who} {sign}{abs(amt)} {name}")
            else:
                out.append(f"{who} +{name}")
        elif amt != p0[name]:
            out.append(f"{who} {name} {p0[name]}→{amt}")
    for name in p0:
        if name not in p1:
            out.append(f"{who} {_MINUS}{name}")


def _power_lines(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    _diff_powers("You", prev.get("player_powers"), nxt.get("player_powers"), out)
    prev_es = {e.get("index"): e for e in (prev.get("enemies") or [])}
    for e1 in nxt.get("enemies") or []:
        e0 = prev_es.get(e1.get("index"))
        if e0 is None:
            continue
        name = e1.get("name", f"enemy {e1.get('index')}")
        _diff_powers(name, e0.get("powers"), e1.get("powers"), out)


def _spawn_lines(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    prev_idx = {e.get("index") for e in (prev.get("enemies") or [])}
    for e in nxt.get("enemies") or []:
        if e.get("index") not in prev_idx:
            out.append(f"Spawned: {e.get('name', 'enemy')}")


def _resource_lines(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    p0 = prev.get("player") or {}
    p1 = nxt.get("player") or {}
    g0, g1 = p0.get("gold"), p1.get("gold")
    if isinstance(g0, int) and isinstance(g1, int) and g0 != g1:
        out.append(f"Gold {g0}→{g1}")
    r0 = {r.get("name") for r in (p0.get("relics") or [])}
    for r in p1.get("relics") or []:
        if r.get("name") not in r0:
            out.append(f"+Relic {r.get('name')}")
    pot0 = Counter(pt.get("name") for pt in (p0.get("potions") or []))
    pot1 = Counter(pt.get("name") for pt in (p1.get("potions") or []))
    for name, n in (pot1 - pot0).items():
        out.extend([f"+Potion {name}"] * n)
    for name, n in (pot0 - pot1).items():
        out.extend([f"Used {name}"] * n)


def _orb_passive_lines(prev: dict[str, Any], nxt: dict[str, Any], out: list[str]) -> None:
    """Attribute end-of-turn effects to the Defect's orb passives.

    Orb passives fire when the player ends their turn; a ``round`` increment
    marks that boundary. The net HP/block diff above bundles enemy-turn effects
    with the orb passives, so this annotates *what each orb did* (using the
    pre-turn-end queue) to make the numbers traceable. Mechanics follow the C#
    decomp (``…/Models.Orbs``): Lightning hits one random enemy, Glass hits all,
    Frost grants block, Plasma grants energy, Dark stores its evoke value.
    """
    r0, r1 = prev.get("round"), nxt.get("round")
    if not (isinstance(r0, int) and isinstance(r1, int) and r1 > r0):
        return
    orbs = prev.get("orbs") or []
    if not orbs:
        return
    descs: list[str] = []
    for o in orbs:
        kind = (o.get("type") or "").lower()
        passive = o.get("passive", 0)
        if kind == "lightning":
            descs.append(f"Lightning orb passive: {passive} damage to a random enemy")
        elif kind == "frost":
            descs.append(f"Frost orb passive: +{passive} block")
        elif kind == "dark":
            descs.append(f"Dark orb passive: +{passive} stored damage")
        elif kind == "plasma":
            descs.append(f"Plasma orb passive: +{passive} energy")
        elif kind == "glass":
            descs.append(f"Glass orb passive: {passive} damage to all enemies")
        else:
            descs.append(f"{o.get('type', 'Orb')} orb passive")
    # Collapse identical lines (e.g. two Lightning orbs) into a "(×N)" suffix.
    for desc, n in Counter(descs).items():
        out.append(desc if n == 1 else f"{desc} (×{n})")


def diff_states(prev: dict[str, Any], nxt: dict[str, Any]) -> list[str]:
    """Return ordered, human-readable effect lines for the ``prev``→``nxt`` transition.

    Args:
        prev: The state an action was taken from.
        nxt: The state resulting from that action.

    Returns:
        Lines like ``"Nibbit HP 50→44 (−6)"``; empty when nothing notable changed.
    """
    out: list[str] = []
    _player_hp_block(prev, nxt, out)
    _enemy_lines(prev, nxt, out)
    _orb_passive_lines(prev, nxt, out)
    _power_lines(prev, nxt, out)
    _spawn_lines(prev, nxt, out)
    _resource_lines(prev, nxt, out)
    return out
