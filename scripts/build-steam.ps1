<#
.SYNOPSIS
  Builds a Steam-ready depot folder for Quick Equalizer.

.DESCRIPTION
  Produces a self-contained folder (no installer) suitable for uploading as a
  Steam depot: the release exe plus an optional bundled fixed-version WebView2
  runtime so the app runs on machines without the system WebView2 (common on
  fresh Windows 10). Run from a terminal where pnpm and cargo are on PATH.

.PARAMETER WebView2Runtime
  Path to an extracted fixed-version WebView2 runtime folder. If present, it is
  copied into the depot as `webview2/` and the app uses it automatically.

.PARAMETER OutDir
  Where to assemble the depot. Defaults to <repo>/steam-depot.
#>
param(
  [string]$WebView2Runtime = (Join-Path $PSScriptRoot "..\src-tauri\webview2-runtime"),
  [string]$OutDir          = (Join-Path $PSScriptRoot "..\steam-depot")
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

foreach ($tool in "pnpm", "cargo") {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
    throw "$tool not found on PATH. Open a fresh terminal (or install it) and retry."
  }
}

Write-Host "==> Building app without installers (tauri build --no-bundle)..." -ForegroundColor Cyan
Push-Location $root
try {
  # pnpm echoes its command line to stderr; under ErrorActionPreference=Stop that
  # would abort the script, so relax it for the native call and check the exit code.
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  pnpm tauri build --no-bundle
  $code = $LASTEXITCODE
  $ErrorActionPreference = $prev
  if ($code -ne 0) { throw "tauri build failed (exit $code)" }
} finally {
  Pop-Location
}

$exe = Join-Path $root "src-tauri\target\release\quick-equalizer.exe"
if (-not (Test-Path $exe)) { throw "Build output not found: $exe" }

Write-Host "==> Assembling depot at $OutDir ..." -ForegroundColor Cyan
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Force $OutDir | Out-Null
Copy-Item $exe $OutDir

if (Test-Path $WebView2Runtime) {
  Copy-Item $WebView2Runtime (Join-Path $OutDir "webview2") -Recurse
  Write-Host "    + bundled fixed-version WebView2 runtime" -ForegroundColor Green
}
else {
  Write-Warning @"
WebView2 fixed runtime not found at:
    $WebView2Runtime
The app will fall back to the system WebView2, which may be missing on a fresh
Windows 10 machine (the app would open to a blank window there).
Download the 'Fixed Version' runtime (x64), extract it to the path above, and
re-run this script:
    https://developer.microsoft.com/microsoft-edge/webview2/
"@
}

$engineBundle = Join-Path $root "src-tauri\engine-bundle"
if (Test-Path $engineBundle) {
  Copy-Item $engineBundle (Join-Path $OutDir "engine") -Recurse
  Write-Host "    + bundled audio engine (engine/)" -ForegroundColor Green
}
else {
  Write-Warning @"
Audio engine DLL not found at:
    $engineBundle
The app will run, but first-run engine setup will fail (no DLL to register).
Drop the APO DLL (as EqualizerAPO.dll) into that folder — see STEAM.md.
"@
}

Write-Host "`nDone. Depot folder: $OutDir" -ForegroundColor Green
Write-Host "Point your Steam depot at this folder's contents." -ForegroundColor Green
