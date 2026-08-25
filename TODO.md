# Quick Equalizer — Road to Ship

Working branch: `steam-qeq` (all work here). `origin/main` untouched until it works.

> **Immediate next action — the one thing gating everything:**
> `git pull && pnpm tauri dev` → **Set up** → pick a *connected* device → Apply →
> approve UAC → every step should show **OK** → **reboot** → play audio, move sliders.
> If it doesn't work: device dialog → **Show engine diagnostics** → paste the output.

---

## P0 — Confirm it works (only you can do this)
- [ ] Run setup on latest code; confirm every step **OK**
- [ ] Reboot, play audio, move sliders — confirm the EQ **actually changes the sound**
- [ ] Confirm the EQ on/off toggle and preamp change the sound
- [ ] If no effect → **Show engine diagnostics** and share it (tells us whether the
      APO is loaded at all vs. a config-level problem)

## P0 — Legal gate (blocks selling)
- [ ] Resolve EqualizerAPO's license (GPL vs LGPL — sources contradict each other).
      Decides whether you can sell closed-source and whether the repo must be public.

## P0 — Flip before release
- [ ] `EnableTrace` is set to `"true"` in `set_config_path` (engine.rs) for
      diagnostics → set to `"false"` so the engine stops logging every audio session

## P1 — Packaging & distribution
- [ ] Download WebView2 **Fixed Version** runtime → `src-tauri/webview2-runtime/`
      (without it the app can open to a blank window on a bare Win10 machine)
- [ ] Run `scripts/build-steam.ps1` → produce `steam-depot/`
- [ ] Test the depot on a **clean machine/VM** (no dev tools): launches + setup completes
- [ ] Code-signing certificate (SmartScreen / Smart App Control warn on unsigned apps)
- [ ] Steam app id + depot + launch option

## P1 — Robustness (test once setup is confirmed)
- [ ] Verify uninstall restores the device (incl. a device that had a vendor APO)
- [ ] Test on a second device type (Bluetooth vs onboard/USB)
- [ ] Test as a **standard (non-admin) user** — the elevation path differs there
- [x] "Restart required" prompt after setup
- [x] Surface setup failures in the UI
- [x] Uninstall / cleanup path (issue #6)
- [x] Preserve + restore the device's original vendor APO
- [x] Machine-wide state so setup works when UAC elevates as another account
- [x] Merge ACLs instead of replacing (was locking the audio service out)

## P2 — Polish
- [ ] App icon / branding pass
- [ ] Handle default-device change while running
- [x] Clipping protection (auto headroom for boosted bands)
- [x] Persist EQ across restarts
- [x] Debounce config writes
- [x] Clean the old device's effect when switching devices
- [x] Preselect the in-use device in the device dialog

## P2 — Launch / CV (after it works)
- [ ] Screenshots + short demo GIF (README + portfolio)
- [ ] Decide repo visibility + business model (follows the license outcome)
- [ ] Steam store page
- [ ] Publish `steam-qeq` → `main` once verified working
