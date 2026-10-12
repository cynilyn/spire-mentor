# deploy.ps1 -- Standalone deploy of SpireCoach into the game's mods folder,
# without a rebuild. `dotnet build` already runs this via the DeploySpireCoach
# MSBuild target (SpireCoach.csproj); use this script when you just want to
# re-copy the manifest/config template after editing them, or to deploy to a
# non-default game install.
#
# Usage:
#   ./deploy.ps1
#   ./deploy.ps1 -GameDir "D:/SteamLibrary/steamapps/common/Slay the Spire 2"

param(
    [string]$GameDir = "C:/Program Files (x86)/Steam/steamapps/common/Slay the Spire 2"
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$modsDir = Join-Path $GameDir "mods/SpireCoach"

if (-not (Test-Path $GameDir)) {
    Write-Error "Game directory not found: $GameDir. Pass -GameDir to override."
}

$dll = Join-Path $scriptDir "bin/Debug/net9.0/SpireCoach.dll"
if (-not (Test-Path $dll)) {
    # Fall back to Release if that's what was last built.
    $dll = Join-Path $scriptDir "bin/Release/net9.0/SpireCoach.dll"
}
if (-not (Test-Path $dll)) {
    Write-Error "SpireCoach.dll not found under bin/. Run 'dotnet build' first."
}

New-Item -ItemType Directory -Force -Path $modsDir | Out-Null
Copy-Item -Path $dll -Destination $modsDir -Force
Copy-Item -Path (Join-Path $scriptDir "SpireCoach.json") -Destination $modsDir -Force
Copy-Item -Path (Join-Path $scriptDir "SpireCoach.conf.template") -Destination $modsDir -Force

# Before 0.3.0 the mod was mods/SimBridge/. Carry a mod-dir conf over (it
# wins over the %LOCALAPPDATA% one), then remove the old folder so the game
# does not load both copies.
$legacyDir = Join-Path $GameDir "mods/SimBridge"
if (Test-Path $legacyDir) {
    $legacyConf = Join-Path $legacyDir "SimBridge.conf"
    $newConf = Join-Path $modsDir "SpireCoach.conf"
    if ((Test-Path $legacyConf) -and -not (Test-Path $newConf)) {
        Copy-Item -Path $legacyConf -Destination $newConf
        Write-Host "Carried over $legacyConf -> $newConf"
    }
    Remove-Item -Recurse -Force $legacyDir
    Write-Host "Removed the old SimBridge install at $legacyDir"
}

Write-Host "Deployed SpireCoach to $modsDir"
Get-ChildItem $modsDir
