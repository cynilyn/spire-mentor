//! Site benchmark: "random-play fights per second, one core".
//!
//! The October 2026 measurement behind the docs site and the READMEs
//! (`sts2-rs/docs/perf/2026-10-14-site-performance.md`) built this code
//! against both the previous and the new engine, so the two engines ran
//! byte-identical harness code.
//!
//! Workload (the ONE definition the site uses;
//! `scripts/site-bench/cs_random_fights.py` at the repository root runs the
//! same workload through the shipped game):
//! * Ironclad, ascension 0, starter deck, 80/80 HP, plus Burning Blood
//!   (`--relics none` drops it);
//! * every content registry installed (`ensure_content_installed`, as the run
//!   AI does), so relic hooks are live;
//! * one Act-1 Overgrowth encounter per seed, with the real monster AI. The
//!   default `--mix round-robin` cycles through the 22 encounters of
//!   `ACT1_ENCOUNTERS` in registry order (seed `s` gets encounter `s % 22`), so
//!   `--seeds 2200` gives exactly 100 start states per encounter, the same
//!   balanced mix `cs_random_fights.py` plays (K fights per encounter).
//!   `--mix random` draws uniformly instead (`random-act1`, the Aug-17
//!   `xsim_random_bench` draw; per-encounter counts then carry ±10% noise);
//! * uniformly random legal actions until the fight ends;
//! * each seed's start state is built once, UNTIMED; every fight clones it
//!   (TIMED) and plays the clone in place;
//! * one thread (no rayon), plain `--release`.
//!
//! Monsters are set up the way a real run sets them up
//! (`sts2_content::run_callbacks::build_combat_for_room` and
//! `run_bridge::enter_combat_from_roster`), not the way the `sts2-cli`
//! scenario builder does (GH #762). The builder gives every monster the AI of
//! slot 0 and drops the encounter's spawn args, so in THE_KIN_BOSS both
//! followers open the same way and in INKLETS_NORMAL no inklet opens with
//! WHIRLWIND. It also rolls HP off `Rng::new(seed)`, the very stream the
//! roster randomizer has just read, so in the five randomized rosters a
//! monster's HP follows from which kind was drawn (a front Axe Ruby Raider
//! always had 20 HP, a front Tracker always 25). Here, after the builder:
//! * each monster's AI is rebuilt with the run path's
//!   `SpawnPosition { alone: n == 1, front: i == 0, back: n > 1 && i == n - 1,
//!   index: i }` and the roster's spawn args;
//! * HP is rolled as the run path rolls it: `roll_unique_max_hps` (C#
//!   `SetUniqueMonsterHpValue`: no two enemies share an HP value when the
//!   ranges allow it) off the combat's own `RunRngSet.niche`, which a summon
//!   later in the fight continues from;
//! * the legacy combat RNG (`CombatState::rng`) is reseeded as the run path
//!   seeds it, from the run seed instead of `Rng::new(seed)`.
//!
//! Given the same roster and `RunRngSet::new(seed)`, the start state then
//! equals `enter_combat_from_roster`'s in every monster field (kind, slot,
//! HP, block, powers, AI state, opening intent), in the RNG cursors and in
//! the opening hand and draw pile (checked on all 22 encounters x 200 seeds).
//!
//! `--legacy-spawn` keeps the builder's monsters as they are. Its output is
//! byte-for-byte that of this harness before the fix, which is what the
//! 2026-10-14 site numbers were measured with.
//!
//! Left as the builder makes it, because nothing in this workload reads it
//! (setting them as the run path does leaves the 2200 x 50 identity
//! unchanged): `is_elite` / `is_boss` (room-type relic gates),
//! `player.character` (character-pool card generation, potion locks), and the
//! creature id order (the run path creates the player creature first).
//!
//! `--encounters A,B,...` (cross-engine runs on the content every engine
//! has) replaces the 22-encounter round-robin list with the given ids, in the
//! given order: seed `s` fights `list[s % list.len()]`, so `--seeds` a
//! multiple of the list length keeps the mix balanced. Without it the run is
//! byte-for-byte the default workload above.
//!
//! Output: one JSON line on stdout (the encounter list used goes to stderr; it
//! is also in the JSON when `--encounters` is given; `"spawn":"run"` marks a
//! run-path start, and a `--legacy-spawn` line has no `spawn` field, as before
//! the fix). `decisions` counts every
//! applied action (card plays and end turns). `decisions`, `wins` and `digest`
//! depend only on the engine's behaviour, so two exact engines must print
//! identical values.
//!
//! ```text
//! cargo build --release -p sts2-bench --example site_random_bench
//! taskset -c 20 target/release/examples/site_random_bench --seeds 2200 --reps 50
//! ```

use std::time::Instant;

use clap::Parser;
use rand::{Rng as _, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

use sts2_cli::sim::scenario::relics::seed_relics;
use sts2_cli::sim::scenario::{resolve_encounter, MonsterSpec};
use sts2_cli::sim::{build_starter_state_from_monsters, PlayerClass};
use sts2_content::encounters::act1_overgrowth::ACT1_ENCOUNTERS;
use sts2_content::monster_dispatch::{build_ai_with_ascension, SpawnPosition};
use sts2_core::combat_loop::{start_combat, start_player_turn};
use sts2_core::monsters::roll_unique_max_hps;
use sts2_core::relics::RelicId;
use sts2_core::{apply, legal_actions, CombatState, MonsterKind};

#[derive(Parser, Debug)]
struct Args {
    /// Number of seeds = start states (one Act-1 encounter each). A multiple
    /// of 22 keeps the round-robin mix exactly balanced.
    #[arg(long, default_value_t = 2200)]
    seeds: u64,
    #[arg(long, default_value_t = 0)]
    seed_start: u64,
    /// Random fights per start state.
    #[arg(long, default_value_t = 50)]
    reps: u64,
    /// `burning-blood` (the Ironclad start) or `none`.
    #[arg(long, default_value = "burning-blood")]
    relics: String,
    /// `round-robin` (encounter `seed % 22`, balanced) or `random` (`random-act1`).
    #[arg(long, default_value = "round-robin")]
    mix: String,
    /// Comma-separated Act-1 Overgrowth encounter ids replacing the
    /// round-robin list, in this order (seed `s` fights `list[s % len]`).
    /// Round-robin mix only. Default: all 22 of `ACT1_ENCOUNTERS`.
    #[arg(long)]
    encounters: Option<String>,
    /// Keep the `sts2-cli` scenario builder's monsters (every monster on slot
    /// 0's AI, no spawn args, HP rolled independently; GH #762): the harness
    /// as it was before the run-path fix, for auditing.
    #[arg(long)]
    legacy_spawn: bool,
}

/// The round-robin encounter list: `--encounters` (validated against
/// `ACT1_ENCOUNTERS`, canonical spelling) or all 22 in registry order.
fn encounter_list(arg: Option<&str>) -> Vec<&'static str> {
    let Some(arg) = arg else {
        return ACT1_ENCOUNTERS.iter().map(|e| e.id).collect();
    };
    let list: Vec<&'static str> = arg
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|id| {
            ACT1_ENCOUNTERS
                .iter()
                .find(|e| e.id.eq_ignore_ascii_case(id))
                .unwrap_or_else(|| {
                    panic!("--encounters: {id:?} is not an Act-1 Overgrowth encounter id")
                })
                .id
        })
        .collect();
    assert!(!list.is_empty(), "--encounters: empty list");
    list
}

/// Give the scenario builder's monsters what a real run gives them
/// (`run_callbacks::build_combat_for_room` then
/// `run_bridge::enter_combat_from_roster`; GH #762):
/// * the AI built from the run path's `SpawnPosition` and the roster's spawn
///   args (the builder uses index 0 for everyone and drops the args);
/// * HP from `roll_unique_max_hps` off the combat's `RunRngSet.niche`, the run
///   path's rule and stream (the builder rolls each monster independently off
///   `Rng::new(seed)`, the roster randomizer's own stream, so two monsters
///   can share a value and a randomized roster's HP is tied to its kinds).
///   The builder leaves `niche` unread, so after this its cursor is where the
///   run path's is: past one read per monster, where a mid-fight summon's HP
///   roll continues;
/// * the legacy combat RNG seeded from the run seed, as the run path seeds it,
///   instead of the builder's `Rng::new(seed)` (already read for HP).
///
/// Spawn buffs need no redo: `apply_creature_spawn_buffs` reads the species
/// and the ascension only, and the builder already ran it. The encounter slot
/// (`MonsterSpec.slot`, the run path's `assign_encounter_slots`) lives on the
/// `MonsterAi`, so it is stamped again onto the new one.
fn spawn_as_run(state: &mut CombatState, specs: &[MonsterSpec]) {
    let n = specs.len();
    assert_eq!(state.enemy_uids.len(), n, "one creature per roster entry");
    assert!(
        specs.iter().all(|m| m.hp_override.is_none()),
        "encounter rosters carry no HP overrides"
    );
    let kinds: Vec<MonsterKind> = specs.iter().map(|m| m.kind).collect();
    let hps = roll_unique_max_hps(&kinds, &mut state.run_rng.niche, state.ascension);
    state.rng = sts2_core::dotnet_rng::Rng::new(state.run_rng.seed);
    for (i, spec) in specs.iter().enumerate() {
        let position = SpawnPosition {
            alone: n == 1,
            front: i == 0,
            back: n > 1 && i == n - 1,
            index: u8::try_from(i).expect("roster index fits u8"),
        };
        let ai = build_ai_with_ascension(spec.kind, spec.args, position, state.ascension);
        let uid = state.enemy_uids[i];
        let creature = state
            .creatures
            .iter_mut()
            .find(|c| c.id == uid.0)
            .expect("enemy uid in the creature store");
        assert_eq!(
            creature.species,
            Some(spec.kind),
            "creature {i} is roster entry {i}"
        );
        creature.current_hp = hps[i];
        creature.max_hp = hps[i];
        creature.ai = Some(ai);
        if let Some(slot) = spec.slot {
            state.set_enemy_slot(uid, slot);
        }
        assert_eq!(state.enemy_slot(uid), spec.slot, "slot of creature {i}");
    }
}

fn start_state(
    seed: u64,
    burning_blood: bool,
    round_robin: bool,
    list: &[&'static str],
    legacy_spawn: bool,
) -> CombatState {
    let mut enc_rng = sts2_core::dotnet_rng::Rng::new(seed as i32);
    // Resolving by UPPER_SNAKE id goes through the same `specs_from_encounter_def`
    // (with the rng, so randomized rosters roll) as the `random-act1` draw.
    let name = if round_robin {
        list[(seed % list.len() as u64) as usize]
    } else {
        "random-act1"
    };
    let monsters = resolve_encounter(name, &mut enc_rng)
        .unwrap_or_else(|e| panic!("encounter {name} does not resolve: {e:?}"));
    let specs = monsters.clone();
    let mut state =
        build_starter_state_from_monsters(seed, monsters, PlayerClass::Ironclad, None, None, None);
    if !legacy_spawn {
        spawn_as_run(&mut state, &specs);
    }
    if burning_blood {
        // Must precede start_combat (relics.rs: "MUST be called BEFORE start_combat").
        seed_relics(&mut state, &[RelicId::BurningBlood]).expect("Burning Blood seeds");
    }
    let _ = start_combat(&mut state);
    let _ = start_player_turn(&mut state);
    state
}

/// One random fight on a fresh copy of `start`: (decisions, won, ended).
fn play(start: &CombatState, rep_seed: u64) -> (u64, bool, bool) {
    let mut state = start.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(rep_seed);
    let mut steps = 0u64;
    while !state.is_over && steps < 5_000 {
        let actions = legal_actions(&state);
        if actions.is_empty() {
            break;
        }
        let pick = rng.gen_range(0..actions.len());
        let _ = apply(&mut state, actions[pick].clone());
        steps += 1;
    }
    (steps, state.player_won, state.is_over)
}

fn main() {
    let args = Args::parse();
    let burning_blood = match args.relics.as_str() {
        "burning-blood" => true,
        "none" => false,
        other => panic!("--relics must be burning-blood or none, got {other:?}"),
    };
    let round_robin = match args.mix.as_str() {
        "round-robin" => true,
        "random" => false,
        other => panic!("--mix must be round-robin or random, got {other:?}"),
    };
    assert!(
        round_robin || args.encounters.is_none(),
        "--encounters needs --mix round-robin"
    );
    let list = encounter_list(args.encounters.as_deref());
    if round_robin {
        eprintln!(
            "encounters (seed s -> index s % {}): {}",
            list.len(),
            list.join(",")
        );
    } else {
        eprintln!("encounters: random-act1 draw");
    }
    // Content install BEFORE any start state: on the new engine the relic
    // channels freeze here and `RelicStore::push` then builds its plan.
    sts2_runai::rollout::ensure_content_installed();

    // Start states: built once, outside the timed region.
    let starts: Vec<CombatState> = (args.seed_start..args.seed_start + args.seeds)
        .map(|s| start_state(s, burning_blood, round_robin, &list, args.legacy_spawn))
        .collect();
    let n_relics = starts[0].relics.len();
    let _ = play(&starts[0], 1); // warm-up

    let mut fights = 0u64;
    let mut wins = 0u64;
    let mut decisions = 0u64;
    let mut anomalies = 0u64;
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let t0 = Instant::now();
    for (i, start) in starts.iter().enumerate() {
        for r in 0..args.reps {
            let (steps, won, ended) = play(start, (i as u64) * 1_000_003 + r);
            fights += 1;
            wins += u64::from(won);
            decisions += steps;
            anomalies += u64::from(!ended);
            digest = (digest ^ ((steps << 1) | u64::from(won))).wrapping_mul(0x0100_0000_01b3);
        }
    }
    let secs = t0.elapsed().as_secs_f64();

    // Only a filtered run names its list in the JSON, and only a run-path
    // start carries `spawn`, so a `--legacy-spawn` line stays byte-identical
    // to the pre-fix harness's (the 2026-10-14 published protocol's).
    let encounters_field = if args.encounters.is_some() {
        format!(",\"encounters\":\"{}\"", list.join(","))
    } else {
        String::new()
    };
    let spawn_field = if args.legacy_spawn {
        ""
    } else {
        ",\"spawn\":\"run\""
    };
    println!(
        "{{\"bench\":\"site_random_bench\"{},\"relics\":\"{}\",\"n_relics\":{},\"mix\":\"{}\",\"n_encounters\":{}{},\"seeds\":{},\"seed_start\":{},\"reps\":{},\"fights\":{},\"wins\":{},\"decisions\":{},\"anomalies\":{},\"digest\":\"{:016x}\",\"elapsed_s\":{:.4},\"fights_per_s\":{:.1},\"decisions_per_s\":{:.0}}}",
        spawn_field,
        args.relics,
        n_relics,
        args.mix,
        list.len(),
        encounters_field,
        args.seeds,
        args.seed_start,
        args.reps,
        fights,
        wins,
        decisions,
        anomalies,
        digest,
        secs,
        fights as f64 / secs,
        decisions as f64 / secs,
    );
}
