# Shipping Quick Equalizer on Steam

This document covers how the app is packaged for Steam. Steam does not run an
installer — it syncs a folder to `steamapps/common/Quick Equalizer/` and runs
the exe from there, **unelevated**. The build is therefore a self-contained
folder (a "depot"), not an MSI/NSIS installer.

## Build the depot

From a terminal where `pnpm` and `cargo` are on PATH:

```powershell
./scripts/build-steam.ps1
```

This runs `pnpm tauri build --no-bundle` (builds the exe, skips installers) and
assembles `steam-depot/`. Upload that folder's contents as your Steam depot.

## WebView2 runtime (important)

The UI renders through **WebView2**. It ships with Windows 11 and most updated
Windows 10 machines, but is **not guaranteed** on a fresh/older Win10 PC — if
it's missing, the app opens to a blank white window.

Because Steam only copies files (no online bootstrapper at install time), we
bundle the **fixed-version WebView2 runtime** in the depot:

1. Download the *Fixed Version* runtime (x64) from
   <https://developer.microsoft.com/microsoft-edge/webview2/>.
2. Extract it to `src-tauri/webview2-runtime/` (git-ignored).
3. Run `scripts/build-steam.ps1` — it copies the runtime into the depot as
   `webview2/`.

At startup the app sets `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` to the bundled
`webview2/` folder if present (see `run()` in `src-tauri/src/lib.rs`), so it uses
the bundled runtime and falls back to the system one otherwise. Adds ~150 MB to
the depot but guarantees the app runs everywhere.

### Optional: static C runtime

Tauri release builds dynamically link the MSVC runtime (`vcruntime140.dll`).
It's present on almost all machines, but to be fully self-contained you can link
it statically by building with:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
```

(or add it to `.cargo/config.toml`). Alternatively, enable the VC++ Redistributable
as a common redistributable in your Steam depot settings.

## First-run engine setup (elevated helper)

Making the EQ affect system audio requires registering a system audio effect,
which needs **administrator rights** (writes to `HKLM` and `Program Files`).
Steam launches the app unelevated, so setup happens on demand:

- The app calls the `run_engine_setup` command, which relaunches **itself**
  elevated via a single UAC prompt (branded as this app, not the underlying
  engine) using `Start-Process -Verb RunAs`.
- The elevated copy detects the `--engine-setup` argument, runs
  `run_engine_setup_elevated()`, and exits before any UI is shown.
- The main app stays unelevated. We deliberately do **not** mark the exe
  `requireAdministrator`, which would prompt on every launch and break the Steam
  overlay.

> **Status: scaffold only.** `run_engine_setup_elevated()` currently just proves
> the elevation path works (it confirms it is elevated and writes
> `%APPDATA%\QuickEqualizer\engine_setup.log`). It does **not** yet register the
> real audio engine — that is the next phase (architecture A: bundle & silently
> run the engine's own installer, or B: bundle the DLLs and write the registry
> ourselves). Until then, on a machine without the engine the app runs and looks
> correct but shows "Setup needed" and won't process audio.

## What Steam needs from you (outside this repo)

- A Steamworks app ID and depot configured to point at `steam-depot/`.
- A launch option pointing at `quick-equalizer.exe`.
- Confirmation you have the legal right to redistribute the bundled audio engine
  (its license is unresolved — see the project notes before shipping commercially).
