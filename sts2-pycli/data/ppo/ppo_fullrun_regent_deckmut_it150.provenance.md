# `ppo_fullrun_regent_deckmut_it150` — served Regent champion

Promoted 2026-08-18, superseding `ppo_fullrun_regent_asc_it200` (served
2026-08-06). **The fifth and last character through the deck-mutation
campaign** (GH #430, part of #427 / #399) — with this promotion every champion
is `deck_mutation_trained` and the #432 stopgap can be retired.

Campaign record: `docs/experiments/2026-08-17-regent-deckmut.md`.
Gate pre-registration + result: `docs/experiments/2026-08-18-regent-deckmut-gate.md`.

## Gate result

Panel **47,000,000–47,000,199**, 200 paired seeds per level, all 11 ascensions,
`--policy net_greedy`, `--procs 30`, default eval-cheap sims (1000 / 4000 boss),
honest combat search (`HiddenInfoMode::BeliefDraw`, the default — **never**
disabled). Both arms the same binary, differing only by `--ckpt`. Exact
two-sided McNemar on discordant pairs. 4,400 runs, 3h43m, **0 unpaired cells
dropped**. Wheel `src_hash=778b87afca23ed4d`, engine `HEAD=f74e9e3c5`.

| A | `asc_it200` | **`deckmut_it150`** | delta | b | c | p |
|---|---|---|---|---|---|---|
| 0 | 64.50% | **70.00%** | +5.50 | 24 | 35 | 1.93e-01 |
| 1 | 68.00% | **76.00%** | +8.00 | 19 | 35 | 4.02e-02 |
| 2 | 62.00% | **64.00%** | +2.00 | 32 | 36 | 7.16e-01 |
| 3 | **60.00%** | 57.00% | −3.00 | 41 | 35 | 5.67e-01 |
| 4 | 56.50% | **62.50%** | +6.00 | 27 | 39 | 1.75e-01 |
| 5 | 50.50% | **57.50%** | +7.00 | 27 | 41 | 1.14e-01 |
| 6 | 49.50% | **55.50%** | +6.00 | 29 | 41 | 1.88e-01 |
| 7 | 40.00% | **48.50%** | +8.50 | 26 | 43 | 5.33e-02 |
| 8 | 35.00% | **42.50%** | +7.50 | 24 | 39 | 7.69e-02 |
| 9 | 11.50% | **18.00%** | +6.50 | 15 | 28 | 6.60e-02 |
| 10 | **9.50%** | 5.50% | −4.00 | 15 | 7 | 1.34e-01 |
| **pooled** | **46.09%** | **50.64%** | **+4.55** | 279 | 379 | **1.11e-04** |

Pre-registered rule (committed before any cell existed): promote iff pooled
delta > 0 at McNemar p < 0.01 **and** no level regresses by more than 5.0pp.
Both **PASS** — +4.55pp at p=1.11e-04, worst level −4.00pp.

### Read this before quoting the per-level numbers

**Only A1 clears significance on its own** (p=0.040). A7 (0.053), A8 (0.077)
and A9 (0.066) are suggestive and nothing more; the other seven levels are
indistinguishable from noise at n=200. The honest summary is *"+4.55pp pooled,
p=1.1e-04, positive at 9 of 11 levels"* — **not** nine separate wins.

**A10 regressed −4.00pp** and cleared the promotion rule by only 1.0pp. It is
not established as a regression either (b=15, c=7, p=0.134 — 19 wins against
11), but it is the one number here worth re-checking on the next Regent panel.
A3's −3.00pp (p=0.57) is squarely noise.

The gate doc records a correction to its **own** pre-registered underpower
caveat: it declared A9/A10 near-uninformative from the incumbent's 7.5%/3.0% on
the *old* 72M panel, but on this panel the incumbent read **11.5% / 9.5%**, so
those levels carry more information than registered, not less. Stated because
the stale caveat would otherwise discount A9's +6.50pp and excuse A10's
−4.00pp — self-serving in opposite directions.

This is the **smallest** of the five deck-mutation gains (Silent +15.0, Ironclad
+8.9, Necrobinder +6.8, Defect +5.5, Regent +4.55). Do not rank the characters
by these numbers: different characters, different panels, and playbook §2.1 has
one bit-identical net scoring 43.1% and 47.2% on two panels.

## Attribution — NOT attributable to learned deck-mutation skill

**+4.55pp is the combined effect of two changes** against the deployed champion:

1. the five deck-mutation surfaces (remove / upgrade / transform / enchant /
   grant) became **visible** to the policy;
2. 150 iterations of ordinary continued training.

The baseline never trained in an environment that presented those surfaces, so
the gain cannot be separated into "the policy learned to mutate its deck" and
"the policy trained longer". **No per-cause claim is made**, and this number
must not later be cited as evidence for deck mutation, for boss conditioning,
or for anything else individually.

This is the same caveat Ironclad's and Defect's provenance files carry. The
precedent it guards: #339/#340's +15.5pp was written up as attributable when it
was not and had to be walked back (GH #333).

It is, however, the **cleanest** leg of the five — a two-way confound, against
the strongest baseline any leg faced (post-#317, uniform curriculum, fully
annealed, deficient in exactly one respect).

### The boss slot is inherited, not introduced

`emb_boss` (categorical slot 9, `N_CAT` 10) shipped with the Necrobinder leg
(#450) and is unconditional, so this campaign inherited it. It is **not** a
third weight change: `asc_it200` predates the slot, `load_compat` keeps the
net's own init for missing tensors, and that init is zeros — so the warm start
was exactly a no-op. What the leg added is the *capability* to use it, and the
iter_20 gradient check confirmed the table learned: 9/11 rows off exactly zero
(the two still at zero being OOV and pad), separation ratio 1.515 against ≈1.41
for random rows.

Separating the boss contribution stays cheap and off the critical path: force
the boss column to the pad row at inference on this net — a same-binary
ablation over one panel, not a retrain.

### One caveat specific to this leg: the engine moved mid-campaign

The candidate **trained** against wheel `78d2c2fa28679f09` (`90d6d809b`) and was
**gated** on `778b87afca23ed4d` (`f74e9e3c5`), after 32 commits of engine/MCTS
perf work landed during the 34 h run. Those commits claim decision-identical
search; on the PyO3 wheel path that claim is false — same net, same 20 seeds,
**0/20 identical trajectories, 7/20 outcomes flipped**, with both builds
internally deterministic (PGO and map-gen ruled out).

The comparison remains fair — **both arms were gated on the same engine**, and
the incumbent was re-measured in-panel rather than having old figures carried
over. But this is a delta measured on an engine *neither* net trained against.
Details in the campaign doc.

## ONNX headroom — OK (promotion blocker cleared)

Same 200 A0 seeds, driven through the deployed lane (`run_arena sweep --policy
ppo --combat eval-cheap`, `STS2_RUNAI_PPO` at the candidate). Thresholds
pre-registered at 80% of budget:

| | measured | budget | |
|---|---|---|---|
| peak act slots | **48** | 160 (30%) | blocker at 128 |
| peak tokens | **164** | 320 (51%) | blocker at 256 |
| max final deck | 38 | | |
| panics | 0 | | |
| combat fallbacks | 0 / 96,036 | | |

Regent's decks stay small — 38 cards at the largest, against the ~126-card case
that forced `A_MAX` 96 → 160 in GH #315 — so the removal surface never
approaches the slot budget on this character.

## Cross-lane check — ONNX vs torch, paired on identical seeds

| | |
|---|---|
| torch (`ppo.probe --policy net_greedy`) | 140/200 = 70.0% |
| ONNX (`run_arena --policy ppo`) | 136/200 = 68.0% |
| delta | −2.00pp |
| per-seed agreement | **190/200 = 95.0%** |
| discordant | b=7, c=3 — McNemar p = 0.344 |

The lanes are not bit-identical decision paths by construction, so exact
agreement was never the bar; an indistinguishable win rate at 95% agreement
says the export is faithful.

**The shipped `.onnx` is byte-identical to the artifact the headroom pass swept**
(`sha256 8ee4efcb2020126096255f0d5f34b2a8bfb97866e20f729a6e0c10dea472e7ea`) —
copied, not re-exported. No `.onnx.data` sidecar; `external_data=False` holds.

## Recipe

```
--character Regent --iterations 150 --games-per-iter 256
--ascension-weights "0:1,...,10:1"
--init-ckpt data/ppo/ppo_fullrun_regent_asc_it200.pt
--entropy-coef 0.02 --entropy-decay-start 100 --entropy-decay-steps 50
--entropy-coef-final 0.0025 --lr-anneal-final 0.3
--save-every 5
```

150 iterations × 256 games, 34 h, **0 errors**, no supervisor restarts.
Entropy endpoints are this character's own (the parent ran its flat phase at
0.02 and finished at 0.002606), **not** the playbook's 0.03/0.0125 — which was
calibrated on Ironclad and Silent and would have restarted the flat phase at ~8×
the parent's floor while finishing ~5× less sharp than the net being measured
against.

No annealed tail was run, deliberately. The parent finished at `lr_frac` 0.3, so
there was no missing anneal to supply, and Necrobinder's `it150 → it180` tail —
the one direct test — gated −0.41pp at p=0.73.
