# ai_partner_test.ps1 -- a co-op run with AI partners in ONE game instance.
#
# docs/plans/2026-11-05-ai-coop-partners.md: `--spire-mentor-ai-partners N`
# hosts a co-op lobby from the main menu exactly as the host menu's "With AI
# partners" button does (SimAiPartnerSession, in-memory transport, no port),
# and 1-3 AI partners join it from inside the same process. No second
# SlayTheSpire2.exe (compare tools/coop_test.ps1).
#
#   ... -Partners 2                 AI partners (1-3; default 1)
#   ... -Character silent,defect    each partner's character, in join order
#   ... -BotHost                    the host plays hands-free too (advisor
#                                   autoplay; embarks once the partners are in)
#   ... -Speed 1                    game speed (default 3x)
#
# Launch from the PowerShell tool as `& '<path>\ai_partner_test.ps1' -BotHost`.

param(
    [string]$GameDir = $(if ($env:STS2_GAME_DIR) { $env:STS2_GAME_DIR } else { "C:/Program Files (x86)/Steam/steamapps/common/Slay the Spire 2" }),
    [ValidateRange(1, 3)]
    [int]$Partners = 1,
    [string]$Character = "",
    [double]$Speed = 3.0,
    # The partners' pacing, overriding the settings: first-play, end-turn, now.
    [string]$Pace = "",
    [string]$LogDir = "",
    [switch]$BotHost
)

$ErrorActionPreference = "Stop"
$exe = Join-Path $GameDir "SlayTheSpire2.exe"
if (-not (Test-Path $exe)) {
    Write-Error "Game not found: $exe (pass -GameDir or set STS2_GAME_DIR)."
}
if (-not (Get-Process steam -ErrorAction SilentlyContinue)) {
    Write-Error "Steam is not running; start it first."
}
if (Get-Process SlayTheSpire2 -ErrorAction SilentlyContinue) {
    Write-Error "A game instance is already running; close it first."
}
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
if (-not $LogDir) {
    $LogDir = Join-Path $repo ("logs/{0}-ai-partner" -f (Get-Date -Format "yyyy-MM-dd-HHmmss"))
}
New-Item -ItemType Directory -Force $LogDir | Out-Null
$hostLog = Join-Path $LogDir "host.godot.log"

$speedArg = $Speed.ToString([System.Globalization.CultureInfo]::InvariantCulture)
$hostArgs = @(
    "--log-file", "`"$hostLog`"",
    "--spire-mentor-speed", $speedArg,
    "--spire-mentor-coop-players", "$($Partners + 1)",
    "--spire-mentor-ai-partners", "$Partners",
    "--spire-mentor-ai-partner-shot", "`"$(Join-Path $LogDir 'host_menu.png')`""
)
if ($Character) { $hostArgs += @("--spire-mentor-ai-partner-character", $Character) }
if ($Pace) { $hostArgs += @("--spire-mentor-ai-partner-pace", $Pace) }
if ($BotHost) { $hostArgs += "--spire-mentor-autoplay" }
$proc = Start-Process -FilePath $exe -WorkingDirectory $GameDir -ArgumentList $hostArgs -PassThru
Set-Content -Path (Join-Path $LogDir "launch.txt") -Encoding ascii -Value ("pid {0}`nargs {1}" -f $proc.Id, ($hostArgs -join " "))
Write-Host "Started the host (pid $($proc.Id)) with $Partners AI partner(s)."
if (-not $BotHost) { Write-Host "Pick a character and Embark once the partner(s) show as ready." }
Write-Host "Log: $hostLog"
Write-Host "Partner lines:  Select-String 'SimAiPartner' '$hostLog' | Select-Object -Last 40"
