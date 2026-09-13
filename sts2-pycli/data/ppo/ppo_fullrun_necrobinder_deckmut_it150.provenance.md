# ppo_fullrun_necrobinder_deckmut_it150

Necrobinder run-policy champion. Third character through the deck-mutation
(flags-on) world, after Ironclad and Silent — and the **first champion of any
character that can see which boss it is building a deck for**. GH #431, part of
#427 / #399.

## What promoted it

Pre-registered gate: `docs/experiments/2026-08-15-necrobinder-deckmut-gate.md`
(committed `486658148` **before** the sweep launched).

| | |
| --- | --- |
| Baseline | `ppo_fullrun_necrobinder_asc_it300` (the net this replaces) |
| Panel | A0–A10 × 200 paired seeds, 41,000,000–41,000,199 (fresh, disjoint) |
| Policy | `net_greedy`, eval-cheap (1000 / 4000 boss), honest search |
| Binary | one build both arms, `src_hash=5e8066537e2a5cf9` |
| **Result** | **26.32% → 33.14%, +6.82pp pooled, McNemar exact p=6.09e-10** |

2,200 paired cells, **0 unpaired dropped** — both arms completed all 200 seeds
at every level.

| A | `asc_it300` | **`deckmut_it150`** | delta | b | c | p |
|---|---|---|---|---|---|---|
| 0 | 39.50% | **50.00%** | +10.50 | 24 | 45 | 1.54e-02 |
| 1 | 51.50% | **53.00%** | +1.50 | 30 | 33 | 8.01e-01 |
| 2 | 36.50% | **44.50%** | +8.00 | 24 | 40 | 5.99e-02 |
| 3 | 35.00% | **48.00%** | +13.00 | 28 | 54 | 5.44e-03 |
| 4 | 34.00% | **40.50%** | +6.50 | 30 | 43 | 1.60e-01 |
| 5 | 22.00% | **35.00%** | +13.00 | 17 | 43 | 1.07e-03 |
| 6 | 28.50% | **30.00%** | +1.50 | 26 | 29 | 7.88e-01 |
| 7 | 17.00% | **32.00%** | +15.00 | 12 | 42 | 5.21e-05 |
| 8 | 20.00% | **23.00%** | +3.00 | 18 | 24 | 4.41e-01 |
| 9 | 4.00% | **5.00%** | +1.00 | 7 | 9 | 8.04e-01 |
| 10 | 1.50% | **3.50%** | +2.00 | 2 | 6 | 2.89e-01 |
| **pooled** | **26.32%** | **33.14%** | **+6.82** | 218 | 368 | **6.09e-10** |

Rule: promote iff pooled delta > 0 at p < 0.01 **and** no level regresses by
more than 5.0pp. Both PASS — **nothing regresses at all**, the worst level
being A9 at +1.00pp.

### Read this before quoting the per-level numbers

The honest summary is **"+6.82pp pooled, p=6e-10, and no level regresses"** —
*not* eleven separate wins. Only **A3, A5 and A7** clear 0.01 individually.
A1, A6, A8, A9 and A10 are null on their own. The per-level p values are
printed so the pooled result cannot be re-read as eleven independent findings;
the rule gates on a pooled test plus a regression floor for exactly that reason.

A9/A10 were **pre-registered as expected-uninformative** before any data: at a
~2–4% base rate, 200 seeds yields a handful of wins, so neither the sign nor
the size there is evidence.

**Do not compare the pooled 33.14% to `asc_it300`'s recorded 31.92%.** Those
are different panels, and cross-panel win rates are never comparable (playbook
§2.1: one bit-identical net read 43.1% and 47.2% on two panels). Only the
within-panel delta means anything.

Headroom (promotion blocker, ONNX lane): max `ppo_peak_act_slots` **50**/160,
max `ppo_peak_tokens` **159**/320 over a 200-seed Necrobinder A0 panel, all 200
rows asserted to carry `character: "necrobinder"` before any number was read.
Cleared.

## Training

Warm-started from `ppo_fullrun_necrobinder_asc_it300.pt` — **the deployed
champion**. 150 iterations × 256 games, uniform A0–A10, **38.2 h, 0 errors over
38,400 runs**, no supervisor restarts.

```
--entropy-coef 0.02 --entropy-decay-start 100 --entropy-decay-steps 50
--entropy-coef-final 0.0025 --lr-anneal-final 0.3
```

### Two deliberate deviations, both argued in advance

**The warm start is not the one GH #431 names.** That issue is built around
`it750` and its three defects (pre-#317 env, no LR anneal, A0-only
curriculum), concluding this leg would bank four improvements at once and be
"the least informative of the five". All three were fixed on 2026-08-08 when
`asc_it300` was promoted at +10.91pp over `it750`. The issue was written
against a stale playbook status table. Warm-starting from `it750` would have
re-banked a gain already claimed and blessed.

**The entropy endpoints are not the playbook recipe's.** 0.03 / 0.0125 was
calibrated on Ironclad and Silent, whose parents floored ≈0.0128 and ran `ent`
at 0.43–0.58. This parent ran its flat phase at **0.02**, sat at `ent`
0.39–0.41, and finished fully annealed at **0.002535** / `ent` 0.287. The
recipe would have restarted the flat phase at ~12× the parent's floor *and*
finished 5× less sharp than the net being measured against — wrong in both
directions at once. This run reused the character's own endpoints.

Both schedules were verified against their closed forms rather than trusted:
final `lr_frac` **0.30000** (predicted 0.30000), final `entropy_coef`
**0.002606** (predicted 0.002606). The `lr_frac` check was also run at it 1–2,
where it reports MISMATCH against a wrong `--iterations` — so it fails
differently depending on the answer instead of agreeing with itself.

`main.stats.jsonl` ships alongside as `…it150.stats.jsonl` — the only record of
the run's entropy/LR state, because it is not recoverable from the weights.

## ⚠ Attribution — promotable, NOT attributable

**Do not write this up as "conditioning on the boss is worth +6.82pp."** Three
things moved at once against `asc_it300`:

1. the five deck-mutation surfaces became visible;
2. the boss-identity input appeared (`N_CAT` 9 → 10);
3. 150 iterations of ordinary continued training.

This was fixed in the pre-registration before the number existed. The gate
licenses one claim: **the combination beats `asc_it300` on a fair paired
sweep.** Nothing finer.

The boss half **is** separable without a retrain, and that is the only thing
that can license a causal claim about (2): pad the boss column at inference on
this net and re-sweep one panel with the same binary. Filed, not run.

The precedent being guarded against is #339/#340, where a +15.5pp was written
up as attributable when it was not and had to be walked back (GH #333).

Unlike Silent's leg there is **no engine-shift confound**: `origin/main` did
not move during the campaign, so this net trained and was gated on the same
engine (`src_hash` 5e8066537e2a5cf9 at both ends, `assert_wheel_fresh()`
re-verified at gate time rather than only at launch).

## Evidence the boss input was learned (not proof of skill)

Per-row `emb_boss` drift, warm start (where the table did not exist) vs it150.
The table is **zero-initialized**, so unlike the `emb_event` campaigns there is
no weight-decay floor to subtract: a row that never received gradient reads
exactly 0.000000.

| table | median row drift | rows moved | |
| --- | --- | --- | --- |
| `emb_boss` | **0.082892** | 9/11 | the new input — highest of any table |
| `emb_relic` | 0.044375 | 250/299 | always-trained reference |
| `emb_potion` | 0.043965 | 50/65 | always-trained reference |
| `emb_event` | 0.032280 | 48/71 | |
| `emb_upg` | 0.000000 | 2/18 | negative control |

Exactly **9 of 11** boss rows moved — the 9 rollable bosses. `<OOV>` is still
exactly 0.000000 (no unrecognised boss id was ever seen, matching the
pre-flight) and `<PAD>` is still exactly 0.000000 (`padding_idx` holding, so
"no boss known" genuinely contributes nothing).

The rows also **separate**, which is the sharper question — drift from zero
only ever grows:

| | mean row norm | mean pairwise | ratio |
|---|---|---|---|
| it 20 | 0.0553 | 0.0854 | 1.55 |
| it 60 | 0.1045 | 0.1656 | 1.59 |
| it 150 | 0.1435 | 0.2312 | **1.61** |

Random equal-norm vectors give ≈1.41; a shared "a boss exists" offset would
give ≈0. So the net learned *which* boss, not merely that there is one.

**This shows the representation was learned. It does not show the resulting
choices are good** — only the gate measures that, and the gate cannot separate
this input from the other two things that moved.

## Cross-lane check

The ONNX export won **48.5%** on the A0 headroom panel against the torch
`net_greedy` gate's **50.0%** on the *same 200 seeds* — 3 runs apart. The
exported net, which is the thing that ships, is faithful to the checkpoint that
was gated. Nothing else in the promotion path verifies that.

The shipped `.onnx` is byte-identical to the file the sweep ran
(`sha256 0479a07c…`); it was copied, not re-exported.

## Not live until #432 completes

`deck_mutation_override` stands down per character via `deck_mutation_trained`.
Necrobinder is now `true`, so this character's deck decisions reach the net.
**Defect (#429) and Regent (#430) are still `false`** and keep the curse-first
stopgap, so the flag, the override and `realgame_hand_select`'s deck-mutation
branch must all stay until those two land.

Separately: nothing deploys from this repo checkout. The gated number is not
what the live workbench serves until someone runs `just deploy-workbench`.
