//! R1/intents: the combat wire telegraphs the WHOLE intent list of the current move (C# `MoveState`
//! `params AbstractIntent[]`, `MoveState.cs:48-53`) on v0.111.0, and stays head-only on v0.107.1.
//!
//! `fixtures/intent_lists_v0111.json` is generated from the C# v0.111.0 `Models/Monsters/*.cs`
//! `new MoveState("ID", Fn, new XIntent(..), new YIntent(..))` declarations (every move with two or
//! more intents; `cs_ref` is the declaring line), so the expected kinds come from C#, not from the port.

use serde_json::Value;
use sts2_content::monster_dispatch::{build_ai_for_version, MonsterSpawnArgs, SpawnPosition};
use sts2_core::{CombatState, GameVersion};
use sts2_wire::emit::combat::combat_state_to_value;
use sts2_wire::state_dict::monster_kind_from_python_name;

const FIXTURE: &str = include_str!("fixtures/intent_lists_v0111.json");

/// The wire `encounter[0].intents[*].kind` of `monster` parked on its `state_id` move.
fn wire_kinds(monster: &str, state_id: &str, version: GameVersion) -> Vec<String> {
    let kind = monster_kind_from_python_name(monster)
        .unwrap_or_else(|| panic!("{monster} is a known monster"));
    let mut state = CombatState {
        game_version: version,
        ..CombatState::default()
    };
    state.player.current_hp = 70;
    state.player.max_hp = 70;
    state.init_player_creature();
    let uid = state.spawn_enemy(kind);
    let creature = state
        .creatures
        .iter_mut()
        .find(|c| c.id == uid.0)
        .expect("enemy spawned");
    let mut ai = build_ai_for_version(
        kind,
        MonsterSpawnArgs::default(),
        SpawnPosition {
            alone: true,
            front: true,
            back: false,
            index: 0,
        },
        0,
        version,
    );
    assert!(
        ai.force_current_state_by_name(state_id),
        "{monster} has no move state {state_id}"
    );
    creature.intent = ai.current_move().intents.first().copied();
    creature.ai = Some(ai);
    let wire = combat_state_to_value(&state).expect("combat_state_to_value");
    wire["encounter"][0]["intents"]
        .as_array()
        .expect("intents is a list")
        .iter()
        .map(|i| i["kind"].as_str().expect("kind").to_owned())
        .collect()
}

#[test]
fn every_multi_intent_move_telegraphs_its_c_sharp_list_v0111() {
    let fixture: Vec<Value> = serde_json::from_str(FIXTURE).expect("fixture parses");
    assert!(
        fixture.len() >= 80,
        "the C# scan found 81 multi-intent moves"
    );
    let mut bad = Vec::new();
    for entry in &fixture {
        let monster = entry["monster"].as_str().unwrap();
        let state_id = entry["state"].as_str().unwrap();
        let want: Vec<String> = entry["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect();
        let got = wire_kinds(monster, state_id, GameVersion::V0_111_0);
        if got != want {
            bad.push(format!(
                "{monster}.{state_id} ({}): want {want:?}, wire {got:?}",
                entry["cs_ref"].as_str().unwrap()
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// v0.107.1 keeps the single-element wire its recorded goldens pin (and its `Debuff` spelling of
/// the card-debuff icon).
#[test]
fn v0107_wire_stays_head_only_and_unchanged() {
    assert_eq!(
        wire_kinds("Axebot", "BOOT_UP", GameVersion::V0_107_1),
        vec!["Defend"]
    );
    assert_eq!(
        wire_kinds("Axebot", "BOOT_UP", GameVersion::V0_111_0),
        vec!["Defend", "Buff"]
    );
    // A card-debuff head names itself `Debuff` on the old wire.
    assert_eq!(
        wire_kinds("Queen", "PUPPET_STRINGS_MOVE", GameVersion::V0_107_1),
        vec!["Debuff"]
    );
    assert_eq!(
        wire_kinds("Queen", "PUPPET_STRINGS_MOVE", GameVersion::V0_111_0),
        vec!["CardDebuff"]
    );
}

/// ThievingHopper THIEVERY is the move the user asked about: `SingleAttackIntent` +
/// `CardDebuffIntent` (`ThievingHopper.cs:164`), attack first.
#[test]
fn thieving_hopper_thievery_is_attack_then_card_debuff_v0111() {
    assert_eq!(
        wire_kinds("ThievingHopper", "THIEVERY_MOVE", GameVersion::V0_111_0),
        vec!["Attack", "CardDebuff"]
    );
}

/// A state rebuilt from a wire dict keeps the move's full declared list on v0.111.0 even when the
/// ingested head only matches the declared head in kind (Tunneler BURROW `[Buff, Defend]`).
#[test]
fn round_tripped_burrow_keeps_both_intents_v0111() {
    use sts2_content::monster_dispatch::{build_ai_for_version, MonsterSpawnArgs, SpawnPosition};
    let kind = monster_kind_from_python_name("Tunneler").expect("known monster");
    let mut state = CombatState {
        game_version: GameVersion::V0_111_0,
        ..CombatState::default()
    };
    state.player.current_hp = 70;
    state.player.max_hp = 70;
    state.init_player_creature();
    let uid = state.spawn_enemy(kind);
    let creature = state
        .creatures
        .iter_mut()
        .find(|c| c.id == uid.0)
        .expect("enemy");
    let mut ai = build_ai_for_version(
        kind,
        MonsterSpawnArgs::default(),
        SpawnPosition {
            alone: true,
            front: true,
            back: false,
            index: 0,
        },
        0,
        GameVersion::V0_111_0,
    );
    assert!(ai.force_current_state_by_name("BURROW_MOVE"));
    creature.intent = ai.current_move().intents.first().copied();
    creature.ai = Some(ai);
    let wire = combat_state_to_value(&state).expect("emit");
    let text = serde_json::to_string(&wire).expect("json");
    let back = sts2_wire::state_from_json_str(&text).expect("ingest");
    let again = combat_state_to_value(&back).expect("emit again");
    let kinds: Vec<String> = again["encounter"][0]["intents"]
        .as_array()
        .expect("intents")
        .iter()
        .map(|i| i["kind"].as_str().expect("kind").to_owned())
        .collect();
    assert_eq!(kinds, vec!["Buff", "Defend"]);
}
