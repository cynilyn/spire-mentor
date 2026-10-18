"""Per-player index of the spire-codex corpus (GH #777/#778): find strong players.

The average uploaded v0.107.1 Ironclad A10 run wins ~14%, about the bot's level,
so "humans" as a pool is no ceiling. This indexes every single-player standard
run by `username` so strong players can be selected, emitting one row per run:
username, build, ascension, win, start_time, file.

Selecting players by their observed win rate and then reporting those same
runs is a winner's-curse estimate (a 3/5 player looks like a 60% player).
`--select` therefore picks players on runs from builds OTHER than `--eval-build`
and reports their `--eval-build` runs, which played no part in the selection.

    python -m ppo.human_players index --out players.jsonl
    python -m ppo.human_players select --index players.jsonl --min-runs 20
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import math
import os
import sys
from multiprocessing import Pool
from typing import Any

DEFAULT_ROOT = "../../sts2_run_scraper/data/files-spire-codex"


def _row(path: str, character: str) -> dict[str, Any] | None:
    try:
        with gzip.open(path) as f:
            d = json.load(f)
    except (OSError, ValueError):
        return None
    players = d.get("players") or []
    if (
        d.get("game_mode") != "standard"
        or d.get("was_abandoned")
        or len(players) != 1
        or players[0].get("character") != f"CHARACTER.{character.upper()}"
    ):
        return None
    return {
        "username": d.get("username"),
        "build": d.get("build_id"),
        "ascension": d.get("ascension"),
        "win": bool(d.get("win")),
        "start_time": d.get("start_time"),
        "file": path,
    }


def _job(a: tuple[str, str]) -> dict[str, Any] | None:
    return _row(*a)


def index(args: argparse.Namespace) -> int:
    base = os.path.join(args.root, args.character.lower())
    paths = [
        os.path.join(base, m, f)
        for m in sorted(os.listdir(base))
        for f in sorted(os.listdir(os.path.join(base, m)))
    ]
    n = 0
    with Pool(args.procs) as pool, open(args.out, "w") as sink:
        for r in pool.imap_unordered(_job, [(p, args.character) for p in paths], chunksize=64):
            if r is not None:
                sink.write(json.dumps(r) + "\n")
                n += 1
    print(f"[human_players] {len(paths)} files, {n} single-player standard runs -> {args.out}")
    return 0


def _wilson(k: int, n: int, z: float = 1.96) -> tuple[float, float]:
    if n == 0:
        return 0.0, 1.0
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return c - h, c + h


def select(args: argparse.Namespace) -> int:
    rows = [json.loads(line) for line in open(args.index)]
    rows = [r for r in rows if r["username"] and r["ascension"] in args.ascension]
    sel = collections.defaultdict(lambda: [0, 0])
    ev = collections.defaultdict(lambda: [0, 0])
    for r in rows:
        bucket = ev if r["build"] == args.eval_build else sel
        bucket[r["username"]][0] += r["win"]
        bucket[r["username"]][1] += 1
    # Rank on the Wilson LOWER bound of the selection-build win rate: it
    # penalises small samples, so a 3/4 player does not outrank a 40/100 one.
    ranked = sorted(
        ((u, k, n, _wilson(k, n)[0]) for u, (k, n) in sel.items() if n >= args.min_runs),
        key=lambda t: -t[3],
    )
    print(
        f"players with >= {args.min_runs} selection runs at A{args.ascension}: {len(ranked)}; "
        f"eval build {args.eval_build}"
    )
    print(f"{'user':24s} {'sel W/N':>10s} {'sel lb':>7s} {'eval W/N':>10s}")
    for u, k, n, lb in ranked[: args.top]:
        ek, en = ev.get(u, (0, 0))
        print(f"{u[:24]:24s} {k:>4d}/{n:<5d} {lb:7.1%} {ek:>4d}/{en:<5d}")
    for top in (10, 25, 50, 100):
        users = [u for u, *_ in ranked[:top]]
        ek = sum(ev[u][0] for u in users if u in ev)
        en = sum(ev[u][1] for u in users if u in ev)
        lo, hi = _wilson(ek, en)
        rate = ek / en if en else float("nan")
        print(
            f"top-{top:<3d} held-out {args.eval_build} A{args.ascension}: {ek}/{en} = {rate:.1%} "
            f"[{lo:.1%}, {hi:.1%}] from {sum(1 for u in users if u in ev)} players"
        )
    if args.out_users:
        with open(args.out_users, "w") as f:
            json.dump([u for u, *_ in ranked[: args.top]], f)
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("index")
    a.add_argument("--root", default=DEFAULT_ROOT)
    a.add_argument("--character", default="Ironclad")
    a.add_argument("--procs", type=int, default=16)
    a.add_argument("--out", required=True)
    b = sub.add_parser("select")
    b.add_argument("--index", required=True)
    b.add_argument("--ascension", type=int, nargs="+", default=[10])
    b.add_argument("--eval-build", default="v0.107.1")
    b.add_argument("--min-runs", type=int, default=20)
    b.add_argument("--top", type=int, default=30)
    b.add_argument("--out-users", default=None)
    args = ap.parse_args(argv)
    return index(args) if args.cmd == "index" else select(args)


if __name__ == "__main__":
    sys.exit(main())
