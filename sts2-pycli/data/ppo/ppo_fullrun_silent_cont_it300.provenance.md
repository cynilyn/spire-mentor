# PPO champion — `ppo_fullrun_silent_cont_it300`

**Promoted 2026-09-08 on a PASSED pre-registered gate**: +5.64pp pooled on
2200 paired cells, McNemar exact two-sided p=6.9e-07, seed-clustered
p=7.5e-04, **all eleven levels positive**.

- **Character**: Silent
- **Source**: `data/runs/silent-plr-ab/ctrl.iter_300.pt` — **arm A (control) of
  the Silent PLR A/B**. Checkpoints are named by `it+1`: the net after the
  final logged iteration `it299` is `iter_300`.
- **Replaces**: `ppo_fullrun_silent_deckmut_it150` (2026-08-14)
- **Features**: `reward_upgrade` — served at the default schema this net is
  **silently wrong, not broken**, so `champions.toml` must carry it.
- **Net**: `ppo.model.RunPolicyNet` defaults (dim=192, n_layers=3, n_heads=4)
- **Pre-registration**: `docs/experiments/2026-09-02-silent-plr-ab.md`

## This is the CONTROL arm of a PLR experiment

The campaign was designed to answer "does Prioritized Level Replay do
anything?", with two arms differing in exactly one flag. **PLR came back NULL**
(see below), so the arm promoted here is the **PLR-free** one — the simpler
recipe, and the one whose behaviour is fully explained by knobs already in the
playbook.

## The recipe

A 150-iteration continuation of the deployed champion, it150 -> it300, resumed
**with optimizer state**. Five things the Silent lineage had never had were
brought over from Ironclad, all of them applied identically to both arms:

| inherited | old champion | this net |
|---|---|---|
| `--act1 roll` (#545) | *absent* — Overgrowth only | **roll** |
| `--features reward_upgrade` | *absent* | **on** |
| `--sims` / `--boss-sims` (#482) | *absent* | **250 / 1000** |
| `--games-per-iter` (#486) | 256 | **960** |
| `STS2_RUNAI_POTION_SWAP` | unset | **1** (trained), **off** (served) |

The entropy schedule was **not** inherited: entropy scale is per-character
(#457), so Silent keeps its own 0.03 / 100 / 50 / 0.0125. It is inert at this
horizon (floor reached at it150) and was pinned explicitly anyway — the #545 v1
post-mortem is that four *undeclared* deviations sank that leg.

### The parent had to be re-laid onto the current encoder

The 2026-08-14 champion cannot be strictly resumed today: `cont` grew 19 -> 25
slots (the Act-2 trade-event payload port) and three `cat` embedding tables
were added, so `cont_proj.proj.weight` is [192, 228] against [192, 300].

Both growths are **append-only**, which is what makes the fix
behaviour-preserving: slots 0-18 mean what they always meant, the new
projection columns are zeroed, and the three new tables are zero-initialised
and enter the token embedding *additively*. `model.load_compat` does this for
the weights; `data/runs/silent-plr-ab/upgrade_parent.py` extends the same
transform to the Adam moments, matched by parameter NAME (indices shift when
the net gains parameters, and `make_optimizer` orders the policy group before
the value group). Moments carried for 50 of 53 parameters; the three new
tensors get lazy step-0 state.

Verified rather than assumed: a 2-iteration smoke resumed at `ev=0.646`,
`kl=0.0008`. A scrambled feature layout would have collapsed explained
variance.

**The gate's champion arm is that re-laid net**, not the raw file — so the
+5.64pp measures the 150 iterations and nothing else.

## The gate — PASSED

Panel **76,000,000–76,000,199** × A0–A10, 200 paired seeds/level = 2200 cells,
`net_greedy`, deployed eval budget (eval-cheap 1000/4000), `--act1 roll`,
**OFF world** (`STS2_RUNAI_POTION_SWAP` unset in both arms, though both nets
trained with it on). **0 errored rows in 4,400.**

| lvl | champion | candidate | Δ | b/c | p |
|---|---|---|---|---|---|
| A0 | 134 | 143 | +4.50pp | 24/33 | 0.2892 |
| A1 | 137 | 142 | +2.50pp | 22/27 | 0.5682 |
| A2 | 123 | 139 | +8.00pp | 25/41 | 0.0640 |
| A3 | 122 | 135 | +6.50pp | 28/41 | 0.1480 |
| A4 | 117 | 136 | **+9.50pp** | 23/42 | 0.0248 |
| A5 | 104 | 116 | +6.00pp | 29/41 | 0.1882 |
| A6 | 100 | 114 | +7.00pp | 27/41 | 0.1143 |
| A7 | 93 | 102 | +4.50pp | 29/38 | 0.3284 |
| A8 | 69 | 81 | +6.00pp | 22/34 | 0.1409 |
| A9 | 27 | 36 | +4.50pp | 13/22 | 0.1755 |
| A10 | 6 | 12 | +3.00pp | 5/11 | 0.2101 |

**Pooled: +5.64pp, 2200 paired cells, discordant 247/371, p=6.9e-07.**
Overall 46.91% -> **52.55%**. Worst level A1 at **+2.50pp** — no level
regressed at all, against a -5.0pp guard.

**Cluster-robust check.** 200 seeds × 11 levels is not 2200 independent runs
(`feedback_gate_panels_cluster_by_seed`). Aggregating per seed:
**+5.64pp ± 1.67pp, z=3.37, p=7.5e-04**, design effect **2.19×**. The pass
does not depend on the independence assumption.

## What this net's gate does NOT establish

The gate bundles the continuation with act mixture, `reward_upgrade`, cheap
sims, volume and the potion-swap surface, and **attributes nothing among
them**. That was its only job — promote or don't.

What the campaign *does* attribute is PLR, because that had a control:

> **PLR is a NULL on Silent.** Primary comparison, arm B (PLR) vs arm A, fresh
> panel 75,000,000–75,000,399 × A0–A10, **4400 paired cells**:
> **Δ = −1.07pp ± 1.33pp**, seed-clustered z=−0.80, **p=0.42** (naive McNemar
> p=0.20, design effect 2.71×). **|PLR effect| < 2.60pp at 95%.**

The sampler was healthy throughout (`replay_frac` 0.49–0.53, `distinct`
455–507 of ~500, `top_share` 0.002, buffer saturated at 4096), so this is a
null result, not a broken run.

**Consequence for the Ironclad champion.** `ppo_fullrun_ironclad_plr_it550`
took +5.23pp from a leg where PLR, the horizon extension and the LR re-anneal
were perfectly confounded. This null makes the honest reading of that gain
"the horizon extension and the LR re-anneal did the work" — which was the
interpretation committed *before* this data existed. It does not invalidate
that promotion, which passed its own gate on its own panel; it changes the
explanation.

**The emerging lever.** Two characters in a row, ~5pp each, from "resume a
finished champion for another 150 iterations" — which re-opens the LR anneal
for free, since `apply_lr_schedule` is anchored to absolute iteration. On
Ironclad that was confounded with PLR; here it is not.

## Carried forward, unmeasured

The GH #620 anti-Fairy potion prior. Priced at ~0pp on the Ironclad lineage
(paired A/B, ±0.47pp); nothing in this leg touched the potion surface.

## Reproduce

```text
cd sts2-pycli && PYTHONPATH=python STS2_PPO_FEATURES=reward_upgrade \
  STS2_RUNAI_POTION_SWAP=1 OMP_NUM_THREADS=1 \
  .venv/bin/python -m ppo.trainer \
    --character Silent --act1 roll --features reward_upgrade \
    --iterations 300 --resume-from-iter 150 \
    --games-per-iter 960 --sims 250 --boss-sims 1000 \
    --ascension-weights 0:1,1:1,2:1,3:1,4:1,5:1,6:1,7:1,8:1,9:1,10:1 \
    --entropy-coef 0.03 \
    --entropy-decay-start 100 --entropy-decay-steps 50 --entropy-coef-final 0.0125 \
    --lr-anneal-final 0.3 --epochs 2 --device cuda --save-every 10 \
    --plr-replay-p 0.0 \
    --save-path data/runs/silent-plr-ab/ctrl
```

Requires `upgrade_parent.py` to have produced `parent_upgraded.pt` at
`<save_path>.iter_150.pt` first. `--policy-lr` is inert on a resume: `base_lr`
is restored from the saved optimizer state before `apply_lr_schedule`'s
`setdefault` runs.

Harnesses: `data/runs/silent-plr-ab/{arm.sh, preflight.py, upgrade_parent.py,
gate.sh, score_ab.py, gate_promo.sh}`. `score_ab.py` was validated by
reproducing the Ironclad gate's stored result exactly (+5.23pp, clustered
z=3.24, deff 2.31×) before it scored this campaign.
