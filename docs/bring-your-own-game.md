# Bring your own game

This repository ships the game's **text and extracted data** (localization,
card/relic/monster JSON, the workbench bundle) but no game code, decompiled
code, art, audio or assemblies. The Rust engine, the AI stack and the
workbench build and run without the game — the workbench shows a neutral
placeholder tile wherever an image would go, and is silent. The rest is
extracted, on your machine, from the copy you own, into git-ignored paths.

You need the game for: the C# parity oracle (`oracle` tier), the decompiled
source the extractors and the mechanics work read plus real art
(`workbench` tier), sound (`sound` tier).

## What you get per tier

| Tier | Needs | Produces | Unlocks |
|---|---|---|---|
| `engine` | nothing | — | the engine, the AI, the workbench (placeholder art, no sound); every oracle-dependent test skips |
| `oracle` | the game, .NET 9 SDK, `uv`, `just` | `sts2-pycli/lib/` (IL-patched game DLLs); refreshes `localization_eng/` | the C# oracle (`just build-cs`), the parity harness, the full pytest suite |
| `workbench` | + `ilspycmd` | `sts2-wiki/decompiled/`, `sts2-wiki/site/public/images/`; refreshes `sts2-wiki/data/<ver>/`, `sfx_map.json`, card text/db, the workbench bundle | real art in the workbench; the decompiled source for mechanics work |
| `sound` | + the FMOD Studio API | `sts2-wiki/site/public/audio/` | sound effects in the workbench |

## Prerequisites

- **Slay the Spire 2** installed through Steam. The engine is pinned to build
  **v0.107.1**; `import-game` refuses a different build unless you pass
  `--allow-version-mismatch` (the oracle and the extracted data would then
  describe a different game).
- [`just`](https://just.systems), [`uv`](https://docs.astral.sh/uv/), Python 3.13.
- Rust 1.96 (pinned by `sts2-rs/rust-toolchain.toml`; rustup installs it on
  first use) and `cargo-nextest`. `sccache` is optional
  (`export RUSTC_WRAPPER=sccache`).
- **oracle tier:** the [.NET 9 SDK](https://dotnet.microsoft.com/download).
- **workbench tier:** ILSpy's CLI — `dotnet tool install -g ilspycmd`
  (lands in `~/.dotnet/tools`). Node 20+ for the web app, plus
  `rustup target add wasm32-unknown-unknown` and
  `cargo install wasm-bindgen-cli --version 0.2.120` (must match the
  `wasm-bindgen` crate in `sts2-rs/Cargo.lock`) for the browser build.
- **sound tier:** the FMOD Studio API (free, registration-gated download from
  fmod.com, Linux tarball `fmodstudioapi*linux.tar.gz`). Unpack it anywhere
  and export `FMOD_SDK_ROOT=<that dir>`. Also `ffmpeg` for the Opus encode.
- Windows: use WSL2 (the scripts are bash); point `import-game` at the
  Windows install through `/mnt/c/...`. Native Windows builds of the Rust
  engine are covered in `BUILDING-WINDOWS.md`.

## Run it

```sh
just import-game                       # auto-detects the Steam install; tier = workbench
just import-game /path/to/game --tier oracle
just import-game --tier all            # + sound, when FMOD_SDK_ROOT is set
just import-game --status              # what exists, what is missing
```

The first argument may be the install folder (the one holding
`SlayTheSpire2.pck`), the macOS `.app`, or the `data_sts2_*` folder holding
`sts2.dll`; `$STS2_GAME_DIR` works too. The driver stages the build under
`sts2_binaries/<version>/` (symlinks by default, `--copy` to copy — the pck
is ~1.9 GB) and then runs the per-subproject recipes in order:

1. `sts2-pycli/setup.sh` — copies the DLLs to `lib/`, IL-patches `sts2.dll`
   for headless use, builds the C# harness.
2. `sts2-wiki: just extract-pck` and `sts2-pycli/scripts/vendor_localization.py`
   — the English localization.
3. `sts2-wiki: just decompile` (ilspycmd), `just extract`,
   `extract-vfx-bindings`, `extract-power-flash-sites`, `extract-images`,
   `extract-vfx`, `build-sfx-map`.
4. `sts2-pycli`: `synergy.gen_card_text`, `workbench.build_card_db`,
   `scripts/build_asset_manifest.py`, `scripts/bundle_workbench_data.py`
   (= `just workbench-data`, the bundle the browser workbench embeds).
5. `sound`: `just extract-sfx` + `just encode-sfx`.

Every step is idempotent — a re-run skips outputs that exist — and
everything is logged to `logs/import-game.log`. To redo one step, delete its
output and re-run.

## Then

```sh
cd sts2-pycli && uv sync --dev
just build-cs                          # C# oracle (oracle tier)
uv run pytest tests                    # the whole suite, oracle tests included
just workbench-static-preview          # after `just workbench-static`: serve on 127.0.0.1:4173
just workbench                         # Python-backed workbench on 127.0.0.1:8077
cd ../sts2-rs && cargo nextest run --workspace   # game-gated tests now run too
```

The workbench golden tests (`sts2-rs/crates/sts2-workbench/tests/golden/`)
ship and run as-is; `just workbench-golden-dump` (sts2-pycli) re-records them
from your data when the display text or the engine changes.

## What is pinned where

The build the engine simulates is `v0.107.1`, recorded in
`sts2-rs/crates/sts2-workbench/src/data.rs` (`ENGINE_WIKI_VERSION`),
`sts2-pycli/python/workbench/localization.py` (`_ENGINE_WIKI_VERSION`) and
`scripts/import_game.py` (`PINNED_VERSION`). `localization_eng/BUILD.txt`
records which build your localization came from.
