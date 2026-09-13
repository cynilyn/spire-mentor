//! A captured wire dict goes from JSON text to a live `CombatState` with no
//! Python interpreter anywhere in the process.
//!
//! This is the property the crate exists for. Before the split, reaching
//! `CombatState` from a wire dict meant linking `libpython` and holding the
//! GIL, so nothing outside the `PyO3` extension could replay a capture. The
//! test crate here depends only on `sts2-wire` — if a `pyo3` symbol ever
//! creeps back into the parse path, this stops linking.

use sts2_wire::{parse_json, state_from_json_str, WireError};

fn fixture() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/combat_action_ironclad.json"),
    )
    .expect("fixture readable")
}

#[test]
fn a_captured_dict_parses_without_python() {
    let text = fixture();
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let parsed = parse_json(&value).expect("fixture must parse");

    assert_eq!(parsed.wire_version, 1);
    assert_eq!(parsed.player.class, "ironclad");
    assert_eq!(parsed.player.hp, 54);
    assert_eq!(parsed.player.relics.len(), 2);
    assert_eq!(parsed.player.gold, Some(137));
    assert_eq!(parsed.deck.hand.len(), 2);
    assert_eq!(parsed.deck.draw_pile.len(), 3);
    assert_eq!(parsed.deck.pinned_top, vec![1]);
    assert_eq!(parsed.encounter.len(), 2);
    assert_eq!(parsed.ascension, 7);

    // A `u64` seed above `i64::MAX` — the case a naive `as_i64` read would
    // have silently rejected.
    assert_eq!(parsed.rng_seed, Some(18_446_744_073_709_551_557));

    // Key-sorted, per the `WireValue::entries` contract.
    // Sorted, though the fixture lists them shuffle / monster_ai /
    // combat_targets.
    let counter_names: Vec<&str> = parsed
        .rng_counters
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(counter_names, ["combat_targets", "monster_ai", "shuffle"]);
}

#[test]
fn a_captured_dict_rebuilds_a_combat_state() {
    let state = state_from_json_str(&fixture()).expect("fixture must rebuild");

    assert_eq!(state.player.current_hp, 54);
    assert_eq!(state.player.max_hp, 80);
    assert_eq!(state.player.block, 5);
    // Two enemies on the wire, one of them dead.
    assert_eq!(state.enemy_uids.len(), 2);
    // The piles came across, not just the scalars.
    assert_eq!(state.player.hand.len(), 2);
    assert_eq!(state.player.draw.len(), 3);
    assert_eq!(state.player.discard.len(), 1);
}

/// The synthetic `status:` / `curse:` ids resolve to their real cards rather
/// than degrading to the AscendersBane placeholder — the single largest
/// reconstruction gap the DLL path ever had, and worth pinning on the
/// interpreter-free route too.
#[test]
fn synthetic_status_and_curse_ids_resolve() {
    let value: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
    let parsed = parse_json(&value).unwrap();

    let slimed = parsed
        .deck
        .draw_pile
        .iter()
        .find(|c| c.raw_id == "status:Slimed")
        .expect("slimed in draw pile");
    assert!(
        slimed.card_id.is_some(),
        "`status:Slimed` must resolve to a real CardId, not fall through to the placeholder"
    );

    let clumsy = parsed
        .deck
        .hand
        .iter()
        .find(|c| c.raw_id == "curse:Clumsy")
        .expect("clumsy in hand");
    assert!(clumsy.card_id.is_some(), "`curse:Clumsy` must resolve");
}

#[test]
fn failures_carry_their_path_and_class() {
    let mut value: serde_json::Value = serde_json::from_str(&fixture()).unwrap();

    // Unmodelled content → Unsupported.
    value["player"]["relics"][1]["id"] = serde_json::json!("DEFINITELY_NOT_A_RELIC");
    let err = parse_json(&value).unwrap_err();
    assert!(matches!(err, WireError::Unsupported { .. }), "got {err:?}");
    assert_eq!(err.path(), "$.player.relics[1]");
    assert!(err.to_string().contains("DEFINITELY_NOT_A_RELIC"));

    // Invariant violation → Schema.
    let mut value: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
    value["player"]["hp"] = serde_json::json!(999);
    let err = parse_json(&value).unwrap_err();
    assert!(matches!(err, WireError::Schema { .. }), "got {err:?}");
    assert_eq!(err.path(), "$.player");

    // Missing required key → Schema, at the parent path.
    let mut value: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
    value.as_object_mut().unwrap().remove("phase");
    let err = parse_json(&value).unwrap_err();
    assert_eq!(err.path(), "$");
    assert!(err.to_string().contains("missing required key `phase`"));
}

#[test]
fn malformed_json_text_fails_at_the_root() {
    let err = state_from_json_str("{not json").unwrap_err();
    assert_eq!(err.path(), "$");
    assert!(err.to_string().contains("invalid JSON"));
}
