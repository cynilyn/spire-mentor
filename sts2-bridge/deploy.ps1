# deploy.ps1 -- Standalone deploy of SimBridge into the game's mods folder,
# without a rebuild. `dotnet build` already runs this via the DeploySimBridge
# MSBuild target (SimBridge.csproj); use this script when you just want to
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
$modsDir = Join-Path $GameDir "mods/SimBridge"

if (-not (Test-Path $GameDir)) {
    Write-Error "Game directory not found: $GameDir. Pass -GameDir to override."
}

$dll = Join-Path $scriptDir "bin/Debug/net9.0/SimBridge.dll"
if (-not (Test-Path $dll)) {
    # Fall back to Release if that's what was last built.
    $dll = Join-Path $scriptDir "bin/Release/net9.0/SimBridge.dll"
}
if (-not (Test-Path $dll)) {
    Write-Error "SimBridge.dll not found under bin/. Run 'dotnet build' first."
}

New-Item -ItemType Directory -Force -Path $modsDir | Out-Null
Copy-Item -Path $dll -Destination $modsDir -Force
Copy-Item -Path (Join-Path $scriptDir "SimBridge.json") -Destination $modsDir -Force
Copy-Item -Path (Join-Path $scriptDir "SimBridge.conf.template") -Destination $modsDir -Force

Write-Host "Deployed SimBridge to $modsDir"
Get-ChildItem $modsDir
