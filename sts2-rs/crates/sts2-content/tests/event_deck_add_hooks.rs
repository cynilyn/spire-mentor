//! A card an event adds to the deck goes through the deck-add hooks (v0.111.0).
//!
//! C# `CardPileCmd.Add(card, Deck)` runs `ModifyCardBeingAddedToDeck` (the eggs) and the
//! `AfterCardChangedPiles` listeners (Lucky Fysh's 15 gold); the event handlers used to push
//! onto the deck directly, skipping both.

use sts2_core::events::EventId;
use sts2_core::relics::RelicId;
use sts2_core::runstate::RunState;
use sts2_core::GameVersion;

fn gold_after_bugslayer(version: GameVersion) -> i32 {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rs = RunState::new();
    rs.game_version = version;
    rs.gold = 100;
    rs.relics.push(RelicId::LuckyFysh);
    rs.enter_event_room(EventId::Bugslayer);
    let deck = rs.master_deck.len();
    let _ = sts2_content::events::dispatch::resolve(&mut rs, "exterminate").expect("event active");
    assert_eq!(rs.master_deck.len(), deck + 1);
    rs.gold - 100
}

#[test]
fn bugslayer_card_pays_lucky_fysh_on_v0111_only() {
    assert_eq!(gold_after_bugslayer(GameVersion::V0_111_0), 15);
    assert_eq!(gold_after_bugslayer(GameVersion::V0_107_1), 0);
}
