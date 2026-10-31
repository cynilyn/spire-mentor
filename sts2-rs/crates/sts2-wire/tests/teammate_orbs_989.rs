//! GH #989: a teammate seat's orb belt survives the wire.
//!
//! Chaos channels orbs for any character, and a Frost orb blocks for 2 at its
//! owner's turn end. The wire carried only our belt, so a rebuilt teammate
//! had none and took 2 more damage than in the game on every enemy turn.

use serde_json::json;
use sts2_core::Orb;
use sts2_wire::emit::combat_state_to_value;
use sts2_wire::state_from_json;

fn with_teammate(extra: &serde_json::Value) -> serde_json::Value {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/combat_action_ironclad.json"),
    )
    .expect("fixture");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("fixture parses");
    let mut ally = json!({
        "class": "ironclad", "hp": 50, "max_hp": 70, "block": 0,
        "energy": 3, "max_energy": 3, "powers": [], "hand": [],
    });
    for (k, val) in extra.as_object().expect("object") {
        ally[k] = val.clone();
    }
    v["allies"] = json!([ally]);
    v
}

fn frost() -> serde_json::Value {
    json!({"type": "Frost", "passive": 2, "evoke": 5})
}

#[test]
fn a_teammates_orbs_fill_their_belt_and_round_trip() {
    let v = with_teammate(&json!({"orbs": [frost()], "orb_slots": 3}));
    let st = state_from_json(&v).expect("builds");
    let belt = &st.allies[0].orbs;
    assert_eq!(belt.capacity, 3);
    assert_eq!(belt.slots.as_slice(), &[Orb::Frost, Orb::Empty, Orb::Empty]);
    assert_eq!(st.player.orbs.capacity, 0, "ours untouched");
    let out = combat_state_to_value(&st).expect("emits");
    assert_eq!(out["allies"][0]["orb_slots"], 3);
    assert_eq!(out["allies"][0]["orbs"][0]["type"], "Frost");
    assert_eq!(out["allies"][0]["orbs"][0]["passive"], 2);
    let back = state_from_json(&out).expect("re-parses");
    assert_eq!(back.allies[0].orbs, st.allies[0].orbs);
}

#[test]
fn a_teammates_focus_scales_their_own_orbs() {
    let v = with_teammate(&json!({
        "orbs": [frost()],
        "orb_slots": 1,
        "powers": [{"name": "FOCUS", "amount": 2}],
    }));
    let st = state_from_json(&v).expect("builds");
    let out = combat_state_to_value(&st).expect("emits");
    assert_eq!(out["allies"][0]["orbs"][0]["passive"], 4);
    assert_eq!(out["allies"][0]["orbs"][0]["evoke"], 7);
}

#[test]
fn a_teammate_without_orbs_emits_none() {
    let st = state_from_json(&with_teammate(&json!({}))).expect("builds");
    assert_eq!(st.allies[0].orbs.capacity, 0);
    let out = combat_state_to_value(&st).expect("emits");
    assert!(out["allies"][0].get("orbs").is_none());
}
