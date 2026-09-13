"""GH #545: a champion must featurize at ITS OWN schema, not the shell's.

`reward_upgrade` appends no slot, so serving a pre-feature champion under
`STS2_PPO_FEATURES=reward_upgrade` is silent: the tensor shape is identical
and the net is simply handed a column it never trained on.
"""

import tomllib
from pathlib import Path

PPO_DIR = Path(__file__).resolve().parent.parent / "data" / "ppo"


def test_a_checkpoint_without_the_feature_ignores_the_env_var(monkeypatch):
    from ppo.net_policy import NetPolicy

    monkeypatch.setenv("STS2_PPO_FEATURES", "reward_upgrade")
    champs = tomllib.loads((PPO_DIR / "champions.toml").read_text())
    # A champion whose manifest entry actually declares no features. Naming
    # "ironclad" meant "a net trained before #545", which stopped being true
    # on 2026-08-28 -- the assertion then read as a bug in NetPolicy when it
    # was only a stale choice of subject.
    default_schema = [e for _n, e in sorted(champs.items()) if not e.get("features")]
    assert default_schema, (
        "every committed champion now declares an encoder feature, so there "
        "is no default-schema checkpoint left to prove the env var is ignored "
        "on one. Pin this to an archived net rather than deleting the check."
    )
    pol = NetPolicy(str(PPO_DIR / default_schema[0]["ckpt"]))
    assert pol.features == frozenset()


def test_a_checkpoint_declaring_the_feature_uses_it(tmp_path, monkeypatch):
    import torch
    from ppo.net_policy import NetPolicy

    src = tomllib.loads((PPO_DIR / "champions.toml").read_text())["ironclad"]["ckpt"]
    ckpt = torch.load(PPO_DIR / src, map_location="cpu", weights_only=False)
    ckpt.setdefault("cfg", {})["features"] = "reward_upgrade"
    out = tmp_path / "declared.pt"
    torch.save(ckpt, out)

    monkeypatch.delenv("STS2_PPO_FEATURES", raising=False)
    pol = NetPolicy(str(out))
    assert pol.features == frozenset({"reward_upgrade"})


def test_by_onnx_filename_resolves_the_real_champion_path():
    import champions

    champ = champions.get("ironclad")
    assert champions.by_onnx_filename(str(champ.onnx_path)) is champ


def test_by_onnx_filename_ignores_a_matching_basename_outside_the_champions_dir(tmp_path):
    """GH #545: a scratch copy of a champion's onnx sitting somewhere that
    isn't `data/ppo` must NOT inherit that champion's declared `features` --
    a basename-only match would resurrect exactly the "served at a schema it
    never trained on" bug this field exists to prevent, just moved from the
    env-var layer to the path layer."""
    import champions

    champs = tomllib.loads((PPO_DIR / "champions.toml").read_text())
    onnx_name = champs["ironclad"]["onnx"]
    decoy = tmp_path / onnx_name
    decoy.write_bytes(b"not a real onnx graph")

    assert champions.by_onnx_filename(str(decoy)) is None
