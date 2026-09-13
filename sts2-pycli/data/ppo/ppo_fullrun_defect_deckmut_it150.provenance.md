# `ppo_fullrun_defect_deckmut_it150` — provenance

First Defect champion trained in the **flags-on** world (GH #399, sub-issue
#429 under umbrella #427): card removal / upgrade / transform / enchant /
event-grant reach the run policy instead of being auto-resolved by the engine.

## Lineage

| | |
| --- | --- |
| Warm start | `ppo_fullrun_defect_asc_it230` (the champion it replaces) |
| Run | 150 iterations x 256 games, uniform A0–A10 |
| Entropy | `--entropy-coef 0.02`, decay from it 100 over 50 steps to 0.002 |
| LR | `--lr-anneal-final 0.3` |
| Final | `lr_frac` 0.3000, `ent_c` 0.002094, `ent` 0.4450 (it 0: 0.4604) |
| Errors | **0** across all 150 iterations (~38,400 runs) |
| Wall-clock | 62.5 h (the last ~90 iterations under a 24-CPU pin) |
| Dir | `~/ppo-defect-deckmut/` |

**The entropy scale deliberately deviates from umbrella #427**, which
prescribes `0.03 → 0.0125`. That is *Ironclad's* scale. Defect's parent
`asc_it230` ran `0.02 → 0.002`, so #427 read literally would have started this
run at 15x the parent's floor and ended the candidate 6x less sharpened than
the incumbent it has to beat on a greedy panel. Precedent: `asc_it230` itself
warm-started from an equally annealed `asc_it80`, restarted flat at 0.02,
decayed to 0.002, and gated at +12.3pp. Confirmed with the user before launch.

Every schedule landed on its closed form — `ent_c` 0.002094 vs 0.0020942 and
`lr_frac` exactly 0.3000 at it 149, checked at it 110/120/130/140 too. Worth
stating because `--lr-anneal-final` defaults silently OFF and cost ~6pp the one
time it was missed.

The committed `.pt` is the **full** checkpoint (`state_dict` + `opt` +
`adv_mean`/`adv_sq` + `cfg`, 24 MB), matching the Ironclad deck-mutation
champion, so a true warm resume of this run is possible.

## Gate

Pre-registered **before any sweep data existed** in
`docs/experiments/2026-08-11-defect-deckmut-gate.md`. Panel
43,000,000–43,000,199 (verified disjoint from every spent panel and from the
60M training seeds), A0–A10 x 200 paired seeds, both arms on one flags-on
binary (`src_hash=af9a1ad7031a7d1f`), `net_greedy`, eval-cheap.

**Pooled 58.1% → 63.6%, +5.5pp, McNemar exact p = 2.00e-06** on 641 discordant
pairs of 2200 cells. 10/11 levels positive; worst is A10 at −1.5pp (13 vs 16
discordant pairs at a ~12% base rate).

Rule was PROMOTE iff pooled delta > 0 at p < 0.01 **and** no level regressing
more than 5.0pp. Both passed.

Only two cells clear p < 0.01 individually (A3 +11.0pp, A5 +11.5pp). Do not
quote either as "the effect" — n=200 per cell is underpowered by design, which
is exactly why the pooled test is the pre-registered one. **The effect is
+5.5pp.**

Headroom (deployed ONNX lane, A0 x 200): `max act_slots 57/160`,
`max tokens 169/320` — both well inside the pre-registered 80% blockers
(128 / 256). Zero combat fallbacks across 117,710 decisions.

## Attribution — READ THIS BEFORE CITING THE NUMBER

**The +5.5pp is NOT attributable to learned deck-mutation skill.** The baseline
`asc_it230` never trained on the flags-on world. Measuring both arms on it is
what makes the comparison fair for a *promotion* decision, but the gain bundles
three things this gate cannot separate:

1. whatever the policy learned about removals / upgrades / transforms,
2. 150 further iterations of ordinary training on every other surface,
3. the plain advantage of having trained in the environment it is evaluated in.

Isolating (1) requires an arm that trains the same 150 iterations on the
**flags-off** world. That is a separate campaign; it is not blocked by this
promotion, and nothing run so far substitutes for it.

What *is* established is narrower and is embedding evidence, not win-rate
evidence: the deck-mutation surface tokens are being trained. Over the parent
leg (flags-off), four of the five mutation rows of `emb_surface.weight` drifted
exactly 0.000000; over iterations 10–20 of this campaign they moved 0.19–0.38%,
against 0.097% for the other exercised rows. Five rows still read exactly
0.000000 (`relic-reward`, `treasure`, `unknown`, two padding), so there is no
weight-decay floor and every nonzero entry is gradient that arrived through a
real decision. Detail in `docs/experiments/2026-08-11-defect-deckmut-preflight.md`.

## Shape of the effect — no prediction was registered, and none is back-filled

The pre-registered doc deliberately made **no** claim about where on the curve
the gain would land, after the Ironclad campaign falsified both of its
predictions.

Observed: the gain is roughly flat in absolute pp across A0–A8. The incumbent
falls 78.0% → 47.0% over that range while the candidate falls 81.5% → 53.0%.
That is *not* the Ironclad shape (a U peaking at A7/A8 with +14/+15.5pp), and
it is not the Defect parent leg's shape either (carried by A0–A8, A9/A10 null).

**A9/A10 are null again** (+1.5pp, −1.5pp) — about 3 runs of difference against
20% / 13% base rates. Those cells are consistent with no effect *and* with the
pooled effect; they cannot distinguish, so no claim is made about A9+. The
A8 → A9 cliff (47.0% → 20.0% for the incumbent) is a property of the game at
that ascension, not of either net.

The flat shape is an observation for a future campaign to test, not a finding
this gate establishes.

## Cautions

* **Every pre-#399 Defect win-rate anchor is non-comparable**, including
  `ppo_fullrun_defect_asc_it230.provenance.md`'s 61.7% pooled. That number was
  measured on panel 46M in the flags-off world; the same net reads 58.1% here.
  Cross-panel comparison is independently forbidden regardless.
* **The gate number is not a live number yet.** Until plan Task 11 retires the
  `auto_front` stopgap — which requires all five characters promoted, because
  the advisor override is global — the served lane still answers deck-mutation
  prompts itself. This champion is served in a world it was not trained for
  until then.
* `card-remove` is the thinnest surface at ~0.12–0.25 decisions/run. Its
  exposure dipped mid-run (0.25 at it 100 → 0.12) and partly recovered (0.17 by
  it 140) without ever reaching zero; it was still the second-strongest
  drifting mutation row over iterations 100–150. If a future campaign shortens
  either the iteration or games-per-iter axis, removal is what goes dark first.
