//! Event→combat bridge content wiring (bead `sts2-rs-ck9z6`, GH #166).
//!
//! Covers the content half of the bridge: the affected event handlers
//! stage the right [`EventCombatRequest`], `build_event_combat` maps each
//! encounter to the correct monster roster (with its spawn powers), and the
//! BattlewornDummy resume grants the per-setting reward only on a clean
//! victory. The engine-side phase routing is covered in
//! `sts2-core::run_manager` tests; the timer itself in
//! `sts2-core::combat_loop`.

use sts2_content::events::dispatch;
use sts2_content::run_bridge::build_event_combat;
use sts2_core::events::{BattlewornDummySetting, EventEncounter};
use sts2_core::monsters::MonsterKind;
use sts2_core::{EventId, PowerKind, RelicId, RunState};

fn enemy_has_power(combat: &sts2_core::CombatState, kind: PowerKind) -> bool {
    combat
        .creatures
        .iter()
        .filter(|c| c.species.is_some())
        .any(|c| {
            c.powers.iter().any(|p| {
                combat
                    .powers
                    .get_by_id(p.0)
                    .is_some_and(|pw| pw.kind == kind)
            })
        })
}

fn first_enemy_kind(combat: &sts2_core::CombatState) -> MonsterKind {
    combat
        .creatures
        .iter()
        .find_map(|c| c.species)
        .expect("an enemy was spawned")
}

#[test]
fn battleworn_settings_stage_resuming_dummy_fight() {
    for (idx, setting) in [
        BattlewornDummySetting::Setting1,
        BattlewornDummySetting::Setting2,
        BattlewornDummySetting::Setting3,
    ]
    .into_iter()
    .enumerate()
    {
        let mut rs = RunState::new();
        rs.enter_event_room(EventId::BattlewornDummy);
        let out = dispatch::resolve_by_index(&mut rs, idx).expect("event active");
        assert!(!out.finished, "fight defers event finish to post-combat");
        let req = rs.event_combat.as_ref().expect("combat staged");
        assert!(req.resume, "BattlewornDummy resumes after combat");
        assert_eq!(req.parent_event, EventId::BattlewornDummy);
        assert!(req.victory_rewards.is_empty(), "reward rolled at resume");
        assert_eq!(req.encounter, EventEncounter::BattlewornDummy(setting));
        // current_event must stay open across the fight.
        assert_eq!(rs.current_event, Some(EventId::BattlewornDummy));
    }
}

#[test]
fn build_event_combat_spawns_correct_dummy_with_timer() {
    for (setting, kind, hp) in [
        (
            BattlewornDummySetting::Setting1,
            MonsterKind::BattleFriendV1,
            75,
        ),
        (
            BattlewornDummySetting::Setting2,
            MonsterKind::BattleFriendV2,
            150,
        ),
        (
            BattlewornDummySetting::Setting3,
            MonsterKind::BattleFriendV3,
            300,
        ),
    ] {
        let mut rs = RunState::new();
        let req = sts2_core::events::EventCombatRequest::resuming(
            EventEncounter::BattlewornDummy(setting),
            EventId::BattlewornDummy,
        );
        let combat = build_event_combat(&mut rs, &req);
        assert_eq!(first_enemy_kind(&combat), kind);
        let enemy = combat
            .creatures
            .iter()
            .find(|c| c.species == Some(kind))
            .expect("dummy present");
        assert_eq!(enemy.max_hp, hp, "fixed HP per setting");
        assert!(
            enemy_has_power(&combat, PowerKind::BattlewornDummyTimeLimit),
            "dummy carries the 3-turn time limit"
        );
    }
}

#[test]
fn build_event_combat_advances_niche_exactly_like_a_real_hp_roll() {
    // C# rolls each event-combat enemy's HP off the shared `niche` stream
    // (CreateCreature → SetUniqueMonsterHpValue), even when Min == Max.
    // build_event_combat must consume `niche` identically so the cursor stays
    // aligned with C# for everything drawn afterward.
    use sts2_core::monsters::roll_unique_max_hps;
    let kind = MonsterKind::BattleFriendV3;

    // Reference: roll HP directly off a fresh state's niche, then sample.
    let mut ref_rs = RunState::new();
    let ref_hp = roll_unique_max_hps(&[kind], &mut ref_rs.rng.niche, ref_rs.ascension_level)[0];
    let ref_after = ref_rs.rng.niche.next_int(0, 1_000_000);

    // build_event_combat on an identical fresh state must match both.
    let mut rs = RunState::new();
    let req = sts2_core::events::EventCombatRequest::resuming(
        EventEncounter::BattlewornDummy(BattlewornDummySetting::Setting3),
        EventId::BattlewornDummy,
    );
    let combat = build_event_combat(&mut rs, &req);
    let built_hp = combat
        .creatures
        .iter()
        .find(|c| c.species == Some(kind))
        .expect("dummy")
        .max_hp;
    let built_after = rs.rng.niche.next_int(0, 1_000_000);

    assert_eq!(built_hp, ref_hp, "HP value matches the canonical roll");
    assert_eq!(
        built_after, ref_after,
        "niche cursor advanced by exactly the HP roll (C# parity)"
    );
}

#[test]
fn build_event_combat_mysterious_knight_carries_spawn_buffs() {
    let mut rs = RunState::new();
    let req = sts2_core::events::EventCombatRequest::no_resume(
        EventEncounter::MysteriousKnight,
        EventId::TheLanternKey,
        Vec::new(),
    );
    let combat = build_event_combat(&mut rs, &req);
    assert_eq!(first_enemy_kind(&combat), MonsterKind::MysteriousKnight);
    // MysteriousKnight.AfterAddedToRoom applies Strength(6) + Plating(6).
    assert!(enemy_has_power(&combat, PowerKind::Strength));
    assert!(enemy_has_power(&combat, PowerKind::Plating));
}

#[test]
fn lantern_key_fight_stages_card_reward_no_resume() {
    let mut rs = RunState::new();
    rs.enter_event_room(EventId::TheLanternKey);
    // Page 0: KeepTheKey opens the Fight page (multi-page).
    let _ = dispatch::resolve(&mut rs, "keep_key").expect("event active");
    assert!(rs.event_combat.is_none(), "page 0 stages no fight");
    // Page 1: Fight stages the real combat.
    let out = dispatch::resolve(&mut rs, "fight").expect("event active");
    assert!(!out.finished);
    let req = rs.event_combat.as_ref().expect("combat staged");
    assert!(!req.resume, "TheLanternKey does not resume");
    assert_eq!(req.encounter, EventEncounter::MysteriousKnight);
    assert_eq!(req.victory_rewards.len(), 1, "LanternKey card reward");
    assert!(matches!(
        req.victory_rewards[0],
        sts2_core::runstate::Reward::Card(_)
    ));
}

#[test]
fn fake_merchant_throw_foul_stages_full_inventory_and_consumes_potion() {
    use sts2_core::potions::PotionKind;
    use sts2_core::runstate::Reward;
    let mut rs = RunState::new();
    // The player holds the FoulPotion they throw (+ an unrelated potion to
    // prove only one FoulPotion is consumed).
    rs.potions = vec![PotionKind::FoulPotion, PotionKind::FoulPotion];
    rs.enter_event_room(EventId::FakeMerchant);
    let out = dispatch::resolve(&mut rs, "throw_foul").expect("event active");
    assert!(!out.finished, "throw_foul defers finish to post-combat");

    // Exactly one FoulPotion consumed (the thrown one).
    assert_eq!(
        rs.potions,
        vec![PotionKind::FoulPotion],
        "one FoulPotion thrown/consumed"
    );

    let req = rs.event_combat.as_ref().expect("combat staged");
    assert!(!req.resume);
    assert_eq!(req.encounter, EventEncounter::FakeMerchant);
    // FakeMerchantsRug + the 6 stocked fake-inventory relics (C# extra rewards).
    assert_eq!(
        req.victory_rewards.len(),
        7,
        "Rug + 6 stocked fake relics, got {:?}",
        req.victory_rewards
    );
    assert!(
        matches!(&req.victory_rewards[0], Reward::Relic(rr) if rr.relic_id == RelicId::FakeMerchantsRug),
        "first extra reward is FakeMerchantsRug"
    );
    // Every reward is a relic, the 6 stock entries are distinct fake relics.
    let mut stock_ids: Vec<RelicId> = req.victory_rewards[1..]
        .iter()
        .map(|r| match r {
            Reward::Relic(rr) => rr.relic_id,
            other => panic!("non-relic extra reward: {other:?}"),
        })
        .collect();
    let before_dedup = stock_ids.len();
    stock_ids.sort_unstable();
    stock_ids.dedup();
    assert_eq!(stock_ids.len(), before_dedup, "stocked relics are distinct");
}

#[test]
fn fake_merchant_buy_purchases_a_stocked_fake_relic_for_50() {
    // The 9 fake-inventory relics (C# FakeMerchant._inventoryRelics).
    const FAKE_INVENTORY: [RelicId; 9] = [
        RelicId::FakeAnchor,
        RelicId::FakeBloodVial,
        RelicId::FakeHappyFlower,
        RelicId::FakeLeesWaffle,
        RelicId::FakeMango,
        RelicId::FakeOrichalcum,
        RelicId::FakeSneckoEye,
        RelicId::FakeStrikeDummy,
        RelicId::FakeVenerableTeaSet,
    ];
    let mut rs = RunState::new();
    rs.gold = 100;
    rs.direct_event_relic_grant = true; // a merchant purchase is obtained inline
    rs.enter_event_room(EventId::FakeMerchant);
    let out = dispatch::resolve(&mut rs, "buy").expect("event active");
    assert!(out.finished);
    assert_eq!(rs.gold, 50, "paid the 50-gold relic cost");
    assert_eq!(rs.relics.len(), 1, "obtained exactly one relic");
    assert!(
        FAKE_INVENTORY.contains(&rs.relics[0]),
        "bought a stocked fake-inventory relic, got {:?}",
        rs.relics[0]
    );
}

#[test]
fn battleworn_resume_grants_only_on_clean_victory() {
    // Setting1: clean victory queues a potion reward; a timeout grants nothing.
    let mut win = RunState::new();
    sts2_content::events::shared::battleworn_dummy_resume(
        &mut win,
        BattlewornDummySetting::Setting1,
        true,
    );
    assert_eq!(win.pending_rewards.len(), 1, "potion queued on victory");

    let mut timeout = RunState::new();
    sts2_content::events::shared::battleworn_dummy_resume(
        &mut timeout,
        BattlewornDummySetting::Setting1,
        false,
    );
    assert!(
        timeout.pending_rewards.is_empty(),
        "RanOutOfTime grants no reward"
    );

    // Setting2: clean victory upgrades cards in place; timeout leaves the
    // deck untouched. (Setting3's relic grant needs a populated grab bag —
    // exercised end-to-end via the run loop, not in this bare-state unit.)
    let mut up_win = RunState::new();
    up_win
        .master_deck
        .push(sts2_core::cards::Card::pod_defaults());
    sts2_content::events::shared::battleworn_dummy_resume(
        &mut up_win,
        BattlewornDummySetting::Setting2,
        false,
    );
    assert!(
        !up_win.master_deck[0].upgraded,
        "no upgrade on RanOutOfTime"
    );
}

// ── B12 (0c repair): DenseVegetation REST -> FIGHT page -> event combat ──

#[test]
fn dense_vegetation_rest_opens_fight_page_and_fight_stages_no_resume_combat() {
    let mut rs = RunState::new();
    rs.max_hp = 80;
    rs.current_hp = 40;
    rs.enter_event_room(EventId::DenseVegetation);
    let page0 = dispatch::present(&rs).expect("event active");
    assert_eq!(
        page0.iter().map(|o| o.option_id).collect::<Vec<_>>(),
        vec!["trudge_on", "rest"],
        "page 0 presents Trudge On / Rest only"
    );

    let out = dispatch::resolve(&mut rs, "rest").expect("event active");
    assert!(!out.finished, "REST opens a second page");
    assert_eq!(rs.current_hp, 64, "heal 30% of 80 = 24");
    assert_eq!(rs.current_event, Some(EventId::DenseVegetation));
    assert!(rs.event_combat.is_none(), "REST itself stages no fight");
    let page1 = dispatch::present(&rs).expect("event active");
    assert_eq!(
        page1.iter().map(|o| o.option_id).collect::<Vec<_>>(),
        vec!["fight"],
        "page 1 is exactly one FIGHT option"
    );

    let out = dispatch::resolve_by_index(&mut rs, 0).expect("event active");
    assert!(!out.finished);
    let req = rs.event_combat.as_ref().expect("combat staged");
    assert!(!req.resume, "shouldResumeAfterCombat: false");
    assert!(req.victory_rewards.is_empty(), "no extra rewards");
    assert!(
        req.monster_room_rewards,
        "normal Monster-room rewards follow"
    );
    assert_eq!(req.encounter, EventEncounter::DenseVegetation);
    assert_eq!(rs.current_event, Some(EventId::DenseVegetation));
}

#[test]
fn trudge_on_still_finishes_the_event() {
    let mut rs = RunState::new();
    rs.enter_event_room(EventId::DenseVegetation);
    let out = dispatch::resolve(&mut rs, "trudge_on").expect("event active");
    assert!(out.finished);
    assert!(rs.current_event.is_none());
}

#[test]
fn build_event_combat_dense_vegetation_spawns_four_wrigglers_with_slot_openers() {
    let mut rs = RunState::new();
    let req = sts2_core::events::EventCombatRequest::no_resume(
        EventEncounter::DenseVegetation,
        EventId::DenseVegetation,
        Vec::new(),
    );
    let combat = build_event_combat(&mut rs, &req);
    let wrigglers: Vec<_> = combat
        .creatures
        .iter()
        .filter(|c| c.species == Some(MonsterKind::DenseVegetationWriggler))
        .collect();
    assert_eq!(wrigglers.len(), 4, "wriggler1..wriggler4");
    for w in &wrigglers {
        assert!(
            (17..=21).contains(&w.max_hp),
            "A0 HP 17-21, got {}",
            w.max_hp
        );
    }
    // Slots 1/3 open on NASTY_BITE, slots 2/4 on WRIGGLE; StartStunned=false.
    let openers: Vec<&str> = wrigglers
        .iter()
        .map(|w| w.ai.as_ref().expect("ai").current_move().id)
        .collect();
    assert_eq!(
        openers,
        vec!["NASTY_BITE", "WRIGGLE", "NASTY_BITE", "WRIGGLE"]
    );
}

#[test]
fn dense_vegetation_niche_advance_is_four_unique_hp_rolls() {
    use sts2_core::monsters::roll_unique_max_hps;
    let kinds = [MonsterKind::DenseVegetationWriggler; 4];
    let mut ref_rs = RunState::new();
    let ref_hps = roll_unique_max_hps(&kinds, &mut ref_rs.rng.niche, ref_rs.ascension_level);
    let ref_after = ref_rs.rng.niche.next_int(0, 1_000_000);

    let mut rs = RunState::new();
    let req = sts2_core::events::EventCombatRequest::no_resume(
        EventEncounter::DenseVegetation,
        EventId::DenseVegetation,
        Vec::new(),
    );
    let combat = build_event_combat(&mut rs, &req);
    let hps: Vec<i32> = combat
        .creatures
        .iter()
        .filter(|c| c.species == Some(MonsterKind::DenseVegetationWriggler))
        .map(|c| c.max_hp)
        .collect();
    assert_eq!(hps, ref_hps);
    assert_eq!(rs.rng.niche.next_int(0, 1_000_000), ref_after);
}

#[test]
fn dense_vegetation_resume_callback_is_a_noop_for_the_no_resume_encounter() {
    let mut rs = RunState::new();
    let (gold, hp) = (rs.gold, rs.current_hp);
    dispatch::resume_event_after_combat_callback(&mut rs, &EventEncounter::DenseVegetation, true);
    assert_eq!((rs.gold, rs.current_hp), (gold, hp));
    assert!(rs.pending_rewards.is_empty());
}
