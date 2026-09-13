# PPO champion — `ppo_fullrun_ironclad_plr_it550`

**Promoted 2026-09-02 on a PASSED pre-registered gate**: +5.23pp pooled on 2200
paired cells, McNemar exact two-sided p=1.0e-06, worst level −1.50pp against a
−5.0pp guard. This is the first Ironclad champion since `asc_it300` to pass its
own gate rather than be promoted over a failed one.

- **Character**: Ironclad
- **Source**: `data/runs/ironclad-plr-c1/main.iter_550.pt` (PLR continuation
  "c1"). Checkpoints are named by `it+1`: the net after the final logged
  iteration `it549` is `iter_550`.
- **Replaces**: `ppo_fullrun_ironclad_underdocks_it400` (promoted 2026-08-31
  by user decision over a FAILED gate)
- **Features**: `reward_upgrade` — derived from the checkpoint's own `cfg`.
- **Net**: `ppo.model.RunPolicyNet` defaults (dim=192, n_layers=3, n_heads=4)
- **Pre-registration**:
  `docs/experiments/2026-08-31-ironclad-plr-continuation.md`

## The recipe, and what changed

**Prioritized Level Replay** (Jiang, Grefenstette, Rocktäschel 2021,
arXiv:2010.03934) applied as a *continuation*, not from scratch. A "level" is a
`(seed, ascension)` pair. After each run the level is scored by mean |GAE| — how
badly the critic mispredicted it — and future batches draw half their games
(`--plr-replay-p 0.5`) from a rank-prioritized, staleness-mixed buffer of
already-seen levels instead of from fresh seeds. β=0.1, ρ=0.1, cap 4096, the
paper's Procgen defaults.

Declared change versus the parent's stored cfg: **`--plr-replay-p 0.5`**, plus
the horizon (`--iterations 550 --resume-from-iter 400`). The pre-flight
mechanical cfg diff also caught that the launch command omitted
`--entropy-coef`, which would have silently taken the dataclass default 0.05
instead of the parent's 0.02; it was pinned explicitly. That knob is inert here
— `effective_entropy_coef` saturates at `entropy_coef_final` for
it ≥ start+steps = 70 — but the #545 v1 post-mortem is that four undeclared
deviations, not one, sank that leg.

### ONE deliberate deviation from the paper

The paper samples the replay batch **with replacement**. That is safe only
because PLR is *online*: it re-scores a level the moment its episode ends,
before the next draw, so the rank-1 level churns away between draws. A batched
collector picks a whole iteration's slots from a FROZEN distribution, and at
β=0.1 the exponent is 1/β = 10, which puts P(rank 1) = 0.999 for **any** buffer
size. The pre-flight smoke measured exactly that collapse: **49 of 51 replays
on one `(seed, ascension)`**. Fixed by drawing without replacement in rounds
(PR #656). Over the whole campaign the sampler then held
`plr_replay_distinct` = 473/473 and `plr_replay_top_share` = 0.0021.

Trained with `STS2_RUNAI_POTION_SWAP=1`; **served with it OFF** (the default),
which the previous champion's provenance measured as free (+0.00pp, ±0.47pp).

## The gate — PASSED

Panel **73,000,000–73,000,199** × A0–A10, 200 paired seeds/level = 2200 cells,
both arms under ONE binary, `net_greedy`, deployed eval budget (eval-cheap
1000/4000), `--act1 roll`, **OFF world** (`STS2_RUNAI_POTION_SWAP` unset in both
arms). **0 errored rows in 4,400**, so no seed was dropped.

| lvl | champion | candidate | Δ | b/c | p |
|---|---|---|---|---|---|
| A0 | 164 | 181 | **+8.50pp** | 12/29 | 0.0115 |
| A1 | 167 | 183 | **+8.00pp** | 12/28 | 0.0166 |
| A2 | 167 | 178 | +5.50pp | 14/25 | 0.1081 |
| A3 | 154 | 170 | **+8.00pp** | 16/32 | 0.0293 |
| A4 | 152 | 169 | **+8.50pp** | 15/32 | 0.0186 |
| A5 | 149 | 162 | +6.50pp | 20/33 | 0.0984 |
| A6 | 152 | 153 | +0.50pp | 28/29 | 1.0000 |
| A7 | 141 | 152 | +5.50pp | 28/39 | 0.2215 |
| A8 | 118 | 132 | +7.00pp | 28/42 | 0.1196 |
| A9 | 66 | 68 | +1.00pp | 30/32 | 0.8991 |
| A10 | 18 | 15 | −1.50pp | 13/10 | 0.6776 |

**Pooled: +5.23pp, 2200 paired cells, discordant 216/331, p=1.0e-06.**
Worst level A10 at −1.50pp. Ten of eleven levels positive.

**Cluster-robust check.** The panel is 200 seeds × 11 levels, not 2200
independent runs, so the pooled McNemar is anti-conservative
(`feedback_gate_panels_cluster_by_seed`). Aggregating to a per-seed total and
taking a seed-clustered SE: **+5.23pp ± 1.61pp, z=3.24, p=1.2e-03**, measured
design effect **2.31×**. The pass does not depend on the independence
assumption.

## What this net's gate does NOT establish

**The gain is not attributed to PLR.** There was no no-PLR control arm. The
comparison is "it400 + 150 iterations *with* PLR" against "it400", so the
declared variable, the horizon, and the LR re-anneal are perfectly confounded.
Re-declaring `--iterations 550` re-opened `lr_frac` from the 0.30 floor to 0.49
at it400 and re-annealed it to 0.30 by it550 — and that re-anneal is precisely
the mechanism the *previous* champion's provenance credits for its own gain.

Indirect evidence, both weak and both recorded so nobody has to re-derive it:

- *For PLR.* This lineage has already measured a pure horizon extension:
  it200→it400, no PLR, **+1.41pp (OFF world), p=0.1558 — a failed gate**. This
  leg got +5.23pp from *fewer* extra iterations (150 vs 200). Horizon
  extensions should show diminishing returns on the second application; this
  went the other way, ~3.7×. Cross-campaign, different panel and parent — not a
  controlled contrast.
- *Against.* The free LR re-anneal above is a known-potent effect here and is
  fully present in this leg.

**The mechanism story also does not match where the gain landed.** PLR spent
its replay budget on A7–A9 (A8–A10 share 0.495 at it549, with **A10 alone at
0.055, below its 1/11 = 0.091 uniform share** — it avoided the levels that are
simply lost, which is the Value Correction Hypothesis behaving as advertised
and the opposite of GH #510's killed hard-level upweighting). But the gate's
gain sits at **A0–A4**, where replay mass was lowest. Whatever PLR did, it did
not do it by making the replayed levels better.

The separating experiment is a paired two-arm continuation at identical horizon
and identical LR schedule, `--plr-replay-p 0.5` vs `0.0`. Scheduled on Silent.
Do **not** cite this net as evidence that PLR works until that lands.

## Carried forward from the parent

The GH #620 anti-Fairy potion prior is inherited and unmeasured here. It was
priced at ~0pp on the parent (paired A/B, ±0.47pp) and nothing in this leg
touched the potion surface.

## Reproduce

```text
cd sts2-pycli && PYTHONPATH=python STS2_PPO_FEATURES=reward_upgrade \
  STS2_RUNAI_POTION_SWAP=1 OMP_NUM_THREADS=1 \
  .venv/bin/python -m ppo.trainer \
    --character Ironclad --act1 roll --features reward_upgrade \
    --iterations 550 --resume-from-iter 400 \
    --games-per-iter 960 --sims 250 --boss-sims 1000 \
    --ascension-weights 0:1,1:1,2:1,3:1,4:1,5:1,6:1,7:1,8:1,9:1,10:1 \
    --entropy-coef 0.02 \
    --entropy-decay-start 40 --entropy-decay-steps 30 --entropy-coef-final 0.0125 \
    --lr-anneal-final 0.3 --epochs 2 --device cuda --save-every 10 \
    --plr-replay-p 0.5 \
    --save-path data/runs/ironclad-plr-c1/main
```

Requires PR #656 (without-replacement PLR draws) or the replay batch collapses
onto one level. `--policy-lr` is inert on a resume: `base_lr` is restored from
the saved optimizer state before `apply_lr_schedule`'s `setdefault` runs.

Gate harness: `data/runs/ironclad-plr-c1/gate.sh` (collection, resumable) and
`score_gate.py` (scoring; validated by reproducing the historical v2 gate).
