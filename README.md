# LocalRecord

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows-0078D6)](https://github.com/AntoineArt/localrecord)
[![Release](https://img.shields.io/github/v/release/AntoineArt/localrecord)](https://github.com/AntoineArt/localrecord/releases/latest)

Lightweight Windows tray app that records **microphone + desktop audio** (OBS-style WASAPI capture), saves a compressed Opus file by default, and copies it to the clipboard.

**Download:** [Latest release](https://github.com/AntoineArt/localrecord/releases/latest) · **Website:** [localrecord.doublea.engineering](https://localrecord.doublea.engineering)

## Why LocalRecord?

Quick recordings without opening a full DAW or OBS. One hotkey toggles capture of both your mic and whatever is playing on your PC. When you stop, the recording is saved and ready to paste or open in Audacity and similar tools.

## Features

- **Dual capture** — WASAPI loopback (desktop/apps) + microphone, mixed to one file
- **Compact Opus output** — default `.opus` format (~30 MB/hour at 64 kbps vs ~660 MB/hour WAV)
- **Global hotkey** — default `Ctrl+Shift+R`, customizable from the tray menu
- **System tray** — start/stop, open or change recordings folder, change shortcut, startup toggle
- **Recording indicator** — tray icon shows a red badge while recording
- **Toast notification on save** — filename and folder shown clearly when a recording finishes
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

Change the folder from the tray menu (**Change recordings folder...**) or set `recordings_dir=` in settings.

Settings are stored in:

`%LOCALAPPDATA%\localrecord\LocalRecord\config\settings.ini`

Example:

```ini
hotkey=Ctrl+Shift+R
format=opus
bitrate=64
recordings_dir=D:\My Recordings
```

Use `format=wav` for uncompressed WAV (paste-as-audio in Audacity). Use `bitrate=32`–`128` for Opus quality (default `64`).

## Tray menu

| Item | Action |
|------|--------|
| Start recording | Begin capture |
| Stop recording | Stop, save file, copy to clipboard |
| Open recordings folder | Open output directory in Explorer |
| Change recordings folder... | Pick a custom save location |
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
3. **Mix** — both streams mixed to 48 kHz stereo in software
4. **Output** — Opus (`.opus`) by default, or 16-bit PCM WAV via settings; clipboard gets the file (WAV paste when using `format=wav`)

## Limitations

- DRM-protected content may not capture
- Apps in exclusive audio mode may be missing from loopback
- Large WAV recordings can be slow to copy to the clipboard (Opus copies the file path only; use `format=wav` if you need paste-as-audio)
- Not all apps accept audio from the clipboard

## Contributing

Issues and pull requests are welcome.

## License

[MIT](LICENSE) © Antoine Art
