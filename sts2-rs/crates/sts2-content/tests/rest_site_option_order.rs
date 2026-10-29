//! Rest-site option order follows the order the relics were obtained in (v0.111.0).
//!
//! `Hook.ModifyRestSiteOptions` walks `Player.Relics`; the registry used to walk relic ids, so Shovel's
//! Dig always preceded Girya's Lift whichever relic came first.

use sts2_core::relics::RelicId;
use sts2_core::runstate::{RestSiteOption, RunState};
use sts2_core::GameVersion;

fn names(version: GameVersion, relics: &[RelicId]) -> Vec<&'static str> {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rs = RunState::new();
    rs.game_version = version;
    for &r in relics {
        rs.relics.push(r);
    }
    rs.rest_site_options()
        .iter()
        .map(|o| match o {
            RestSiteOption::Rest => "rest",
            RestSiteOption::Smith { .. } => "smith",
            RestSiteOption::Dig => "dig",
            RestSiteOption::Lift { .. } => "lift",
            _ => "other",
        })
        .collect()
}

#[test]
fn options_follow_relic_order_on_v0111_only() {
    let shovel_first = [RelicId::Shovel, RelicId::Girya];
    let girya_first = [RelicId::Girya, RelicId::Shovel];
    assert_eq!(
        names(GameVersion::V0_111_0, &shovel_first),
        ["rest", "smith", "dig", "lift"]
    );
    assert_eq!(
        names(GameVersion::V0_111_0, &girya_first),
        ["rest", "smith", "lift", "dig"]
    );
    // v0.107.1 is unchanged: the registry order, whichever relic came first.
    assert_eq!(
        names(GameVersion::V0_107_1, &shovel_first),
        names(GameVersion::V0_107_1, &girya_first)
    );
}
