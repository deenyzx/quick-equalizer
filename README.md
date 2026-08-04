# Quick Equalizer

A lightweight, **system-wide audio equalizer for Windows** — apply a real-time
10-band EQ to everything you hear, from a clean, unobtrusive interface.

<p>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows-0078D6">
  <img alt="Built with Tauri" src="https://img.shields.io/badge/Tauri-2-24C8DB">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-000000?logo=rust">
  <img alt="React" src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black">
</p>

## Why

I built Quick Equalizer to fill a gap: no existing system EQ was simple and
practical enough for what my friends and I actually wanted. So I designed and
built my own, with a focus on being fast, lightweight, and easy to use.

## Features

- Real-time, **system-wide** 10-band equalizer (32 Hz – 16 kHz)
- Draggable, canvas-rendered EQ curve with keyboard fine-control
- Built-in presets (gaming, music, podcast, …) plus savable custom presets
- Per-device setup with automatic detection via the Windows Core Audio API
- Preamp control and one-click EQ on/off
- Frameless, translucent (acrylic) UI that stays out of the way

## Tech stack

| Layer | Technology |
|---|---|
| Frontend | React 19, Vite, HTML canvas |
| App shell / backend | Tauri 2 (Rust) |
| System integration | Windows Core Audio (MMDevice), COM, Win32 registry & security APIs |
| DSP engine | EqualizerAPO (see [Credits](#credits)) |

## How it works

Quick Equalizer integrates at the Windows audio-pipeline level. On first run it
performs a one-time, elevated setup that registers a system **Audio Processing
Object (APO)** on the selected output device:

- writes the COM class registration for the APO,
- takes ownership of the protected (TrustedInstaller-owned) `MMDevices` registry
  keys and installs the effect on the audio endpoint,
- enables loading of the unsigned APO and points it at a user-writable config.

From then on, the UI writes filter settings to the engine's config in real time
as you adjust the curve.

## Build

Requires **Node.js**, **pnpm**, and the **Rust toolchain (MSVC)**.

```bash
pnpm install
pnpm tauri dev      # run in development
pnpm tauri build    # production build
```

See [STEAM.md](STEAM.md) for packaging and distribution details.

## Credits

Digital signal processing is handled by
[EqualizerAPO](https://sourceforge.net/projects/equalizerapo/), an open-source
system-wide equalizer for Windows. Quick Equalizer builds the user experience,
device management, preset system, and installation/registration on top of it.

## Status

In active development.
