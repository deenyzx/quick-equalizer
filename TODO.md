# Quick Equalizer — Road to Ship

Working branch: `steam-qeq` (all work here). `origin/main` untouched until it works.

> **Immediate next action:** re-run setup — the last fix (registry path) is committed
> but not yet tested. `pnpm tauri dev` → Set up → pick a *connected* device → Apply →
> approve UAC → confirm every step shows **OK** → reboot → check audio is EQ'd.

---

## P0 — Make it actually work (core blocker)
- [ ] Re-run engine setup on latest code; confirm `write device FxProperties` = OK
- [ ] Reboot, play audio, move sliders — confirm EQ **actually affects audio**
- [ ] If audio plays but EQ does nothing → fix `AudioProcessingObjects` metadata and/or
      `ConfigPath` value name (both compile-verified only, not runtime-verified)
- [ ] Verify EQ on/off toggle and preamp actually change the sound

## P0 — Legal gate (blocks selling)
- [ ] Resolve EqualizerAPO license (GPL vs LGPL). Decides whether you can sell
      closed-source, and whether the repo must be public. **Do before monetizing.**

## P1 — Packaging & distribution
- [ ] Download WebView2 **Fixed Version** runtime → `src-tauri/webview2-runtime/`
- [ ] Run `scripts/build-steam.ps1` → produce `steam-depot/`
- [ ] Test the depot build on a **clean machine/VM** (no dev tools) — confirm it launches
      (WebView2) and completes engine setup
- [ ] Code-signing certificate (SmartScreen / Smart App Control warnings on end-user PCs)

## P1 — Robustness
- [x] "Restart required" prompt in the UI after successful setup
- [x] Graceful error UX when setup fails (surface the log to the user)
- [x] Uninstall / cleanup path — remove APO registration + installed files (issue #6)
      *(compile-verified; test after install is confirmed)*
- [ ] Test on a second device type (Bluetooth vs onboard/USB)

## P2 — Polish
- [ ] App icon / branding pass
- [x] Persist EQ (curve, preamp, on/off, preset) across app restarts
- [ ] Handle default-device change while running

## P2 — Launch / CV (after it works)
- [ ] Screenshots + short demo GIF (in README + portfolio)
- [ ] Decide repo visibility + business model (see license outcome)
- [ ] Steam store page (if selling)
- [ ] Publish `steam-qeq` → `main` once verified working
