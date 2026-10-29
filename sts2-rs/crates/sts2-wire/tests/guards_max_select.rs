//! The Guards screen reports the card's own `999999999` upper bound on v0.111.0 (the engine's working bound is
//! the hand size); v0.107.1 keeps reporting the clipped number.

use sts2_core::cards::CardId;
use sts2_core::{ChoiceKind, ChoiceOption, CombatState, GameVersion, PendingChoice};
use sts2_wire::emit::combat::pending_choice;

#[test]
fn guards_screen_reports_the_cards_own_max_on_v0111_only() {
    for (version, want) in [
        (GameVersion::V0_111_0, 999_999_999_u64),
        (GameVersion::V0_107_1, 2),
    ] {
        let mut state = CombatState {
            game_version: version,
            ..CombatState::default()
        };
        let options = vec![
            ChoiceOption {
                card_id: CardId::StrikeIronclad,
                upgraded: false,
                card_uid: None,
                source_pile: None,
            },
            ChoiceOption {
                card_id: CardId::DefendIronclad,
                upgraded: false,
                card_uid: None,
                source_pile: None,
            },
        ];
        state.pending_choice = Some(PendingChoice {
            kind: ChoiceKind::TransformInHandToMinionSacrifice,
            options,
            min_select: 0,
            max_select: 2,
            can_skip: true,
            ..PendingChoice::default()
        });
        let v = pending_choice(&state).expect("choice open");
        assert_eq!(v["max_select"].as_u64(), Some(want), "{version:?}");
    }
}
