# Quick Equalizer

A lightweight 10-band equalizer for Windows. I built it because I needed a simple system-wide equalizer and couldn't find one that suited me.

Quick Equalizer is a small desktop front end for [Equalizer APO](https://sourceforge.net/projects/equalizerapo/). You shape the curve in the app, and it writes the settings to Equalizer APO's config file, which applies them to all audio on the selected playback device.

Built with **Tauri 2 (Rust)** and **React**.

---

## Features

- **10-band EQ** at 32, 64, 125, 250 and 500 Hz and 1, 2, 4, 8 and 16 kHz, each from −12 to +12 dB. Drag a point on the curve, or pick a band with **←/→** and adjust it in 0.1 dB steps with **↑/↓** (**Esc** or **Enter** deselects).
- **Preamp** from −12 to +12 dB, to avoid clipping when you boost bands.
- **On/off switch** that bypasses the EQ without losing your curve.
- **Presets:** built-in presets for games (CS2, Valorant, R6S, PUBG, League, Fortnite, Esport) plus Music, Podcast and Flat. Save your own curves as custom presets; they are kept between sessions.
- **Device setup:** on first launch the app lists your active playback devices through the Windows Core Audio API and points Equalizer APO at the one you choose. Switch later with **Change device**.
- **Native look:** frameless window with a custom title bar and the Windows 11 Mica background.

## How it works

Equalizer APO does the actual audio processing inside the Windows audio pipeline. Quick Equalizer writes every change to `C:\Program Files\EqualizerAPO\config\config.txt` as one preamp line and ten peaking filters (Q 1.41), and Equalizer APO applies the new file straight away.

| Part | Where | What it does |
|------|-------|--------------|
| Front end | `src/App.jsx` | Draws the EQ curve on a canvas, handles mouse, touch and keyboard input, presets and the device setup dialog |
| Back end | `src-tauri/src/lib.rs` | Writes the Equalizer APO config, lists audio devices, configures the chosen device, and loads and saves presets |

Custom presets are stored in `%APPDATA%\QuickEqualizer\presets.json`.

## Requirements

- Windows 10 or 11 (the Mica background needs Windows 11)
- [Equalizer APO](https://sourceforge.net/projects/equalizerapo/) installed in the default location, `C:\Program Files\EqualizerAPO`. During its installation, select your playback device in the Device Selector and restart when asked.

The device setup step writes to Equalizer APO's config folder and to the Windows registry, so it may need to be run as administrator.

## Build from source

You need the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/) (Microsoft C++ Build Tools, WebView2 and Rust), plus Node.js and pnpm.

```bash
git clone https://github.com/deenyzx/quick-equalizer.git
cd quick-equalizer
pnpm install
pnpm tauri dev     # run the app in development mode
pnpm tauri build   # build an installer into src-tauri/target/release/bundle
```

## Tech stack

- **Rust** with **Tauri 2**: desktop shell and back end
- **windows** crate: Core Audio device list and registry access
- **React 19** and **Vite**: user interface
- **window-vibrancy**: Mica window background

## License

[MIT](LICENSE)
