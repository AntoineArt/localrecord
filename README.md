# LocalRecord

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows-0078D6)](https://github.com/AntoineArt/localrecord)
[![Release](https://img.shields.io/github/v/release/AntoineArt/localrecord)](https://github.com/AntoineArt/localrecord/releases/latest)

Lightweight Windows tray app that records **microphone + desktop audio** (OBS-style WASAPI capture), saves a WAV file, and copies it to the clipboard.

**Download:** [Latest release](https://github.com/AntoineArt/localrecord/releases/latest) · **Website:** [localrecord.doublea.engineering](https://localrecord.doublea.engineering)

## Why LocalRecord?

Quick recordings without opening a full DAW or OBS. One hotkey toggles capture of both your mic and whatever is playing on your PC. When you stop, the WAV is saved and ready to paste into Audacity or similar tools.

## Features

- **Dual capture** — WASAPI loopback (desktop/apps) + microphone, mixed to one file
- **Global hotkey** — default `Ctrl+Shift+R`, customizable from the tray menu
- **System tray** — start/stop, open recordings folder, change shortcut, startup toggle
- **Recording indicator** — tray icon shows a red badge while recording
- **Clipboard export** — audio copied as `WAVE` on stop
- **No console window** — runs quietly in the notification area
- **Low idle cost** — no audio threads until you record

## Requirements

- Windows 10 or later
- Default playback and recording devices configured in Windows

## Install

1. Download [`localrecord.exe`](https://github.com/AntoineArt/localrecord/releases/latest/download/localrecord.exe) from the latest release
2. Place it anywhere you like (Downloads, `Program Files`, etc.)
3. Run it — it appears in the system tray

### SmartScreen warning (unsigned app)

LocalRecord is **open source** but **not code-signed**. Code signing costs hundreds of dollars per year, which this small free project cannot afford yet.

Windows SmartScreen may show **"Windows protected your PC"** on first launch. This is normal for unsigned software:

1. Click **More info**
2. Click **Run anyway**

The app uses microphone capture, global hotkeys, clipboard access, and an optional startup entry — behaviors that security software monitors. You can review the full source in this repository.

## Usage

1. Launch `localrecord.exe`
2. Press your shortcut (default **Ctrl+Shift+R**) or use the tray menu to start
3. Press the shortcut again or choose **Stop recording**
4. Paste into your editor, or find the file in the recordings folder

Recordings are saved to:

`%LOCALAPPDATA%\localrecord\LocalRecord\recordings\`

Settings (hotkey) are stored in:

`%LOCALAPPDATA%\localrecord\LocalRecord\config\settings.ini`

## Tray menu

| Item | Action |
|------|--------|
| Start recording | Begin capture |
| Stop recording | Stop, save WAV, copy to clipboard |
| Open recordings folder | Open output directory in Explorer |
| Change shortcut | Pick a new global hotkey |
| Launch at Windows startup | Toggle auto-start |
| Exit | Quit the app |

## Build from source

### On Windows (recommended)

```powershell
cargo build --release
```

Output: `target\release\localrecord.exe`

### Cross-compile from Linux/WSL

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/localrecord.exe`

### Code signing (optional)

If you have a certificate, use `scripts/sign.ps1` after building. See the script for usage.

## How it works

Same core approach as OBS on Windows:

1. **Desktop audio** — WASAPI loopback on the default render device
2. **Microphone** — WASAPI capture on the default input device
3. **Mix** — both streams resampled to 48 kHz stereo and summed in software
4. **Output** — 16-bit PCM WAV + clipboard (`CF_WAVE`)

## Limitations

- DRM-protected content may not capture
- Apps in exclusive audio mode may be missing from loopback
- Large recordings can be slow to copy to the clipboard (file is always saved on disk)
- Not all apps accept audio from the clipboard

## Contributing

Issues and pull requests are welcome.

## License

[MIT](LICENSE) © Antoine Art
