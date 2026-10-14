# Stages the Black Ops files Pocket Lab needs for Kino der Toten into one folder you can
# copy to the iPhone (Files app or Apple Devices > Pocket Lab > Files), leaving out what it
# never reads: cutscene videos, the soundtrack and the other maps.
#
#   powershell -File scripts\stage-bo1-for-iphone.ps1 [-Bo1 <Black Ops folder>] [-Out <folder>] [-Copy]
#
# By default the staged files are hard links to your Steam install (no extra disk space;
# needs the same drive). -Copy makes real copies instead.
param(
    [string]$Bo1 = 'C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Black Ops',
    [string]$Out = (Join-Path $env:USERPROFILE 'BO1-iPhone'),
    [switch]$Copy
)
$ErrorActionPreference = 'Stop'

$zones = @(
    # the runtime common the map loads on (BO1 multiplayer's, until zombies' own are wired)
    'Common\common_mp.ff', 'Common\code_post_gfx_mp.ff', 'Common\patch_mp.ff',
    # zombies commons and singleplayer startup zones
    'Common\common_zombie.ff', 'Common\common_zombie_patch.ff',
    'Common\code_post_gfx.ff', 'Common\common.ff', 'Common\patch.ff',
    # Kino der Toten
    'Common\zombie_theater.ff', 'Common\zombie_theater_patch.ff'
)

$game = Join-Path $Out 'Call of Duty Black Ops'
if (-not (Test-Path (Join-Path $Bo1 'zone\Common\common_mp.ff'))) { throw "No Black Ops install at $Bo1" }
New-Item -ItemType Directory -Force -Path (Join-Path $game 'zone\Common'), (Join-Path $game 'zone\English'), (Join-Path $game 'main') | Out-Null

function Stage([string]$from, [string]$to) {
    if (Test-Path $to) { Remove-Item $to -Force }
    if ($Copy) { Copy-Item $from $to } else { New-Item -ItemType HardLink -Path $to -Target $from | Out-Null }
}

foreach ($z in $zones) { Stage (Join-Path $Bo1 "zone\$z") (Join-Path $game "zone\$z") }
# language zones are small (about 22 MB for all of them)
Get-ChildItem (Join-Path $Bo1 'zone\English') -Filter *.ff | ForEach-Object { Stage $_.FullName (Join-Path $game "zone\English\$($_.Name)") }
# textures and sounds
Get-ChildItem (Join-Path $Bo1 'main') -Filter *.iwd | ForEach-Object { Stage $_.FullName (Join-Path $game "main\$($_.Name)") }
foreach ($f in 'localization.txt') { if (Test-Path (Join-Path $Bo1 $f)) { Stage (Join-Path $Bo1 $f) (Join-Path $game $f) } }

$bytes = (Get-ChildItem $game -Recurse -File | Measure-Object Length -Sum).Sum
"Staged {0:N1} GB in {1}" -f ($bytes / 1GB), $game
"Copy the whole 'Call of Duty Black Ops' folder into Pocket Lab's 'Games' folder on the iPhone."
