"""RG-14: character="random" resolves like the lobby's RandomCharacter slot.

`StartRunLobby.BeginRunLocally` draws the three acts off the `act_selection`
stream, then one `NextItem(AllCharacters)` per random slot. The binding does the
same before constructing the run; explicit characters are unchanged. Expected
values come from the Rust unit test `resolve_random_character_pins`.
"""

import pytest

sts2_rs = pytest.importorskip("sts2_rs")


@pytest.mark.parametrize(
    ("seed", "expected"),
    [("PIN1", "necrobinder"), ("PIN2", "regent"), ("PIN3", "silent")],
)
def test_random_resolves_from_string_seed(seed, expected):
    rm = sts2_rs.RunManager(string_seed=seed, character="random")
    assert rm.character.lower() == expected


def test_random_is_case_insensitive_and_seed_deterministic():
    a = sts2_rs.RunManager(string_seed="PIN1", character="RANDOM")
    b = sts2_rs.RunManager(string_seed="PIN1", character="Random")
    assert a.character == b.character


@pytest.mark.parametrize(("seed", "expected"), [(1, "ironclad"), (2, "silent"), (3, "defect")])
def test_random_numeric_seed(seed, expected):
    rm = sts2_rs.RunManager(seed=seed, character="random")
    assert rm.character.lower() == expected


def test_explicit_character_unchanged():
    rm = sts2_rs.RunManager(string_seed="PIN1", character="Silent")
    assert rm.character.lower() == "silent"
