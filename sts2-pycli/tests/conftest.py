"""Pytest fixtures: Game process wrapper for unit tests.

The ``Game`` class itself lives in ``python/sts2_client.py``; this module just
binds it as a pytest fixture. ``python/`` is added to ``sys.path`` via the
``pythonpath`` setting in ``pyproject.toml``.

Two kinds of inputs are absent from a bring-your-own-game checkout (the public
export) until ``just import-game`` has run, and the suite must degrade to
SKIPS — never to failures — without them:

* The C# oracle links against the player's own game assembly
  (``lib/sts2.dll``; never published). Tests that need it carry the ``oracle``
  marker — applied here to every user of the ``game`` fixture — and are
  skipped when the assembly is absent, so ``pytest -m "not oracle"`` is the
  game-free subset and a plain ``pytest`` degrades to skips instead of ``EOF
  from game process`` noise. Tests that build ``Game()`` themselves are skipped
  the same way when the constructor raises ``OracleUnavailable``.
* Game-derived data files (``localization_eng/``, the sibling wiki's
  ``data/<ver>/``, ``workbench/data/card_db.json``), training checkpoints
  (``data/ppo/*.pt``) and private deploy artifacts (root ``Dockerfile``,
  ``web/wrangler.toml``). ``_NEEDS`` below maps the test modules / tests that
  read them to the file that must exist; they get the ``game_data`` marker and
  skip when it is missing.
"""

from pathlib import Path

import pytest
from sts2_client import Game, OracleUnavailable, oracle_available

ORACLE_SKIP_REASON = "C# oracle unavailable: lib/sts2.dll missing (run `just import-game`)"

_PYCLI = Path(__file__).resolve().parents[1]
_MONO = _PYCLI.parent

# Requirement name -> (path that must exist, how to obtain it).
_REQUIREMENTS: dict[str, tuple[Path, str]] = {
    "localization": (_PYCLI / "localization_eng" / "cards.json", "just import-game --tier oracle"),
    "wiki_data": (_MONO / "sts2-wiki" / "data" / "v0.107.1" / "cards.json", "just import-game"),
    "card_db": (_PYCLI / "python" / "workbench" / "data" / "card_db.json", "just import-game"),
    "checkpoints": (
        _PYCLI / "data" / "ppo" / "ppo_fullrun_it350.pt",
        "private training checkpoints (.pt)",
    ),
    "dockerfile": (_MONO / "Dockerfile", "private deploy artifact"),
    "wrangler": (_PYCLI / "web" / "wrangler.toml", "private deploy artifact"),
}

# Node-id prefix (relative to tests/) -> requirement. A bare module prefix
# covers every test in the module; ``module::test`` covers one test.
_NEEDS: dict[str, str] = {
    # --- the vendored English localization
    "test_localization_registry_coverage.py": "localization",
    "test_realgame_advisor_deck_mutation.py": "localization",
    "test_rust_game_backend.py::test_csharp_resolve_event_fields_resolves_boss_relic_and_regular_event": "localization",
    "test_hint_adapter.py::test_ironclad_vs_nibbit_mid_turn": "localization",
    "test_hint_adapter.py::test_mid_turn_with_potions": "localization",
    "test_hint_adapter.py::test_the_title_path_still_wins_when_the_id_is_not_a_registry_name": "localization",
    "test_hint_adapter.py::test_wax_and_melt_flags_survive_translation": "localization",
    "workbench/test_regret_review.py": "localization",
    "workbench/test_bridge_tree_log.py": "localization",
    "workbench/test_branching.py": "localization",
    "workbench/test_timeline_details.py": "localization",
    "workbench/test_localization.py": "localization",
    "workbench/test_chains_of_binding.py": "localization",
    "workbench/test_card_text_placeholders.py": "localization",
    "workbench/test_event_option_loc_keys.py": "localization",
    "workbench/test_monster_map_sync.py": "localization",
    "workbench/test_orb_map_sync.py": "localization",
    "workbench/test_server.py::test_winged_boots_loc_key_resolves_from_relics_json": "localization",
    # --- the sibling wiki's extracted data (sts2-lookup, boss move titles)
    "test_play.py::test_boss_known_moves_includes_localized_titles": "wiki_data",
    "test_play.py::test_show_boss_renders_name_and_moves": "wiki_data",
    "test_sts2_mcp.py::TestRecommendCombat::test_recommend_picks_a_card": "wiki_data",
    "test_mcp_recommend_combat_mcts.py::TestRecommendCombatMctsHappyPath": "wiki_data",
    "test_mcp_recommend_combat_mcts.py::TestRecommendCombatUntouched": "wiki_data",
    "test_advisor_mirror.py::test_a_potion_screen_is_searched_end_to_end": "wiki_data",
    # --- the extracted card database the deck analyzer reads
    "workbench/test_deck_analysis.py": "card_db",
    # --- torch checkpoints (only the champion .onnx files are published)
    "test_ppo_onnx_policy.py": "checkpoints",
    "test_ppo_features_from_checkpoint.py": "checkpoints",
    "test_champion_resolvers_agree.py": "checkpoints",
    "test_ppo_shared_builders.py::test_ppo_policy_evaluate_and_recommend": "checkpoints",
    "test_ppo_shared_builders.py::test_ppo_policy_evaluate_degrades_on_inference_error": "checkpoints",
    "test_ppo_shared_builders.py::test_session_run_policy_selector": "checkpoints",
    "test_ppo_workbench_integration.py::test_snapshot_greedy_full_run_is_legal": "checkpoints",
    # --- private deploy artifacts
    "test_deploy_art_assets.py": "dockerfile",
    "test_deployed_image_has_lazy_import_deps.py": "dockerfile",
    "workbench/test_dockerfile_ships_ppo_onnx.py": "dockerfile",
    "test_bug_reports_cli.py": "wrangler",
}


def _needs(nodeid: str) -> str | None:
    rel = nodeid.split("tests/", 1)[-1]
    parts = rel.split("::")
    for n in range(len(parts), 0, -1):
        hit = _NEEDS.get("::".join(parts[:n]))
        if hit:
            return hit
    return None


def pytest_collection_modifyitems(items):
    for item in items:
        if "game" in getattr(item, "fixturenames", ()):
            item.add_marker(pytest.mark.oracle)
        req = _needs(item.nodeid)
        if req:
            item.add_marker(pytest.mark.game_data)
            path, how = _REQUIREMENTS[req]
            if not path.exists():
                item.add_marker(pytest.mark.skip(reason=f"needs {path.relative_to(_MONO)} ({how})"))


@pytest.hookimpl(wrapper=True)
def pytest_runtest_setup(item):
    # Module/class-scoped fixtures that build their own ``Game()``.
    try:
        return (yield)
    except OracleUnavailable:
        pytest.skip(ORACLE_SKIP_REASON)


@pytest.hookimpl(wrapper=True)
def pytest_runtest_call(item):
    try:
        return (yield)
    except OracleUnavailable:
        pytest.skip(ORACLE_SKIP_REASON)


@pytest.fixture(autouse=True)
def _reset_engine_unlock_state():
    """Undo any epoch lock a test installed into the engine. GH #755.

    ``sts2_rs.set_locked_epochs`` mutates PROCESS-GLOBAL engine state: every
    card pool built afterwards in the process is pruned. The live drivers'
    ``main()`` installs the live census on purpose (``realgame_mcts`` /
    ``realgame_random`` → ``install_live_unlock_state``), so a test that runs one
    leaked the lock into whatever ran next in its xdist worker — e.g. the
    Scroll Boxes golden then rolled DEATHBRINGER instead of DEATH_MARCH. The
    adapter's once-per-process wire latch is reset with it.

    Reads ``sys.modules`` instead of importing, so tests that never load the
    engine pay nothing.
    """
    yield
    import sys

    rs = sys.modules.get("sts2_rs")
    if rs is not None and hasattr(rs, "locked_cards") and rs.locked_cards():
        rs.set_locked_epochs([])
    adapter = sys.modules.get("realgame_adapter")
    if adapter is not None:
        adapter._WIRE_UNLOCKS_INSTALLED = None


@pytest.fixture
def game():
    """Each test gets an independent game process."""
    if not oracle_available():
        pytest.skip(ORACLE_SKIP_REASON)
    g = Game()
    yield g
    g.close()
