#!/bin/bash
# setup.sh — Copy game DLLs from Steam installation to lib/,
#            then build + install the sts2_rs PyO3 extension via maturin.
#
# Prerequisites:
#   - Slay the Spire 2 installed via Steam
#   - .NET 9+ SDK (ARM64 for Apple Silicon, x64 for Intel/Linux)
#   - Rust 1.96+ toolchain (https://rustup.rs; ../sts2-rs/rust-toolchain.toml pins the exact version)
#   - maturin (`cargo install maturin` or `pipx install maturin`)
#   - sts2-pycli's uv venv synced (`just install-dev` or `uv sync --dev`)
#     and ACTIVATED before running this script (we refuse to install
#     into a wrong/unset venv — see venv guard below).
#
# Usage:
#   ./setup.sh                    # Auto-detect Steam path
#   ./setup.sh /path/to/game      # Manual game directory
#
# Skip flags (env):
#   STS2_SKIP_RUST=1              # Skip the maturin step (DLL-only setup)
#   STS2_SKIP_DOTNET=1            # Skip the .NET copy + IL-patch + build steps

set -e

# Resolve repo root (directory of this script) so relative paths work
# regardless of $PWD.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ── .NET DLL extraction + headless build ──
if [ "${STS2_SKIP_DOTNET:-0}" = "1" ]; then
    echo "⏭  STS2_SKIP_DOTNET=1 — skipping .NET copy + build."
else

# ── Locate game directory ──

GAME_DIR="$1"

if [ -z "$GAME_DIR" ]; then
    # Auto-detect based on platform
    case "$(uname -s)" in
        Darwin)
            GAME_DIR="$HOME/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/Resources/data_sts2_macos_arm64"
            if [ ! -d "$GAME_DIR" ]; then
                # Try x86_64
                GAME_DIR="$HOME/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/Resources/data_sts2_macos_x86_64"
            fi
            ;;
        Linux)
            GAME_DIR="$HOME/.steam/steam/steamapps/common/Slay the Spire 2"
            if [ ! -d "$GAME_DIR" ]; then
                GAME_DIR="$HOME/.local/share/Steam/steamapps/common/Slay the Spire 2"
            fi
            ;;
        MINGW*|MSYS*|CYGWIN*)
            GAME_DIR="C:/Program Files (x86)/Steam/steamapps/common/Slay the Spire 2"
            ;;
    esac
fi

USE_VENDORED_DLLS=0
if [ ! -d "$GAME_DIR" ]; then
    # No Steam install — fall back to the vendored, pre-patched DLLs in lib/
    # (tracked via git-lfs). This is the "runs on any clone" path: the
    # committed sts2.dll is already IL-patched for headless, so we skip the
    # copy + patch steps below and go straight to the build.
    if [ -f "lib/sts2.dll" ]; then
        USE_VENDORED_DLLS=1
        echo "📦 No game dir found — using vendored lib/ DLLs (git-lfs, pre-patched)."
    else
        echo "❌ Game directory not found: $GAME_DIR"
        echo "   …and no vendored lib/sts2.dll to fall back on."
        echo ""
        echo "Usage: ./setup.sh /path/to/game/data     (or: just import-game — see docs/bring-your-own-game.md)"
        echo ""
        echo "On macOS, this is usually:"
        echo "  ~/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/Resources/data_sts2_macos_arm64"
        echo ""
        echo "If the DLLs are vendored via git-lfs, run \`git lfs pull\` first."
        exit 1
    fi
else
    echo "📁 Game directory: $GAME_DIR"
fi

# ── Copy DLLs (skipped when using vendored lib/) ──

if [ "$USE_VENDORED_DLLS" != "1" ]; then

mkdir -p lib

# Download Lib.Harmony (the 0Harmony.dll the harness references) through the
# .NET SDK's own NuGet client into lib/. Version pinned to the family the game
# ships (2.4.x); override with STS2_HARMONY_VERSION.
fetch_harmony() {
    local ver="${STS2_HARMONY_VERSION:-2.4.1}"
    local tmp
    tmp=$(mktemp -d)
    cat > "$tmp/Harmony.csproj" << PROJ
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>net9.0</TargetFramework></PropertyGroup>
  <ItemGroup><PackageReference Include="Lib.Harmony" Version="$ver" /></ItemGroup>
</Project>
PROJ
    local dotnet_bin="${DOTNET:-dotnet}"
    [ -x "$HOME/.dotnet-arm64/dotnet" ] && dotnet_bin="$HOME/.dotnet-arm64/dotnet"
    if "$dotnet_bin" restore "$tmp/Harmony.csproj" --packages "$tmp/pkgs" >/dev/null 2>&1; then
        local dll
        dll=$(find "$tmp/pkgs/lib.harmony" -path '*net9.0*' -name 0Harmony.dll -print -quit 2>/dev/null)
        [ -n "$dll" ] || dll=$(find "$tmp/pkgs/lib.harmony" -name 0Harmony.dll -print -quit 2>/dev/null)
        if [ -n "$dll" ]; then
            cp "$dll" lib/0Harmony.dll
            echo "  ✓ 0Harmony.dll (Lib.Harmony $ver from NuGet)"
        else
            echo "  ✗ Lib.Harmony $ver restored but no 0Harmony.dll inside"
        fi
    else
        echo "  ✗ could not restore Lib.Harmony $ver from NuGet (offline?)"
    fi
    rm -rf "$tmp"
}

DLLS=(
    "sts2.dll"
    "SmartFormat.dll"
    "SmartFormat.ZString.dll"
    "Sentry.dll"
    "Steamworks.NET.dll"
    "MonoMod.Backports.dll"
    "MonoMod.ILHelpers.dll"
    "0Harmony.dll"
    "System.IO.Hashing.dll"
)

echo ""
echo "📦 Copying DLLs to lib/..."
for dll in "${DLLS[@]}"; do
    src="$GAME_DIR/$dll"
    if [ -f "$src" ]; then
        cp "$src" "lib/$dll"
        echo "  ✓ $dll"
    else
        echo "  ✗ $dll not found at $src"
        # Try searching subdirectories, then the whole install
        # (STS2_INSTALL_ROOT — scripts/import_game.py sets it to the folder
        # holding the pck; the Windows build keeps some DLLs outside the data dir).
        found=$(find "$GAME_DIR" -name "$dll" -print -quit 2>/dev/null)
        if [ -z "$found" ] && [ -n "${STS2_INSTALL_ROOT:-}" ] && [ -d "$STS2_INSTALL_ROOT" ]; then
            found=$(find "$STS2_INSTALL_ROOT" -name "$dll" -print -quit 2>/dev/null)
        fi
        if [ -n "$found" ]; then
            cp "$found" "lib/$dll"
            echo "    → found at $found"
        elif [ "$dll" = "0Harmony.dll" ]; then
            # Harmony (MIT, pardeike/Harmony) is the one referenced assembly a
            # game install may not carry; fetch the NuGet package instead.
            echo "    → not in the install; fetching Lib.Harmony from NuGet"
            fetch_harmony
        else
            echo "    ⚠ Skipped (may cause build errors)"
        fi
    fi
done

# Back up original sts2.dll
if [ -f "lib/sts2.dll" ] && [ ! -f "lib/sts2.dll.original" ]; then
    cp "lib/sts2.dll" "lib/sts2.dll.original"
    echo "  ✓ Backed up sts2.dll.original"
fi

fi  # USE_VENDORED_DLLS (copy + backup)

# ── Detect .NET SDK ──

DOTNET=""
if [ -x "$HOME/.dotnet-arm64/dotnet" ]; then
    DOTNET="$HOME/.dotnet-arm64/dotnet"
elif command -v dotnet &>/dev/null; then
    DOTNET="dotnet"
fi

if [ -z "$DOTNET" ]; then
    echo ""
    echo "❌ .NET SDK not found."
    echo "   Install .NET 9+ from https://dotnet.microsoft.com/download"
    echo "   Or set DOTNET env var to your dotnet binary path."
    exit 1
fi

echo ""
echo "🔧 .NET SDK: $DOTNET ($($DOTNET --version))"

# ── IL Patch sts2.dll (skipped when using vendored, pre-patched lib/) ──

if [ "$USE_VENDORED_DLLS" = "1" ]; then
echo ""
echo "⏭  Vendored sts2.dll is already IL-patched — skipping patch step."
else

echo ""
echo "🔨 Applying IL patches to sts2.dll..."

# Create a temporary patching project
PATCH_DIR=$(mktemp -d)
cat > "$PATCH_DIR/Patcher.csproj" << 'PROJ'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <!-- Same family as src/Sts2Headless (net9.0); RollForward runs it on a newer SDK. -->
    <TargetFramework>net9.0</TargetFramework>
    <RollForward>LatestMajor</RollForward>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Mono.Cecil" Version="0.11.6" />
  </ItemGroup>
</Project>
PROJ

cat > "$PATCH_DIR/Program.cs" << 'CSHARP'
using System;
using System.IO;
using Mono.Cecil;
using Mono.Cecil.Cil;

var dllPath = args[0];
Console.WriteLine($"Patching {dllPath}...");

var resolver = new DefaultAssemblyResolver();
var libDir = Path.GetDirectoryName(dllPath)!;
resolver.AddSearchDirectory(libDir);
// Also search for GodotSharp.dll in the GodotStubs output (fallback)
var stubsDir = Path.Combine(Path.GetDirectoryName(libDir)!, "src", "GodotStubs", "bin", "Debug", "net9.0");
if (Directory.Exists(stubsDir)) resolver.AddSearchDirectory(stubsDir);
var module = ModuleDefinition.ReadModule(dllPath, new ReaderParameters {
    AssemblyResolver = resolver,
    ReadingMode = ReadingMode.Deferred  // Don't force-resolve all references upfront
});

int patches = 0;

// Patch 1: Task.Yield() — make YieldAwaitable.YieldAwaiter.IsCompleted return true
// This prevents async deadlocks in headless mode
foreach (var type in module.Types)
{
    foreach (var nested in type.NestedTypes)
    {
        foreach (var nested2 in nested.NestedTypes)
        {
            if (nested2.Name.Contains("YieldAwaiter") || nested2.Name == "<>c")
            {
                foreach (var method in nested2.Methods)
                {
                    if (method.Name == "get_IsCompleted" && method.Body != null)
                    {
                        var il = method.Body.GetILProcessor();
                        il.Body.Instructions.Clear();
                        il.Emit(OpCodes.Ldc_I4_1);
                        il.Emit(OpCodes.Ret);
                        patches++;
                        Console.WriteLine($"  Patched {type.Name}.{nested.Name}.{nested2.Name}.IsCompleted");
                    }
                }
            }
        }
    }
}

// Patch 2: WaitUntilQueueIsEmptyOrWaitingOnNonPlayerDrivenAction → return Task.CompletedTask
foreach (var type in module.Types)
{
    foreach (var method in type.Methods)
    {
        if (method.Name == "WaitUntilQueueIsEmptyOrWaitingOnNonPlayerDrivenAction" && method.Body != null)
        {
            var il = method.Body.GetILProcessor();
            il.Body.Instructions.Clear();
            // return Task.CompletedTask
            var taskType = module.ImportReference(typeof(System.Threading.Tasks.Task));
            var completedProp = module.ImportReference(
                typeof(System.Threading.Tasks.Task).GetProperty("CompletedTask")!.GetGetMethod()!);
            il.Emit(OpCodes.Call, completedProp);
            il.Emit(OpCodes.Ret);
            patches++;
            Console.WriteLine($"  Patched {type.Name}.{method.Name} → Task.CompletedTask");
        }
    }
}

Console.WriteLine($"Applied {patches} patches");
var outPath = dllPath + ".patched";
module.Write(outPath);
module.Dispose();
File.Delete(dllPath);
File.Move(outPath, dllPath);
Console.WriteLine("Done!");
CSHARP

REPO_DIR="$(pwd)"
cd "$PATCH_DIR"
$DOTNET run -- "$REPO_DIR/lib/sts2.dll" 2>&1
cd "$REPO_DIR"
rm -rf "$PATCH_DIR"

fi  # USE_VENDORED_DLLS (IL patch)

# ── Build ──

echo ""
echo "🏗️ Building..."
$DOTNET build src/Sts2Headless/Sts2Headless.csproj 2>&1 | tail -5

fi  # STS2_SKIP_DOTNET

# ──────────────────────────────────────────────────────────────────────
# Rust / sts2_rs PyO3 extension (MCTS hint backend)
# ──────────────────────────────────────────────────────────────────────
#
# Builds the sts2_rs extension out of ../sts2-rs/crates/sts2-py and
# installs it editable-in-release-mode into the active venv via
# `maturin develop --release`. This is what powers the `sts2_hint`
# package's calls into `sts2_rs.state_from_dict`, `mcts_recommend`,
# etc. See:
#   docs/plans/.../integrate-sts2-rs-mcts-hint-system…md  (cross-repo plan)
#   python/sts2_hint/                                      (consumer)
#
if [ "${STS2_SKIP_RUST:-0}" = "1" ]; then
    echo ""
    echo "⏭  STS2_SKIP_RUST=1 — skipping maturin build of sts2_rs."
else

echo ""
echo "🦀 Building sts2_rs PyO3 extension via maturin..."

# ── Venv guard ──
# Refuse to install into the wrong venv or with no venv active. We match
# on the venv path containing "sts2-pycli" (uv's default venv lives at
# $REPO/.venv, so the realpath will always contain the repo name).
if [ -z "${VIRTUAL_ENV:-}" ]; then
    echo "❌ No virtualenv active."
    echo ""
    echo "   sts2_rs must be installed into sts2-pycli's venv. Activate it first:"
    echo "     cd $SCRIPT_DIR && source .venv/bin/activate"
    echo "   (or run \`just install-dev\` first if .venv doesn't exist yet)."
    echo ""
    echo "   Override with STS2_SKIP_RUST=1 if you really want to skip this."
    exit 1
fi

# Resolve symlinks (e.g. macOS /private/var) for a stable comparison.
VENV_REAL="$(cd "$VIRTUAL_ENV" 2>/dev/null && pwd -P || echo "$VIRTUAL_ENV")"
if [[ "$VENV_REAL" != *sts2-pycli* ]]; then
    echo "❌ Active venv looks wrong: $VIRTUAL_ENV"
    echo ""
    echo "   Expected a venv whose path contains 'sts2-pycli' (typically"
    echo "   \$REPO/.venv). Activate the right one and rerun, or set"
    echo "   STS2_SKIP_RUST=1 to skip."
    exit 1
fi

echo "  ✓ venv: $VIRTUAL_ENV"

# ── Locate maturin ──
if ! command -v maturin &>/dev/null; then
    echo "❌ maturin not found on PATH."
    echo "   Install with one of:"
    echo "     cargo install maturin"
    echo "     pipx install maturin"
    echo "     pip install maturin   # (into the active venv)"
    exit 1
fi

# ── Pick an uninstall driver ──
# uv-managed venvs don't ship pip; fall back to `uv pip` when present.
# Order: `uv pip` (sts2-pycli is uv-based) > `pip` > `python -m pip`.
PIP_UNINSTALL=()
if command -v uv &>/dev/null; then
    PIP_UNINSTALL=(uv pip uninstall)
elif command -v pip &>/dev/null && python -c "import pip" &>/dev/null; then
    PIP_UNINSTALL=(pip uninstall -y)
elif python -m pip --version &>/dev/null; then
    PIP_UNINSTALL=(python -m pip uninstall -y)
fi

# ── Stale-wheel uninstall ──
# A previously-installed wheel named sts2_rs (or sts2-rs) — from pip,
# uv, or an earlier `maturin build`+`pip install` — will shadow the
# editable build produced by `maturin develop` and cause confusing
# import behavior. Uninstall both spellings unconditionally; ignore
# failures (nothing-to-uninstall is the common case).
if [ "${#PIP_UNINSTALL[@]}" -gt 0 ]; then
    echo "  🧹 Removing any stale sts2_rs install in the venv (via '${PIP_UNINSTALL[*]}')..."
    "${PIP_UNINSTALL[@]}" sts2_rs 2>/dev/null || true
    "${PIP_UNINSTALL[@]}" sts2-rs 2>/dev/null || true
else
    echo "  ⚠ No pip/uv pip available to run stale-wheel uninstall — skipping."
    echo "    (maturin will still install, but a pre-existing wheel could shadow it.)"
fi

# ── Resolve the sts2-rs PyO3 crate ──
# Relative to this script: ../sts2-rs/crates/sts2-py
STS2_PY_MANIFEST="$SCRIPT_DIR/../sts2-rs/crates/sts2-py/Cargo.toml"
if [ ! -f "$STS2_PY_MANIFEST" ]; then
    echo "❌ sts2-rs PyO3 manifest not found: $STS2_PY_MANIFEST"
    echo "   Expected the sts2-rs repo at ../sts2-rs (sibling of sts2-pycli)."
    echo "   Clone https://github.com/<...>/sts2-rs alongside this repo, or"
    echo "   set STS2_SKIP_RUST=1 to skip the hint backend."
    exit 1
fi

# ── Build + install ──
echo "  🔨 maturin develop --release -m $STS2_PY_MANIFEST"
maturin develop --release -m "$STS2_PY_MANIFEST"

# ── Smoke check ──
echo "  🔎 Verifying import + version..."
python -c "import sts2_rs; v = sts2_rs.version(); assert v, 'empty version'; print('     sts2_rs version:', v)"

fi  # STS2_SKIP_RUST

echo ""
echo "✅ Setup complete!"
echo ""
echo "To play:"
echo "  python3 python/play.py"
echo ""
echo "To run batch games:"
echo "  python3 python/play_full_run.py 10"
