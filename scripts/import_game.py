#!/usr/bin/env python3
"""Bring your own game: regenerate every game-derived input from a local install.

This repository ships no game content. Everything the C# oracle, the parity
harness and the workbench need from Slay the Spire 2 — the game assembly,
its localization, the decompiled source the extractors read, card/monster
data, art, sound — is produced HERE, from the copy of the game you own, into
paths that are git-ignored in the public repository.

Usage:
    python3 scripts/import_game.py [GAME_DIR] [--tier oracle|workbench|sound|all]
    python3 scripts/import_game.py --status
    just import-game [GAME_DIR] [--tier ...]

GAME_DIR is the Steam install (the folder holding SlayTheSpire2.pck), the
macOS .app, or the data_sts2_* folder holding sts2.dll; omitted, the usual
Steam locations and $STS2_GAME_DIR are tried.

Tiers (each includes the ones before it):
    engine     nothing to import — the Rust workspace builds and tests as-is
    oracle     sts2-pycli/lib/ (IL-patched game DLLs) + localization_eng/
               -> the C# oracle, the parity harness, the full pytest suite
    workbench  + decompiled source, wiki data JSON, art, the sfx map, card
               text / card db, the workbench's embedded data bundle
               -> the Python and browser workbenches render real game
               text and art; `just wasm-build` / `just workbench-static` next
    sound      + the sound effects (needs the FMOD Studio API, see below)
    all        = sound

Every step is idempotent (re-running skips what exists) and logged to
logs/import-game.log. Read docs/bring-your-own-game.md first.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARIES = ROOT / "sts2_binaries"
WIKI = ROOT / "sts2-wiki"
PYCLI = ROOT / "sts2-pycli"
RS = ROOT / "sts2-rs"
LOG = ROOT / "logs" / "import-game.log"

# The build the engine simulates. sts2-rs/crates/sts2-workbench/src/data.rs
# (ENGINE_WIKI_VERSION) and sts2-pycli/python/workbench/localization.py
# (_ENGINE_WIKI_VERSION) pin the same value; move all three together.
PINNED_VERSION = "v0.107.1"

TIERS = ["engine", "oracle", "workbench", "sound"]

# Files the C# harness links against (sts2-pycli/setup.sh copies + patches).
GAME_DLLS = [
    "sts2.dll", "SmartFormat.dll", "SmartFormat.ZString.dll", "Sentry.dll", "Steamworks.NET.dll",
    "MonoMod.Backports.dll", "MonoMod.ILHelpers.dll", "0Harmony.dll", "System.IO.Hashing.dll",
]


class Step:
    """One idempotent step: `done()` says whether its output exists."""

    def __init__(self, name: str, tier: str, done, run, needs: tuple[str, ...] = ()):
        self.name, self.tier, self.done, self.run, self.needs = name, tier, done, run, needs


# --------------------------------------------------------------------------
# Logging / shell
# --------------------------------------------------------------------------


def log(msg: str) -> None:
    line = f"[{dt.datetime.now():%Y-%m-%d %H:%M:%S}] {msg}"
    print(line, flush=True)
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a", encoding="utf-8") as fh:
        fh.write(line + "\n")


def sh(cmd: list[str], cwd: Path, env: dict[str, str] | None = None) -> None:
    log(f"$ (cd {cwd.relative_to(ROOT) if cwd != ROOT else '.'} && {' '.join(cmd)})")
    full_env = dict(os.environ)
    if env:
        full_env.update(env)
    with LOG.open("a", encoding="utf-8") as fh:
        proc = subprocess.run(cmd, cwd=cwd, env=full_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        fh.write(proc.stdout)
    if proc.returncode != 0:
        tail = "\n".join(proc.stdout.splitlines()[-25:])
        raise SystemExit(f"step failed ({proc.returncode}): {' '.join(cmd)}\n{tail}\n(full log: {LOG})")


def just(recipe: str, cwd: Path, *args: str, env: dict[str, str] | None = None) -> None:
    sh(["just", recipe, *args], cwd, env)


def which(tool: str, *alt: str) -> str | None:
    for candidate in (tool, *alt):
        found = shutil.which(candidate)
        if found:
            return found
        p = Path(candidate).expanduser()
        if p.is_file() and os.access(p, os.X_OK):
            return str(p)
    return None


# --------------------------------------------------------------------------
# Locate + stage the game build
# --------------------------------------------------------------------------


def steam_candidates() -> list[Path]:
    home = Path.home()
    common = "steamapps/common/Slay the Spire 2"
    if platform.system() == "Darwin":
        return [home / "Library/Application Support/Steam" / common / "SlayTheSpire2.app"]
    if platform.system() == "Windows":
        return [Path("C:/Program Files (x86)/Steam") / common, Path("C:/Program Files/Steam") / common]
    return [
        home / ".steam/steam" / common,
        home / ".local/share/Steam" / common,
        home / ".var/app/com.valvesoftware.Steam/.local/share/Steam" / common,  # Flatpak
    ]


def find_pck(root: Path) -> Path | None:
    for name in ("SlayTheSpire2.pck", "Slay the Spire 2.pck"):
        for base in (root, root / "Contents/Resources"):
            if (base / name).is_file():
                return base / name
    hits = sorted(root.rglob("*.pck")) if root.is_dir() else []
    return hits[0] if hits else None


def find_data_dir(root: Path) -> Path | None:
    if (root / "sts2.dll").is_file():
        return root
    for base in (root, root / "Contents/Resources"):
        for d in sorted(base.glob("data_sts2_*")):
            if (d / "sts2.dll").is_file():
                return d
    return None


def locate(arg: str | None) -> tuple[Path, Path, Path]:
    """-> (data_dir with sts2.dll, pck, release_info.json)."""
    tried: list[str] = []
    candidates: list[Path] = []
    if arg:
        candidates.append(Path(arg).expanduser())
    elif os.environ.get("STS2_GAME_DIR"):
        candidates.append(Path(os.environ["STS2_GAME_DIR"]).expanduser())
    else:
        candidates.extend(steam_candidates())
    for cand in candidates:
        tried.append(str(cand))
        roots = [cand, cand.parent, cand.parent.parent]  # a data dir sits one or two levels below the pck
        for root in roots:
            pck = find_pck(root)
            data = find_data_dir(root)
            if pck and data:
                info = next((p for p in (root / "release_info.json", root / "Contents/Resources/release_info.json") if p.is_file()), None)
                if info is None:
                    raise SystemExit(f"found the game at {root} but no release_info.json next to the pck")
                return data, pck, info
    raise SystemExit(
        "could not find a Slay the Spire 2 install. Tried:\n  " + "\n  ".join(tried)
        + "\nPass the install folder (the one holding SlayTheSpire2.pck) as the first argument "
        "or set STS2_GAME_DIR."
    )


def read_version(release_info: Path) -> str:
    return str(json.loads(release_info.read_text(encoding="utf-8"))["version"])


def link_or_copy(src: Path, dst: Path, copy: bool) -> None:
    if dst.is_symlink() or dst.exists():
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    if copy:
        if src.is_dir():
            shutil.copytree(src, dst)
        else:
            shutil.copy2(src, dst)
    else:
        try:
            os.symlink(src, dst, target_is_directory=src.is_dir())
        except OSError:
            # Windows without developer mode: fall back to a copy.
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                shutil.copy2(src, dst)


def stage(data_dir: Path, pck: Path, release_info: Path, version: str, copy: bool) -> Path:
    """Populate sts2_binaries/<version>/ with the fixed layout every recipe reads."""
    build = BINARIES / version
    link_or_copy(release_info, build / "release_info.json", copy=True)
    link_or_copy(pck, build / "SlayTheSpire2.pck", copy)
    link_or_copy(data_dir, build / "data", copy)
    log(f"staged {build.relative_to(ROOT)}/ (release_info.json, SlayTheSpire2.pck, data/ -> {data_dir})")
    return build


# --------------------------------------------------------------------------
# Steps
# --------------------------------------------------------------------------


def build_steps(build: Path, version: str) -> list[Step]:
    wiki_env = {"STS2_VERSION": version, "STS2_BINARIES": str(BINARIES)}
    wiki_data = WIKI / "data" / version
    loc_eng = PYCLI / "localization_eng"
    images = WIKI / "site/public/images"
    audio = WIKI / "site/public/audio"
    fmod = os.environ.get("FMOD_SDK_ROOT", "")

    def real_file(path: Path) -> bool:
        """Present and not an un-smudged LFS pointer (the private repo vendors lib/ that way)."""
        if not path.is_file():
            return False
        with path.open("rb") as fh:
            return not fh.read(24).startswith(b"version https://git-lfs")

    def has_lib() -> bool:
        # The DLLs alone are not the output: setup.sh also IL-patches sts2.dll
        # (leaving sts2.dll.original behind) and builds the harness. A run that
        # was interrupted after the copy must not count as done.
        harness = PYCLI / "src/Sts2Headless/bin/Debug/net9.0"
        return (
            all(real_file(PYCLI / "lib" / d) for d in GAME_DLLS)
            and (PYCLI / "lib/sts2.dll.original").is_file()
            and (harness / "Sts2Headless.dll").is_file()
        )

    def oracle_lib():
        # STS2_INSTALL_ROOT: where setup.sh looks for DLLs the data dir lacks
        # (the pck's folder, i.e. the real install root through the symlink).
        install_root = (build / "SlayTheSpire2.pck").resolve().parent
        sh(["bash", "setup.sh", str(build / "data")], PYCLI,
           env={"STS2_SKIP_RUST": "1", "STS2_INSTALL_ROOT": str(install_root)})

    def extract_pck():
        just("extract-pck", WIKI, env=wiki_env)

    def localization():
        sh([sys.executable, "scripts/vendor_localization.py", str(build / "SlayTheSpire2.pck"), "--version", version], PYCLI)

    def decompile():
        just("decompile", WIKI, env=wiki_env)

    def wiki_extract():
        just("extract", WIKI, env=wiki_env)
        just("extract-vfx-bindings", WIKI, env=wiki_env)
        just("extract-power-flash-sites", WIKI, env=wiki_env)

    def wiki_images():
        just("extract-images", WIKI, env=wiki_env)
        just("extract-vfx", WIKI, env=wiki_env)

    def sfx_map():
        just("build-sfx-map", WIKI, env=wiki_env)

    def card_text_and_db():
        py_env = {"PYTHONPATH": str(PYCLI / "python")}
        sh([sys.executable, "-m", "synergy.gen_card_text"], PYCLI, env=py_env)
        sh([sys.executable, "-m", "workbench.build_card_db"], PYCLI, env=py_env)

    def workbench_data():
        sh([sys.executable, "scripts/build_asset_manifest.py"], PYCLI)
        sh([sys.executable, "scripts/bundle_workbench_data.py"], PYCLI)

    def sfx():
        just("extract-sfx", WIKI, env={**wiki_env, "FMOD_SDK_ROOT": fmod})
        just("encode-sfx", WIKI, env=wiki_env)

    return [
        Step("game DLLs -> sts2-pycli/lib/ (IL-patched) + C# oracle build", "oracle", has_lib, oracle_lib, ("dotnet",)),
        Step("pck localization + atlases -> sts2-wiki/extracted/", "oracle",
             lambda: (WIKI / "extracted" / version / "localization").is_dir(), extract_pck, ("just", "uv")),
        Step("localization -> sts2-pycli/localization_eng/", "oracle",
             lambda: (loc_eng / "cards.json").is_file() and (loc_eng / "BUILD.txt").is_file(), localization, ("uv",)),
        Step("decompile sts2.dll -> sts2-wiki/decompiled/", "workbench",
             lambda: (WIKI / "decompiled" / version).is_dir(), decompile, ("just", "ilspycmd")),
        Step("wiki data JSON -> sts2-wiki/data/", "workbench",
             lambda: (wiki_data / "cards.json").is_file() and (wiki_data / "vfx_bindings.json").is_file()
             and (wiki_data / "power_flash_sites.json").is_file(), wiki_extract, ("just", "uv")),
        Step("art + VFX -> sts2-wiki/site/public/images/", "workbench",
             lambda: (images / "vfx" / "manifest.json").is_file() and any(images.glob("card_atlas/*")), wiki_images, ("just", "uv")),
        Step("sfx map -> sts2-pycli/web/public/sfx_map.json", "workbench",
             lambda: (PYCLI / "web/public/sfx_map.json").is_file(), sfx_map, ("just", "uv")),
        Step("card text + card db -> sts2-pycli/", "workbench",
             lambda: (PYCLI / "web/public/card_text.json").is_file() and (PYCLI / "python/workbench/data/card_db.json").is_file(),
             card_text_and_db),
        Step("workbench bundle -> sts2-rs/crates/sts2-workbench/data/game/", "workbench",
             lambda: (RS / "crates/sts2-workbench/data/game/wiki_cards.json").is_file(), workbench_data),
        Step("sound effects -> sts2-wiki/site/public/audio/ (needs FMOD_SDK_ROOT)", "sound",
             lambda: any(audio.rglob("*.webm")), sfx, ("just", "uv")),
    ]


def preflight(steps: list[Step]) -> None:
    missing: dict[str, str] = {}
    hints = {
        "dotnet": ".NET 9 SDK — https://dotnet.microsoft.com/download",
        "just": "https://just.systems (cargo install just)",
        "uv": "https://docs.astral.sh/uv/",
        "ilspycmd": "dotnet tool install -g ilspycmd (ILSpy's CLI; the decompile step)",
    }
    for step in steps:
        for tool in step.needs:
            if tool in missing:
                continue
            found = which(tool, "~/.dotnet/tools/ilspycmd" if tool == "ilspycmd" else tool, "~/.dotnet/dotnet" if tool == "dotnet" else tool)
            if not found:
                missing[tool] = hints.get(tool, "")
    if missing:
        lines = [f"  {tool}: {hint}" for tool, hint in missing.items()]
        raise SystemExit("missing tools for the requested tier:\n" + "\n".join(lines))


def status(build: Path | None, version: str) -> int:
    print(f"pinned build: {PINNED_VERSION}")
    if build and build.is_dir():
        print(f"staged build: {build.relative_to(ROOT)}  (version {version})")
    else:
        print("staged build: none (sts2_binaries/<version>/ absent) — run `just import-game <GAME_DIR>`")
    steps = build_steps(build or BINARIES / PINNED_VERSION, version)
    for tier in TIERS[1:]:
        print(f"\n[{tier}]")
        for step in steps:
            if step.tier == tier:
                print(f"  {'ok     ' if step.done() else 'MISSING'}  {step.name}")
    return 0


# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[1], formatter_class=argparse.RawDescriptionHelpFormatter, epilog=__doc__)
    parser.add_argument("game_dir", nargs="?", help="Steam install folder / .app / data_sts2_* folder")
    parser.add_argument("--tier", choices=TIERS + ["all"], default="workbench")
    parser.add_argument("--status", action="store_true", help="report which artifacts exist and exit")
    parser.add_argument("--copy", action="store_true", help="copy the pck and DLLs into sts2_binaries/ instead of symlinking")
    parser.add_argument("--allow-version-mismatch", action="store_true",
                        help=f"proceed even if the install is not {PINNED_VERSION} (the engine is pinned; expect drift)")
    args = parser.parse_args(argv)
    tier = "sound" if args.tier == "all" else args.tier

    if args.status:
        staged = sorted(BINARIES.glob("v*")) if BINARIES.is_dir() else []
        build = staged[-1] if staged else None
        version = build.name if build else PINNED_VERSION
        return status(build, version)

    if tier == "engine":
        print("tier engine: nothing to import — `cd sts2-rs && cargo nextest run --workspace` works as-is.")
        return 0

    data_dir, pck, release_info = locate(args.game_dir)
    version = read_version(release_info)
    log(f"game install: data={data_dir} pck={pck} version={version}")
    if version != PINNED_VERSION and not args.allow_version_mismatch:
        raise SystemExit(
            f"the install is {version} but the engine is pinned to {PINNED_VERSION}; the parity oracle and "
            "the extracted data would describe a different game. Pass --allow-version-mismatch to proceed anyway."
        )
    build = stage(data_dir, pck, release_info, version, args.copy)

    wanted = TIERS[: TIERS.index(tier) + 1]
    steps = [s for s in build_steps(build, version) if s.tier in wanted]
    if tier == "sound" and not os.environ.get("FMOD_SDK_ROOT"):
        log("FMOD_SDK_ROOT is unset — skipping the sound tier (the FMOD Studio API is a free, registration-gated "
            "download from fmod.com; unpack it and set FMOD_SDK_ROOT to its root). The workbench is silent without it.")
        steps = [s for s in steps if s.tier != "sound"]
    preflight([s for s in steps if not s.done()])

    for step in steps:
        if step.done():
            log(f"skip (present): {step.name}")
            continue
        log(f"run: {step.name}")
        step.run()
        if not step.done():
            raise SystemExit(f"step finished but its output is missing: {step.name} (see {LOG})")
    log("done")
    if tier in ("workbench", "sound"):
        print("\nnext: the browser workbench —  cd sts2-pycli && just wasm-build && just workbench-static\n"
              "      the Python workbench   —  cd sts2-pycli && just workbench\n"
              "      the full test suites   —  see docs/bring-your-own-game.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
