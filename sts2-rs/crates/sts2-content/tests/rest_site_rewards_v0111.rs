//! Rest-site heal rewards (`ExecuteRestSiteHeal` -> `ModifyRestSiteHealRewards` -> `OfferCustom`): Tiny Mailbox adds two
//! `PotionReward`s (`TinyMailbox.cs:14-23`). C# populates and claims them at the rest site, so the potions are rolled on the
//! `Rewards` stream there; the engine used to leave them on the pending queue until the next reward screen.
//!
//! Paired witnesses: v0.107.1 keeps the deferred queue, v0.111.0 claims at once (one free belt slot -> exactly one potion
//! gained, the second reward finds the belt full).

use sts2_core::potions::PotionKind;
use sts2_core::run_manager::RunManager;
use sts2_core::{GameVersion, RelicId};

fn rest_site_with_mailbox(version: GameVersion) -> RunManager {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rm = sts2_content::run_callbacks::new_run_manager_for_character(
        7,
        sts2_core::CharacterId::Ironclad,
    )
    .with_game_version(version);
    sts2_content::run_callbacks::initialize_run_setup(&mut rm);
    let mut rm = rm.with_neow_start();
    rm.run_state.relics.push(RelicId::TinyMailbox);
    rm.run_state.potions = vec![PotionKind::BlockPotion, PotionKind::StrengthPotion];
    rm.run_state.current_hp = rm.run_state.max_hp - 20;
    rm
}

fn rest(rm: &mut RunManager) {
    let _ = rm.do_rest_site_action(sts2_core::runstate::RestSiteOption::Rest);
}

#[test]
fn tiny_mailbox_potions_are_claimed_at_the_rest_site_on_v0111_only() {
    let mut rm = rest_site_with_mailbox(GameVersion::V0_111_0);
    let rewards_before = rm.run_state.rng.rewards.counter();
    rest(&mut rm);
    assert_eq!(
        rm.run_state.potions.len(),
        3,
        "the one free slot is filled at once"
    );
    assert!(
        rm.run_state.pending_rewards.is_empty(),
        "nothing left queued"
    );
    assert!(
        rm.run_state.rng.rewards.counter() > rewards_before,
        "rolled at the rest site"
    );

    let mut old = rest_site_with_mailbox(GameVersion::V0_107_1);
    rest(&mut old);
    assert_eq!(old.run_state.potions.len(), 2, "0.107.1: still queued");
    assert!(!old.run_state.pending_rewards.is_empty());
}
