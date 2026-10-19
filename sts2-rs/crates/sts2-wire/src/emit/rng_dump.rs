//! `RunManager.rng_dump` — the Rust-side counterpart of the oracle's
//! `dump_rng_trace` (v0.111.0 seed-hash port, stage 1, W1-W4).
//!
//! Same JSON schema as the oracle (`RunSimulator.RngTrace.cs`): top-level
//! `seed_string`/`input_seed`/`prefix`/`run_seed`/`player_seed`/`n`/`schedule`/
//! `streams`/`models` plus the Rust-only `src_hash` (added by the caller).
//! 22 streams for a Neow-screen run: 12 run streams + 3 player streams +
//! `act_selection` + `act_1/2/3_map` + `event:NEOW` + `event:<first event of
//! the current act>` + `encounter:<first normal encounter of the current
//! act>`, plus the Rust-only `run_manager`.
//!
//! **Read-only.** Every draw runs on a clone: `draws` on a fresh generator at
//! the stream's seed (counter 0), `next_draws` on a clone of the live object.
//! `current` / `next_draws` are present only while a live object exists
//! (the 12 run streams, the 3 player streams, `run_manager`, `event:NEOW`
//! while its `Rng` is retained, and the open event's own stream); derived
//! streams that the game builds and drops (`act_selection`, `act_N_map`,
//! `encounter:*`) carry `null`.
//!
//! Call mapping (design `stage1_design.md` section 5): `NextInt(min,max)` and
//! `NextInt(n)` are half-open (`next_int_exclusive`), `NextFloat()` is
//! `next_float(1.0)` rounded to `f32`, never `sample()`.

use serde_json::{Map, Value};
use sts2_core::dotnet_rng::{xxhash64_utf8, Rng};
use sts2_core::run_manager::RunManager;

use super::map_ext::{obj, set};

/// One scheduled call (oracle `TraceCall`).
#[derive(Clone, Debug, PartialEq)]
pub struct TraceCall {
    /// `NextInt` | `NextFloat` | `NextDouble` | `NextBool`.
    pub call: String,
    /// Integer arguments (`NextInt` takes 1 or 2).
    pub args: Vec<i64>,
}

impl TraceCall {
    /// The oracle's label: `NextInt(min,max)`, `NextInt(n)`, `NextFloat()`, ...
    #[must_use]
    pub fn label(&self) -> String {
        match (self.call.as_str(), self.args.len()) {
            ("NextInt", 1) => "NextInt(n)".to_owned(),
            ("NextInt", 2) => "NextInt(min,max)".to_owned(),
            (c, _) => format!("{c}()"),
        }
    }

    /// Validate a `(call, args)` pair the way the oracle's `ParseTraceCall`
    /// does; `None` for anything it would reject.
    #[must_use]
    pub fn parse(call: &str, args: &[i64]) -> Option<Self> {
        let ok = match call {
            "NextInt" => args.len() == 1 || args.len() == 2,
            "NextFloat" | "NextDouble" | "NextBool" => args.is_empty(),
            _ => false,
        };
        if !ok {
            return None;
        }
        // `NextInt` ranges must be non-empty or the draw would panic.
        if call == "NextInt" {
            let (lo, hi) = if args.len() == 1 {
                (0, args[0])
            } else {
                (args[0], args[1])
            };
            if lo >= hi || i32::try_from(lo).is_err() || i32::try_from(hi).is_err() {
                return None;
            }
        }
        Some(Self {
            call: call.to_owned(),
            args: args.to_vec(),
        })
    }
}

/// The oracle's default schedule, cycled over the `n` draws.
#[must_use]
pub fn default_schedule() -> Vec<TraceCall> {
    vec![
        TraceCall {
            call: "NextInt".into(),
            args: vec![0, 100],
        },
        TraceCall {
            call: "NextFloat".into(),
            args: vec![],
        },
        TraceCall {
            call: "NextInt".into(),
            args: vec![100],
        },
    ]
}

/// C# `StringHelper.Slugify`: `CamelCaseRegex` `([A-Za-z0-9]|\G(?!^))([A-Z])`
/// -> `$1_$2`, upper-case, whitespace -> `_`, drop everything outside
/// `[A-Z0-9_]`. Used to turn an `EventId::python_name` into the C# `Id.Entry`.
#[must_use]
pub fn csharp_slugify(name: &str) -> String {
    let chars: Vec<char> = name.trim().chars().collect();
    let mut out = String::with_capacity(chars.len() + 4);
    let mut i = 0;
    // `\G` anchors an empty match at the end of the previous match.
    let mut prev_end: Option<usize> = None;
    while i < chars.len() {
        if chars[i].is_ascii_alphanumeric()
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_uppercase()
        {
            out.push(chars[i]);
            out.push('_');
            out.push(chars[i + 1]);
            i += 2;
            prev_end = Some(i);
        } else if i > 0 && prev_end == Some(i) && chars[i].is_ascii_uppercase() {
            out.push('_');
            out.push(chars[i]);
            i += 1;
            prev_end = Some(i);
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out.to_uppercase()
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
        .collect()
}

fn state_value(rng: &Rng) -> Value {
    let [s0, s1, s2, s3] = rng.state();
    let mut d = obj();
    set(&mut d, "state0", s0);
    set(&mut d, "state1", s1);
    set(&mut d, "state2", s2);
    set(&mut d, "state3", s3);
    set(&mut d, "counter", rng.counter());
    Value::Object(d)
}

/// `n` draws of `schedule` (cycled) on a clone of `from`.
fn draws(from: &Rng, n: usize, schedule: &[TraceCall]) -> Value {
    let mut r = from.clone();
    let mut list = Vec::with_capacity(n);
    for i in 0..n {
        let c = &schedule[i % schedule.len()];
        let value: Value = match (c.call.as_str(), c.args.len()) {
            ("NextInt", 1) => Value::from(r.next_int_exclusive(0, arg_i32(c.args[0]))),
            ("NextInt", _) => {
                Value::from(r.next_int_exclusive(arg_i32(c.args[0]), arg_i32(c.args[1])))
            }
            // f32 shortest-form on the oracle side; compare after rounding to f32.
            ("NextFloat", _) => Value::from(f64::from(r.next_float(1.0) as f32)),
            ("NextDouble", _) => Value::from(r.next_double()),
            _ => Value::from(r.next_bool()),
        };
        let mut e = obj();
        set(&mut e, "call", c.label());
        set(&mut e, "args", &c.args);
        e.insert("value".to_owned(), value);
        list.push(Value::Object(e));
    }
    Value::Array(list)
}

#[allow(clippy::cast_possible_truncation)]
const fn arg_i32(v: i64) -> i32 {
    // `TraceCall::parse` already rejected out-of-range values.
    v as i32
}

/// One `streams.<name>` entry. `seed` is the stream's construction seed;
/// `live` is the engine's own object when it holds one.
fn stream(
    scope: &str,
    formula: &str,
    seed: u64,
    live: Option<&Rng>,
    n: usize,
    sch: &[TraceCall],
) -> Value {
    let initial = Rng::new(seed);
    let mut d = obj();
    set(&mut d, "scope", scope);
    set(&mut d, "seed_formula", formula);
    set(&mut d, "seed", seed);
    d.insert("initial".to_owned(), state_value(&initial));
    d.insert("draws".to_owned(), draws(&initial, n, sch));
    if let Some(l) = live {
        d.insert("current".to_owned(), state_value(l));
        d.insert("next_draws".to_owned(), draws(l, n, sch));
    } else {
        d.insert("current".to_owned(), Value::Null);
        d.insert("next_draws".to_owned(), Value::Null);
    }
    Value::Object(d)
}

fn model(name: &str, streams: &[String], note: &str) -> Value {
    let mut d = obj();
    set(&mut d, "model", name);
    set(&mut d, "streams", streams);
    set(&mut d, "note", note);
    Value::Object(d)
}

/// `RunManager.rng_dump` payload (everything except the Rust-only `src_hash`,
/// which the binding appends). `seed_string` is the string the run was built
/// from (`None` for a numeric-seed run); `predict_next_floor` adds the `+1`
/// the oracle applies to the `encounter:*` prediction.
#[must_use]
pub fn rng_dump(
    rm: &RunManager,
    n: usize,
    schedule: Option<&[TraceCall]>,
    predict_next_floor: bool,
    seed_string: Option<&str>,
) -> Value {
    let default = default_schedule();
    let sch: &[TraceCall] = match schedule {
        Some(s) if !s.is_empty() => s,
        _ => &default,
    };
    let rs = &rm.run_state;
    let rr = &rs.rng;
    let run_seed = rr.seed;
    let player_seed = rr.player_seed;
    let h = xxhash64_utf8;
    let mut streams = Map::new();

    // Run-level streams (`RunRngType`, in the oracle's enum order).
    let run_streams: [(&str, &Rng); 12] = [
        ("up_front", &rr.up_front),
        ("shuffle", &rr.shuffle),
        ("unknown_map_point", &rr.unknown_map_point),
        ("combat_card_generation", &rr.combat_card_generation),
        ("combat_potion_generation", &rr.combat_potion_generation),
        ("combat_card_selection", &rr.combat_card_selection),
        ("combat_energy_costs", &rr.combat_energy_costs),
        ("combat_targets", &rr.combat_targets),
        ("monster_ai", &rr.monster_ai),
        ("niche", &rr.niche),
        ("combat_orbs", &rr.combat_orbs),
        ("treasure_room_relics", &rr.treasure_room),
    ];
    for (name, live) in run_streams {
        streams.insert(
            name.to_owned(),
            stream(
                "run",
                "run_seed + hash(name)",
                run_seed.wrapping_add(h(name)),
                Some(live),
                n,
                sch,
            ),
        );
    }
    // Player-level streams.
    let player_streams: [(&str, &Rng); 3] = [
        ("rewards", &rr.rewards),
        ("shops", &rr.shops),
        ("transformations", &rr.transformations),
    ];
    for (name, live) in player_streams {
        streams.insert(
            name.to_owned(),
            stream(
                "player",
                "player_seed + hash(name)",
                player_seed.wrapping_add(h(name)),
                Some(live),
                n,
                sch,
            ),
        );
    }
    // Derived streams the game builds on demand.
    streams.insert(
        "act_selection".to_owned(),
        stream(
            "derived",
            "hash(seed_string) + hash(\"act_selection\")",
            rm.act_seed.wrapping_add(h("act_selection")),
            None,
            n,
            sch,
        ),
    );
    for a in 1..=3 {
        let nm = format!("act_{a}_map");
        let seed = run_seed.wrapping_add(h(&nm));
        streams.insert(
            nm,
            stream(
                "derived",
                "run_seed + hash(\"act_N_map\")",
                seed,
                None,
                n,
                sch,
            ),
        );
    }

    // Event streams: `run_seed + slot 0 + H(event id)`. The first event of
    // the *current* act is the oracle's `rooms.events.FirstOrDefault()`.
    let act_rooms = rs
        .generated_rooms
        .get(usize::from(rs.act_index).saturating_sub(1));
    let act_event_id = act_rooms
        .and_then(|g| g.events_shuffled.first())
        .map(|py| csharp_slugify(py))
        .unwrap_or_default();
    let enc_id = act_rooms
        .and_then(|g| g.normal_encounters.first())
        .map(|s| (*s).to_owned())
        .unwrap_or_default();
    let open_event_entry = rs.current_event.map(|e| csharp_slugify(e.python_name()));
    let mut event_ids: Vec<String> = vec!["NEOW".to_owned()];
    if !act_event_id.is_empty() && act_event_id != "NEOW" {
        event_ids.push(act_event_id.clone());
    }
    for eid in &event_ids {
        let live: Option<&Rng> = if eid == "NEOW" {
            // Live only while the Neow event is the open event.
            if rs.current_event == Some(sts2_core::events::EventId::Neow) {
                rs.neow_event_rng.as_ref()
            } else {
                None
            }
        } else if open_event_entry.as_deref() == Some(eid.as_str()) {
            rs.event_state.rng.as_ref()
        } else {
            None
        };
        streams.insert(
            format!("event:{eid}"),
            stream(
                "derived",
                "run_seed + slot + hash(event_id)",
                run_seed.wrapping_add(h(eid)),
                live,
                n,
                sch,
            ),
        );
    }
    // Encounter prediction: runtime `TotalFloor` is `total_floor + act_index`
    // (C# counts the Ancient event as a history entry); the oracle dumps the
    // *next* floor, `+ 1`.
    if !enc_id.is_empty() {
        let floor =
            i64::from(rs.total_floor) + i64::from(rs.act_index) + i64::from(predict_next_floor);
        #[allow(clippy::cast_sign_loss)]
        let floor_u = floor.max(0) as u64;
        streams.insert(
            format!("encounter:{enc_id}"),
            stream(
                "derived",
                "run_seed + total_floor + hash(encounter_id), total_floor = current+1",
                run_seed.wrapping_add(floor_u).wrapping_add(h(&enc_id)),
                None,
                n,
                sch,
            ),
        );
    }
    // Rust-only: the run manager's own stream (no C# analog).
    streams.insert(
        "run_manager".to_owned(),
        stream(
            "rust_only",
            "run_seed + 9999 (Rust-only, no hash)",
            rr.run_manager.seed(),
            Some(&rr.run_manager),
            n,
            sch,
        ),
    );

    let s = |names: &[&str]| -> Vec<String> { names.iter().map(|x| (*x).to_owned()).collect() };
    let enc_stream = if enc_id.is_empty() {
        "encounter:<id>".to_owned()
    } else {
        format!("encounter:{enc_id}")
    };
    let models = vec![
        model("neow", &s(&["event:NEOW"]), "EventModel.Rng for the NEOW event (options + coin flips)"),
        model(
            "map",
            &s(&["act_1_map", "act_2_map", "act_3_map"]),
            "StandardActMap(Rng(seed,\"act_N_map\")); built when an act is entered, not held afterwards",
        ),
        model(
            "encounter",
            &["up_front".to_owned(), enc_stream],
            "room lists come from up_front (RoomSet generation); monster composition from EncounterModel._rng",
        ),
        model("card_reward", &s(&["rewards"]), "CardFactory.CreateForReward draws player.PlayerRng.Rewards (shop cards use shops)"),
        model("relic_reward", &s(&["rewards", "up_front"]), "RelicFactory.RollRarity draws rewards; grab-bag fill/shuffle uses up_front at run start"),
        model("potion_reward", &s(&["rewards"]), "PotionReward draws player.PlayerRng.Rewards"),
        model("shop", &s(&["shops"]), "MerchantInventory draws player.PlayerRng.Shops (+ rewards for relic rarity)"),
        model(
            "event",
            &if act_event_id.is_empty() {
                s(&["unknown_map_point"])
            } else {
                vec!["unknown_map_point".to_owned(), format!("event:{act_event_id}")]
            },
            "'?' room type from unknown_map_point; event content from the per-event EventModel.Rng",
        ),
        model("combat_shuffle", &s(&["shuffle"]), "RunRngSet.Shuffle: draw-pile shuffles and random-position inserts"),
        model(
            "monster_hp",
            &s(&["niche"]),
            "Creature.SetUniqueMonsterHpValue(RunState.Rng.Niche); monster.Rng (live_monster:*) drives monster AI picks",
        ),
    ];

    let mut top = obj();
    set(&mut top, "type", "rng_trace");
    if let Some(raw) = seed_string {
        let b = sts2_core::seed_bases(raw);
        set(&mut top, "seed_string", raw);
        set(&mut top, "input_seed", &raw[b.prefix.len()..]);
        set(&mut top, "prefix", b.prefix);
    } else {
        top.insert("seed_string".to_owned(), Value::Null);
        top.insert("input_seed".to_owned(), Value::Null);
        top.insert("prefix".to_owned(), Value::Null);
    }
    set(&mut top, "run_seed", run_seed);
    set(&mut top, "player_seed", player_seed);
    set(&mut top, "n", n);
    set(
        &mut top,
        "schedule",
        sch.iter()
            .map(|c| {
                let mut e = obj();
                set(&mut e, "call", c.label());
                set(&mut e, "args", &c.args);
                Value::Object(e)
            })
            .collect::<Vec<_>>(),
    );
    top.insert("streams".to_owned(), Value::Object(streams));
    top.insert("models".to_owned(), Value::Array(models));
    Value::Object(top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_csharp_regex() {
        assert_eq!(csharp_slugify("FurCoat"), "FUR_COAT");
        assert_eq!(csharp_slugify("WhisperingHollow"), "WHISPERING_HOLLOW");
        assert_eq!(csharp_slugify("RanwidTheElder"), "RANWID_THE_ELDER");
        assert_eq!(
            csharp_slugify("TheFutureOfPotions"),
            "THE_FUTURE_OF_POTIONS"
        );
        assert_eq!(csharp_slugify("Neow"), "NEOW");
        // Consecutive capitals: each boundary gets an underscore (`\G` anchor).
        assert_eq!(csharp_slugify("ABc"), "A_BC");
        assert_eq!(csharp_slugify("ABC"), "A_B_C");
    }

    #[test]
    fn parse_rejects_what_the_oracle_rejects() {
        assert!(TraceCall::parse("NextInt", &[0, 100]).is_some());
        assert!(TraceCall::parse("NextInt", &[100]).is_some());
        assert!(TraceCall::parse("NextFloat", &[]).is_some());
        assert!(TraceCall::parse("NextFloat", &[1]).is_none());
        assert!(TraceCall::parse("NextInt", &[]).is_none());
        assert!(TraceCall::parse("NextInt", &[5, 5]).is_none());
        assert!(TraceCall::parse("Bogus", &[]).is_none());
    }
}
