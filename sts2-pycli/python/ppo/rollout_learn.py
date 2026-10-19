"""Learn the MCTS playout policy from the search's own decisions.

Every combat simulation scores its tree path by ONE playout to the end of the
fight, so the playout policy judges every candidate move. The deployed one is
a hand-written card-priority list (`sts2_mcts::expert`); in long boss fights
its playouts almost never win, so the search gets no win signal (GH #777
human-bank audit). This module replaces it with a policy learned from the
search itself (expert iteration):

    gen    play human-bank fights with the pilot; at every searched decision
           record the playout features of each legal action
           (`sts2_rs.rollout_features`, computed by the same Rust code the
           playout runs) and the root's visit counts.
    train  fit the factorized softmax of `sts2_mcts::rollout_net` to the
           visit distributions and write its weights as JSON.

Honest-mode searches already play the built-in Ironclad net
(`sts2-rs/data/rollout_nets/ironclad.json`, `RolloutPolicyKind::Net`). To try
new weights, set `STS2_ROLLOUT_NET=<json>`; it replaces the built-in net for
every character. `STS2_ROLLOUT_TEMP` overrides that net's softmax temperature
(never the built-in net's), and `STS2_RUNAI_MCTS_OVERRIDES=rollout_policy=heuristic` restores the hand-written
playout.

Training rows exclude every run in `--holdout` (fight files whose runs are
used to evaluate), so evaluation decks are never trained on.
"""

from __future__ import annotations

import argparse
import collections
import json
import math
import os
import random
import time
from multiprocessing import get_context
from typing import Any

import numpy as np

from ppo.pilot_human_decks import _fight_seed

ROOMS = {"monster": 0, "elite": 1, "boss": 2}
STATE_DIM = 32
ACTION_DIM = 24
HEUR_FEATURE = 21  # -priority / tau, the heuristic playout's own logit


def _canon(action: dict[str, Any]) -> str:
    return json.dumps(action, sort_keys=True)


# ── gen ──────────────────────────────────────────────────────────────────


def gen_fight(job: tuple[Any, ...]) -> dict[str, Any]:
    fight, fight_idx, reps, rep0, sims, boss_sims, clair = job
    import sts2_rs

    if clair:
        os.environ["STS2_RUNAI_HONEST"] = "0"
    key = f"{fight['file']}:{fight['floor']}"
    out: dict[str, Any] = {"key": key, "decisions": [], "unmatched": 0}
    try:
        for rep in range(reps):
            s = _fight_seed(key) * 100 + rep0 + rep
            state = sts2_rs.build_scenario_state(
                s,
                "Ironclad",
                fight["deck"],
                int(fight["hp_before"]),
                int(fight["max_hp"]),
                fight["relics"],
                fight["encounter"],
                ascension=int(fight["ascension"]),
                potions=fight["potions"][:3],
            )
            rm = sts2_rs.RunManager(
                seed=s, character="Ironclad", ascension_level=int(fight["ascension"])
            )
            rm.debug_enter_combat(state, fight["room_type"])
            agent = sts2_rs.MctsCombatAgent(seed=s, sims=sims, boss_sims=boss_sims)
            steps = 0
            while rm.phase() == "combat" and not rm.is_over() and steps < 3000:
                feats = sts2_rs.rollout_features(rm.combat_state())
                rec = agent.step(rm)
                steps += 1
                if rec is None:
                    break
                search = rec.get("combat_search")
                if not search or len(feats["actions"]) < 2:
                    continue
                index = {_canon(a): i for i, a in enumerate(feats["actions"])}
                visits = [0.0] * len(feats["actions"])
                wins = total = 0
                for a in search["actions"]:
                    i = index.get(_canon(a["action"]))
                    if i is None:
                        out["unmatched"] += 1
                        continue
                    visits[i] += a["visits"]
                    wins += a["wins"]
                    total += a["visits"]
                if not total:
                    continue
                chosen = index.get(_canon(search.get("recommended") or {}), -1)
                out["decisions"].append(
                    {
                        "state": feats["state"],
                        "keys": feats["keys"],
                        "x": feats["x"],
                        "visits": visits,
                        "chosen": chosen,
                        "pwin": wins / total,
                        "room": ROOMS.get(fight["room_type"], 0),
                        "fight": fight_idx,
                    }
                )
    except Exception as e:
        out["error"] = repr(e)
    return out


class ShardWriter:
    """Accumulates decisions and writes `.npz` shards; keys become vocab ids."""

    def __init__(self, out_dir: str, per_shard: int) -> None:
        self.out_dir = out_dir
        self.per_shard = per_shard
        os.makedirs(out_dir, exist_ok=True)
        vocab_path = os.path.join(out_dir, "vocab.json")
        self.vocab: dict[str, int] = (
            json.load(open(vocab_path)) if os.path.exists(vocab_path) else {"<unk>": 0}
        )
        self.n_shard = len([f for f in os.listdir(out_dir) if f.startswith("shard-")])
        self.buf: list[dict[str, Any]] = []

    def add(self, decisions: list[dict[str, Any]]) -> None:
        self.buf.extend(decisions)
        if len(self.buf) >= self.per_shard:
            self.flush()

    def flush(self) -> None:
        if not self.buf:
            return
        d = self.buf
        ids = [self.vocab.setdefault(k, len(self.vocab)) for r in d for k in r["keys"]]
        np.savez_compressed(
            os.path.join(self.out_dir, f"shard-{self.n_shard:04d}.npz"),
            state=np.asarray([r["state"] for r in d], np.float32),
            n_act=np.asarray([len(r["keys"]) for r in d], np.int16),
            x=np.asarray([x for r in d for x in r["x"]], np.float32),
            key=np.asarray(ids, np.int32),
            visits=np.asarray([v for r in d for v in r["visits"]], np.float32),
            chosen=np.asarray([r["chosen"] for r in d], np.int16),
            pwin=np.asarray([r["pwin"] for r in d], np.float32),
            room=np.asarray([r["room"] for r in d], np.int8),
            fight=np.asarray([r["fight"] for r in d], np.int32),
        )
        json.dump(self.vocab, open(os.path.join(self.out_dir, "vocab.json"), "w"))
        self.n_shard += 1
        self.buf = []


def gen(args: argparse.Namespace) -> int:
    fights = [json.loads(line) for line in open(args.fights)]
    holdout: set[str] = set()
    for path in args.holdout or []:
        holdout |= {json.loads(line)["file"] for line in open(path)}
    if args.holdout_encounter:
        holdout |= {f["file"] for f in fights if f["encounter"] in args.holdout_encounter}
    rooms = set(args.rooms.split(","))
    pool = [
        (i, f) for i, f in enumerate(fights) if f["file"] not in holdout and f["room_type"] in rooms
    ]
    rng = random.Random(args.sample_seed)
    if args.max_monster:
        mon = [p for p in pool if p[1]["room_type"] == "monster"]
        keep = {i for i, _ in rng.sample(mon, min(args.max_monster, len(mon)))}
        pool = [p for p in pool if p[1]["room_type"] != "monster" or p[0] in keep]
    rng.shuffle(pool)
    done_path = os.path.join(args.out_dir, "done.txt")
    done = set(open(done_path).read().split()) if os.path.exists(done_path) else set()
    jobs = [
        (f, i, args.reps, args.rep0, args.sims, args.boss_sims, args.clair)
        for i, f in pool
        if f"{f['file']}:{f['floor']}" not in done
    ]
    print(
        f"[rollout_learn] {len(jobs)} fights to play ({len(holdout)} held-out runs, "
        f"rooms {sorted(rooms)})",
        flush=True,
    )
    writer = ShardWriter(args.out_dir, args.per_shard)
    t0 = time.time()
    n = n_dec = err = unmatched = 0
    with get_context("spawn").Pool(args.procs) as pool_, open(done_path, "a") as done_f:
        for row in pool_.imap_unordered(gen_fight, jobs):
            n += 1
            err += "error" in row
            unmatched += row["unmatched"]
            n_dec += len(row["decisions"])
            writer.add(row["decisions"])
            done_f.write(row["key"] + "\n")
            done_f.flush()
            if n % 200 == 0 or n == len(jobs):
                print(
                    f"[rollout_learn] {n}/{len(jobs)} fights, {n_dec} decisions, {err} errors, "
                    f"{unmatched} unmatched root actions, {time.time() - t0:.0f}s",
                    flush=True,
                )
    writer.flush()
    print("DONE", flush=True)
    return 0


# ── train ────────────────────────────────────────────────────────────────


def load_shards(data_dirs: list[str]) -> tuple[dict[str, np.ndarray], list[str]]:
    """Concatenate shards from several gen runs into one vocab space."""
    vocab: dict[str, int] = {"<unk>": 0}
    parts: dict[str, list[np.ndarray]] = collections.defaultdict(list)
    fight_offset = 0
    for d in data_dirs:
        local = json.load(open(os.path.join(d, "vocab.json")))
        remap = np.zeros(max(local.values()) + 1, np.int32)
        for k, i in local.items():
            remap[i] = vocab.setdefault(k, len(vocab))
        max_fight = 0
        for f in sorted(os.listdir(d)):
            if not f.startswith("shard-"):
                continue
            z = np.load(os.path.join(d, f))
            for name in z.files:
                a = z[name]
                if name == "key":
                    a = remap[a]
                elif name == "fight":
                    max_fight = max(max_fight, int(a.max()) + 1)
                    a = a + fight_offset
                parts[name].append(a)
        fight_offset += max_fight
    data = {k: np.concatenate(v) for k, v in parts.items()}
    keys = [k for k, _ in sorted(vocab.items(), key=lambda kv: kv[1])]
    return data, keys


def _spread_duplicates(
    key: np.ndarray, x: np.ndarray, visits: np.ndarray, starts: np.ndarray, n_act: np.ndarray
) -> np.ndarray:
    """Copies of one card share features and so logits; the search keeps one
    child for them (the transposition table), so its visits sit on one copy.
    Spread each group's mass evenly over its members."""
    out = visits.copy()
    for s, n in zip(starts, n_act, strict=True):
        if n < 2:
            continue
        groups: dict[bytes, list[int]] = collections.defaultdict(list)
        for j in range(s, s + n):
            groups[key[j].tobytes() + x[j].tobytes()].append(j)
        for members in groups.values():
            if len(members) > 1:
                out[members] = visits[members].sum() / len(members)
    return out


def _playout_net(torch: Any, hidden: int, emb_dim: int, n_keys: int) -> Any:
    """The torch twin of `sts2_mcts::rollout_net::RolloutNet`."""

    class PlayoutNet(torch.nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.hidden, self.emb_dim = hidden, emb_dim
            self.mlp = torch.nn.Sequential(
                torch.nn.Linear(STATE_DIM, hidden),
                torch.nn.ReLU(),
                torch.nn.Linear(hidden, emb_dim),
            )
            self.emb = torch.nn.Embedding(n_keys, emb_dim)
            self.bias = torch.nn.Embedding(n_keys, 1)
            self.proj = torch.nn.Linear(ACTION_DIM, emb_dim, bias=False)
            self.lin = torch.nn.Linear(ACTION_DIM, 1, bias=False)

        def forward(self, S: Any, X: Any, K: Any, M: Any) -> Any:
            """Logits `[B, A]` for states `S [B, STATE_DIM]`, action features
            `X [B, A, ACTION_DIM]`, key rows `K [B, A]`; `M` masks real actions."""
            q = self.mlp(S)
            k = self.emb(K) + self.proj(X)
            z = (k * q[:, None, :]).sum(-1) + self.bias(K)[..., 0] + self.lin(X)[..., 0]
            return z.masked_fill(~M, -1e9)

        def to_json(
            self,
            keys: list[str],
            temperature: float,
            meta: dict | None = None,
            no_early_end_turn: bool = False,
        ) -> dict:
            w = lambda t: t.detach().cpu().numpy().astype(float).round(6).tolist()  # noqa: E731
            return {
                "state_dim": STATE_DIM,
                "action_dim": ACTION_DIM,
                "hidden": self.hidden,
                "emb_dim": self.emb_dim,
                "temperature": temperature,
                # The playout rule the weights were trained under: the search
                # loads it from the file, so a net trained with early EndTurn
                # masked must not deploy with it allowed.
                "no_early_end_turn": no_early_end_turn,
                "keys": keys,
                "w1": w(self.mlp[0].weight),
                "b1": w(self.mlp[0].bias),
                "w2": w(self.mlp[2].weight),
                "b2": w(self.mlp[2].bias),
                "emb": w(self.emb.weight),
                "bias": w(self.bias.weight[:, 0]),
                "proj": w(self.proj.weight),
                "lin": w(self.lin.weight[0]),
                "meta": meta or {},
            }

    return PlayoutNet()


def _load_playout_net(torch: Any, net: dict[str, Any]) -> Any:
    m = _playout_net(torch, net["hidden"], net["emb_dim"], len(net["keys"]))
    t = lambda a: torch.as_tensor(np.asarray(a, np.float32))  # noqa: E731
    with torch.no_grad():
        m.mlp[0].weight.copy_(t(net["w1"]))
        m.mlp[0].bias.copy_(t(net["b1"]))
        m.mlp[2].weight.copy_(t(net["w2"]))
        m.mlp[2].bias.copy_(t(net["b2"]))
        m.emb.weight.copy_(t(net["emb"]))
        m.bias.weight.copy_(t(net["bias"])[:, None])
        m.proj.weight.copy_(t(net["proj"]))
        m.lin.weight.copy_(t(net["lin"])[None, :])
    return m


def train(args: argparse.Namespace) -> int:
    import torch

    data, keys = load_shards(args.data)
    n_dec = len(data["n_act"])
    n_act = data["n_act"].astype(np.int64)
    starts = np.concatenate([[0], np.cumsum(n_act)[:-1]])
    # Root visits are near-uniform at the deployed c_puct (GH #442: top child
    # ~1.06x uniform), so they say little; the search's recommended action
    # carries its verdict. `--target-mix` blends the two.
    chosen = np.zeros_like(data["visits"])
    has_choice = data["chosen"] >= 0
    chosen[starts[has_choice] + data["chosen"][has_choice]] = 1.0
    visits = data["visits"] / np.maximum(np.add.reduceat(data["visits"], starts), 1e-9).repeat(
        n_act
    )
    mix = np.where(has_choice.repeat(n_act), args.target_mix, 1.0)
    target = _spread_duplicates(
        data["key"], data["x"], (1.0 - mix) * chosen + mix * visits, starts, n_act
    )
    print(f"[rollout_learn] {n_dec} decisions, {len(target)} actions, {len(keys)} keys", flush=True)

    # Pad to [decisions, A_max].
    a_max = int(n_act.max())
    col = np.arange(len(target)) - np.repeat(starts, n_act)
    row = np.repeat(np.arange(n_dec), n_act)
    pad = lambda v, fill, dt: np.full((n_dec, a_max, *v.shape[1:]), fill, dt)  # noqa: E731
    X = pad(data["x"], 0.0, np.float32)
    X[row, col] = data["x"]
    K = pad(data["key"], 0, np.int64)
    K[row, col] = data["key"]
    T = pad(target, 0.0, np.float32)
    T[row, col] = target
    T /= np.maximum(T.sum(1, keepdims=True), 1e-9)
    M = np.zeros((n_dec, a_max), bool)
    M[row, col] = True
    C = data["chosen"].astype(np.int64)

    fights = np.unique(data["fight"])
    rng = np.random.default_rng(args.seed)
    val_f = set(
        rng.choice(fights, max(1, int(len(fights) * args.val_frac)), replace=False).tolist()
    )
    is_val = np.array([f in val_f for f in data["fight"]])
    dev = torch.device(args.device)
    tt = lambda a: torch.as_tensor(a, device=dev)  # noqa: E731
    S_, X_, K_, T_, M_, C_ = map(tt, (data["state"], X, K, T, M, C))
    R_ = tt(data["room"].astype(np.int64))
    tr_idx = torch.as_tensor(np.flatnonzero(~is_val), device=dev)
    va_idx = torch.as_tensor(np.flatnonzero(is_val), device=dev)

    torch.manual_seed(args.seed)
    net = _playout_net(torch, args.hidden, args.emb_dim, len(keys)).to(dev)
    for p in (net.emb.weight, net.bias.weight, net.proj.weight):
        torch.nn.init.normal_(p, std=0.01)
    with torch.no_grad():
        # Start AT the heuristic playout: logit = -priority / tau.
        net.lin.weight.zero_()
        net.lin.weight[0, HEUR_FEATURE] = 1.0
    opt = torch.optim.AdamW(net.parameters(), lr=args.lr, weight_decay=args.wd)

    def logits(idx: torch.Tensor) -> torch.Tensor:
        return net(S_[idx], X_[idx], K_[idx], M_[idx])

    def metrics(idx: torch.Tensor, heuristic: bool = False) -> dict[str, float]:
        out: dict[str, list[float]] = collections.defaultdict(list)
        with torch.no_grad():
            for b in torch.split(idx, 65536):
                z = X_[b][..., HEUR_FEATURE].masked_fill(~M_[b], -1e9) if heuristic else logits(b)
                lp = torch.log_softmax(z, -1)
                ce = -(T_[b] * lp).sum(-1)
                has = C_[b] >= 0
                top1 = (z.argmax(-1) == C_[b]).float()
                out["ce"].append(ce.sum().item())
                out["top1"].append(top1[has].sum().item())
                out["n"].append(float(len(b)))
                out["n_ch"].append(has.sum().item())
                for r, name in ((2, "boss"),):
                    sel = R_[b] == r
                    out[f"ce_{name}"].append(ce[sel].sum().item())
                    out[f"n_{name}"].append(sel.sum().item())
        n, n_ch = sum(out["n"]), max(1.0, sum(out["n_ch"]))
        return {
            "ce": sum(out["ce"]) / n,
            "top1": sum(out["top1"]) / n_ch,
            "ce_boss": sum(out["ce_boss"]) / max(1.0, sum(out["n_boss"])),
        }

    base = metrics(va_idx, heuristic=True)
    print(f"[rollout_learn] heuristic on val: {base}", flush=True)
    steps_per_epoch = math.ceil(len(tr_idx) / args.batch)
    for epoch in range(args.epochs):
        perm = tr_idx[torch.randperm(len(tr_idx), device=dev)]
        tot = 0.0
        for b in torch.split(perm, args.batch):
            lp = torch.log_softmax(logits(b), -1)
            loss = -(T_[b] * lp).sum(-1).mean()
            opt.zero_grad()
            loss.backward()
            opt.step()
            tot += loss.item()
        print(
            f"[rollout_learn] epoch {epoch + 1}: train ce {tot / steps_per_epoch:.4f}  "
            f"val {metrics(va_idx)}",
            flush=True,
        )

    meta = {"data": args.data, "decisions": n_dec, "val_heuristic": base, "val": metrics(va_idx)}
    json.dump(net.to_json(keys, args.temperature, meta), open(args.out, "w"))
    print(f"[rollout_learn] wrote {args.out}", flush=True)
    return 0


# ── rl ───────────────────────────────────────────────────────────────────


def _combat_states(sts2_rs: Any, fights: list[dict[str, Any]]) -> list[tuple[str, Any]]:
    """Each fight's opening combat state (same build as `gen`); modded runs skipped."""
    out = []
    for f in fights:
        key = f"{f['file']}:{f['floor']}"
        s = _fight_seed(key) * 100 + 77
        try:
            st = sts2_rs.build_scenario_state(
                s,
                "Ironclad",
                f["deck"],
                int(f["hp_before"]),
                int(f["max_hp"]),
                f["relics"],
                f["encounter"],
                ascension=int(f["ascension"]),
                potions=f["potions"][:3],
            )
        except ValueError:
            continue
        rm = sts2_rs.RunManager(seed=s, character="Ironclad", ascension_level=int(f["ascension"]))
        rm.debug_enter_combat(st, f["room_type"])
        out.append((key, rm.combat_state()))
    return out


def _unpack_playouts(d: dict[str, bytes]) -> dict[str, np.ndarray]:
    u = lambda k, dt: np.frombuffer(d[k], dt).copy()  # noqa: E731  (writable for torch)
    return {
        "steps": u("steps", np.uint32).astype(np.int64),
        "state": u("state", np.float32).reshape(-1, STATE_DIM),
        "n_act": u("n_act", np.uint8).astype(np.int64),
        "chosen": u("chosen", np.uint8).astype(np.int64),
        "rows": u("rows", np.uint32).astype(np.int64),
        "x": u("x", np.float32).reshape(-1, ACTION_DIM),
        "allowed": u("allowed", np.uint8).astype(bool),
        "won": u("won", np.uint8).astype(bool),
        "hp_frac": u("hp_frac", np.float32),
        "removed": u("removed", np.float32),
        "round": u("round", np.float32),
    }


def rl(args: argparse.Namespace) -> int:
    """REINFORCE on playout outcomes: play many playouts from human-bank fight
    starts with the current policy, and raise the log-probability of the
    decisions in playouts that did better than their fight's other playouts."""
    import sts2_rs
    import torch

    dev = torch.device(args.device)
    init = json.load(open(args.init))
    keys = init["keys"]
    net = _load_playout_net(torch, init).to(dev)
    opt = torch.optim.Adam(net.parameters(), lr=args.lr)
    fights = [json.loads(line) for line in open(args.fights)]
    holdout: set[str] = set()
    for path in args.holdout or []:
        holdout |= {json.loads(line)["file"] for line in open(path)}
    if args.holdout_encounter:
        holdout |= {f["file"] for f in fights if f["encounter"] in args.holdout_encounter}
    rooms = set(args.rooms.split(","))
    pool = [f for f in fights if f["file"] not in holdout and f["room_type"] in rooms]
    random.Random(args.seed).shuffle(pool)
    t0 = time.time()
    states = _combat_states(sts2_rs, pool[: args.n_states])
    evals = []
    for path in args.eval_fights:
        evals.append(
            (
                os.path.basename(path),
                _combat_states(sts2_rs, [json.loads(line) for line in open(path)]),
            )
        )
    print(
        f"[rollout_learn rl] {len(states)} training starts, eval "
        f"{[(n, len(e)) for n, e in evals]}, built in {time.time() - t0:.0f}s",
        flush=True,
    )
    rng = np.random.default_rng(args.seed)
    settings = {
        k: v
        for k, v in vars(args).items()
        if isinstance(v, (str, int, float, bool, list, type(None)))
    }

    def evaluate(net_json: str, it: int) -> None:
        lp = sts2_rs.LearnedPlayout(net_json)
        for name, ev in evals:
            row = []
            for temp, ne in ((1.0, False), (0.25, True)):
                jobs = [st for _, st in ev for _ in range(args.eval_reps)]
                seeds = [1_000_003 * i + 17 for i in range(len(jobs))]
                won = np.frombuffer(
                    lp.play_many(jobs, seeds, temperature=temp, no_early_end=ne)["won"], np.uint8
                )
                row.append(f"T={temp}{' ne' if ne else ''} {100 * won.mean():.1f}%")
            print(f"[rollout_learn rl] it {it} eval {name}: {'  '.join(row)}", flush=True)

    for it in range(args.iters + 1):
        net_json = json.dumps(net.to_json(keys, 1.0))
        if it % args.eval_every == 0:
            evaluate(net_json, it)
            out = net.to_json(
                keys,
                args.temperature,
                meta={"rl": settings, "iteration": it},
                no_early_end_turn=args.no_early_end,
            )
            json.dump(out, open(args.out, "w"))
        if it == args.iters:
            break
        lp = sts2_rs.LearnedPlayout(net_json)
        idx = rng.choice(len(states), min(args.batch_states, len(states)), replace=False)
        jobs = [states[i][1] for i in idx for _ in range(args.k)]
        seeds = [int(v) for v in rng.integers(0, 2**62, len(jobs))]
        p = _unpack_playouts(
            lp.play_many(jobs, seeds, temperature=1.0, no_early_end=args.no_early_end)
        )
        r = np.where(p["won"], 1.0 + args.w_hp * p["hp_frac"], args.w_removed * p["removed"])
        g = r.reshape(len(idx), args.k)
        adv = (g - (g.sum(1, keepdims=True) - g) / (args.k - 1)).reshape(-1)
        adv = adv / (adv.std() + 1e-6)
        step_adv = np.repeat(adv, p["steps"])
        n_dec = len(p["n_act"])
        a_max = int(p["n_act"].max())
        starts = np.concatenate([[0], np.cumsum(p["n_act"])[:-1]])
        col = np.arange(len(p["rows"])) - np.repeat(starts, p["n_act"])
        row = np.repeat(np.arange(n_dec), p["n_act"])
        X = np.zeros((n_dec, a_max, ACTION_DIM), np.float32)
        X[row, col] = p["x"]
        K = np.zeros((n_dec, a_max), np.int64)
        K[row, col] = p["rows"]
        M = np.zeros((n_dec, a_max), bool)
        M[row, col] = p["allowed"]
        tt = lambda a: torch.as_tensor(a, device=dev)  # noqa: E731
        S_, X_, K_, M_, C_, A_ = map(
            tt, (p["state"], X, K, M, p["chosen"], step_adv.astype(np.float32))
        )
        perm = torch.randperm(n_dec, device=dev)
        ent_tot = loss_tot = 0.0
        for b in torch.split(perm, args.minibatch):
            logp = torch.log_softmax(net(S_[b], X_[b], K_[b], M_[b]), -1)
            lp_ch = logp.gather(1, C_[b][:, None])[:, 0]
            ent = -(logp.exp() * logp.masked_fill(~M_[b], 0.0)).sum(-1)
            loss = -(A_[b] * lp_ch).mean() - args.ent * ent.mean()
            opt.zero_grad()
            loss.backward()
            opt.step()
            ent_tot += ent.mean().item()
            loss_tot += loss.item()
        nb = math.ceil(n_dec / args.minibatch)
        print(
            f"[rollout_learn rl] it {it + 1}: playout win {100 * p['won'].mean():.1f}%  reward "
            f"{r.mean():.3f}  decisions/playout {n_dec / len(jobs):.1f}  entropy {ent_tot / nb:.3f}  "
            f"loss {loss_tot / nb:+.4f}  {time.time() - t0:.0f}s",
            flush=True,
        )
    print("DONE", flush=True)
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("gen", help="record search decisions + playout features")
    g.add_argument("--fights", required=True, help="pilot_human_decks extract JSONL")
    g.add_argument("--out-dir", required=True)
    g.add_argument("--holdout", nargs="*", help="fight JSONL files whose runs are never trained on")
    g.add_argument(
        "--holdout-encounter", nargs="*", help="also hold out every run with this encounter"
    )
    g.add_argument("--rooms", default="monster,elite,boss")
    g.add_argument("--max-monster", type=int, default=0, help="subsample monster fights (0 = all)")
    g.add_argument("--sample-seed", type=int, default=0)
    g.add_argument("--reps", type=int, default=1)
    g.add_argument(
        "--rep0", type=int, default=50, help="seed rep offset (evaluation uses reps 0/1)"
    )
    g.add_argument("--sims", type=int, default=1000)
    g.add_argument("--boss-sims", type=int, default=4000)
    g.add_argument("--clair", action="store_true", help="clairvoyant teacher (STS2_RUNAI_HONEST=0)")
    g.add_argument("--procs", type=int, default=40)
    g.add_argument("--per-shard", type=int, default=20000)
    t = sub.add_parser("train", help="fit the playout policy, write its JSON weights")
    t.add_argument("--data", nargs="+", required=True, help="gen output dirs")
    t.add_argument("--out", required=True)
    t.add_argument("--hidden", type=int, default=32)
    t.add_argument("--emb-dim", type=int, default=16)
    t.add_argument("--epochs", type=int, default=8)
    t.add_argument("--batch", type=int, default=4096)
    t.add_argument("--lr", type=float, default=3e-3)
    t.add_argument("--wd", type=float, default=1e-4)
    t.add_argument("--val-frac", type=float, default=0.05)
    t.add_argument("--temperature", type=float, default=1.0)
    t.add_argument(
        "--target-mix",
        type=float,
        default=0.2,
        help="weight of the root visit distribution in the target; the rest is the "
        "search's recommended action (1.0 = visits only)",
    )
    t.add_argument("--seed", type=int, default=0)
    t.add_argument("--device", default="cuda")
    r = sub.add_parser("rl", help="improve a playout policy by REINFORCE on playout outcomes")
    r.add_argument("--init", required=True, help="starting weights (JSON from train)")
    r.add_argument("--out", required=True)
    r.add_argument("--fights", required=True)
    r.add_argument("--holdout", nargs="*")
    r.add_argument("--holdout-encounter", nargs="*")
    r.add_argument("--eval-fights", nargs="*", default=[], help="held-out fight files to score")
    r.add_argument("--eval-reps", type=int, default=4)
    r.add_argument("--eval-every", type=int, default=10)
    r.add_argument("--rooms", default="boss,elite")
    r.add_argument("--n-states", type=int, default=12000)
    r.add_argument("--iters", type=int, default=200)
    r.add_argument("--batch-states", type=int, default=512)
    r.add_argument("--k", type=int, default=8, help="playouts per start (the baseline group)")
    r.add_argument("--minibatch", type=int, default=16384)
    r.add_argument("--lr", type=float, default=3e-4)
    r.add_argument("--ent", type=float, default=0.01)
    r.add_argument("--w-hp", type=float, default=0.3, help="win reward = 1 + w_hp * hp left")
    r.add_argument(
        "--w-removed", type=float, default=0.2, help="loss reward = w * enemy HP removed"
    )
    r.add_argument("--no-early-end", action="store_true")
    r.add_argument(
        "--temperature",
        type=float,
        default=1.0,
        help="temperature written to --out. The RL net plays best at its training "
        "temperature 1.0: at 0.5 its cut in boss deaths shrank from 6.1pp to 4.9pp "
        "(GH #777).",
    )
    r.add_argument("--seed", type=int, default=0)
    r.add_argument("--device", default="cuda")
    args = ap.parse_args(argv)
    return {"gen": gen, "train": train, "rl": rl}[args.cmd](args)


if __name__ == "__main__":
    raise SystemExit(main())
