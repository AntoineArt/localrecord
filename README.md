# LocalRecord

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-0078D6)](https://github.com/AntoineArt/localrecord)
[![Release](https://img.shields.io/github/v/release/AntoineArt/localrecord)](https://github.com/AntoineArt/localrecord/releases/latest)

Lightweight tray app that records **microphone + desktop audio**, saves a compressed Opus file by default, and copies it to the clipboard.

**Download:** [Latest release](https://github.com/AntoineArt/localrecord/releases/latest) · **Website:** [localrecord.doublea.engineering](https://localrecord.doublea.engineering)

Supported platforms: **Windows** (WASAPI) and **Linux** (PulseAudio/PipeWire).

## Why LocalRecord?

Quick recordings without opening a full DAW or OBS. One hotkey toggles capture of both your mic and whatever is playing on your PC. When you stop, the recording is saved and ready to paste or open in Audacity and similar tools.

## Features

- **Dual capture** — WASAPI loopback (desktop/apps) + microphone, mixed to one file
- **Auto-levelling (AGC)** — each source is levelled independently so a quiet mic is not buried under loud desktop audio
- **Compact Opus output** — default `.opus` format (~30 MB/hour at 64 kbps vs ~660 MB/hour WAV)
- **Global hotkey** — default `Ctrl+Shift+R`, customizable from the tray menu
- **System tray** — start/stop, open or change recordings folder, change shortcut, auto-levelling and startup toggles
- **Recording indicator** — tray icon shows a red badge while recording
- **Toast notification on save** — filename and folder shown clearly when a recording finishes
- **No console window** — runs quietly in the notification area
- **Low idle cost** — no audio threads until you record

## Requirements

### Windows

- Windows 10 or later
- Default playback and recording devices configured in Windows

### Linux

- PipeWire with PulseAudio compatibility (`pipewire-pulse`) or PulseAudio
- GTK 3 and libappindicator for the system tray
- A notification daemon (e.g. mako, dunst)
- `zenity` for the shortcut picker dialog
- `xdg-utils` for opening the recordings folder

Arch Linux example:

```bash
sudo pacman -S --needed libpulse opus gtk3 libappindicator-gtk3 pipewire-pulse zenity xdg-utils
```

## Install

### Windows

1. Download [`localrecord.exe`](https://github.com/AntoineArt/localrecord/releases/latest/download/localrecord.exe) from the latest release
2. Place it anywhere you like (Downloads, `Program Files`, etc.)
3. Run it — it appears in the system tray

### Linux

1. Download [`localrecord`](https://github.com/AntoineArt/localrecord/releases/latest/download/localrecord) from the latest release, or build from source (see below)
2. Make it executable: `chmod +x localrecord`
3. Run `./localrecord` — it appears in the system tray

Or build locally:

```bash
cargo build --release
./target/release/localrecord
```

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

- **Windows:** `%LOCALAPPDATA%\localrecord\LocalRecord\recordings\`
- **Linux:** `~/.local/share/localrecord/recordings/`

Change the folder from the tray menu (**Change recordings folder...**) or set `recordings_dir=` in settings.

Settings are stored in:

- **Windows:** `%LOCALAPPDATA%\localrecord\LocalRecord\config\settings.ini`
- **Linux:** `~/.config/localrecord/settings.ini`

Example:

```ini
hotkey=Ctrl+Shift+R
format=opus
bitrate=64
agc=on
recordings_dir=D:\My Recordings
```

Use `format=wav` for uncompressed WAV (paste-as-audio in Audacity). Use `bitrate=32`–`128` for Opus quality (default `64`). Use `agc=off` to record raw levels (see [Auto-levelling](#auto-levelling-agc)).

## Auto-levelling (AGC)

**On by default.** Set `agc=off` in settings, or untick **Auto-level mic and desktop audio** in the tray menu, to record raw levels instead.

### The problem it solves

LocalRecord writes what the operating system hands it, and the two sources are levelled by completely separate things:

- your **microphone** level comes from the device's analog gain and the Windows input slider
- your **desktop audio** level comes from the system output volume and the per-app volume sliders of whatever is playing

Nothing keeps those in step, so the two routinely arrive tens of dB apart — one source inaudible under the other, or the whole recording too quiet to use.

This is easy to miss because video conferencing apps hide it: Discord, Meet, Teams and anything else built on WebRTC run their own AGC with up to ~30 dB of gain. A microphone that sounds perfectly fine in a call can still be 40 dB below where a recording needs it. LocalRecord is often the first thing to show you the raw signal.

### What it does

One independent AGC per source, applied before the mix:

| | |
|---|---|
| Target | −20 dBFS RMS per source |
| Gain range | −20 dB to +30 dB (loud sources are pulled down as well as quiet ones pushed up) |
| Noise gate | −55 dBFS — below this the gain is held, not raised |
| Response | fast when turning down (~50 ms), slow when turning up (~2 s) |

Both channels of a source always receive the same gain, so the stereo image is preserved. The summed mix then passes through a soft saturator at −1 dBFS, so the extra level cannot clip the output.

The gate matters more than it looks: without it the AGC would wind up to maximum gain during every silence and amplify room noise. It also protects the desktop stream, which is *digital silence* when nothing is playing — an ungated AGC would sit at +30 dB and detonate on the first sound.

### What it costs

- **Noise comes up with the signal.** Boosting a quiet microphone by 30 dB boosts its hiss by 30 dB too. Auto-levelling makes a badly configured mic *audible*, not *clean* — it is a safety net, not a substitute for setting your input gain correctly.
- **The natural balance between sources is discarded.** That is the point, but it means turning your system volume down mid-recording no longer makes the desktop audio quieter in the file; the AGC just compensates.
- **Recordings are no longer bit-identical between takes** of the same material.

Turn it off if you are capturing material where the relative balance of the two sources is itself the content.

### Checking your levels

To see where a recording actually landed:

```bash
ffmpeg -i recording.opus -af volumedetect -f null -
```

A healthy recording peaks somewhere around −6 to −3 dBFS. A peak near −40 dBFS means a source is roughly 100× too quiet — worth fixing at the source (device gain, Windows input level, system volume) rather than leaving to the AGC.

## Tray menu

| Item | Action |
|------|--------|
| Start recording | Begin capture |
| Stop recording | Stop, save file, copy to clipboard |
| Open recordings folder | Open output directory in Explorer |
| Change recordings folder... | Pick a custom save location |
| Change shortcut | Pick a new global hotkey |
| Auto-level mic and desktop audio | Toggle AGC — applies to the next recording |
| Launch at startup | Toggle auto-start (Windows registry or XDG autostart on Linux) |
| Exit | Quit the app |

## Build from source

### On Windows (recommended)

```powershell
cargo build --release
```

Output: `target\release\localrecord.exe`

### On Linux

Install build dependencies first (Arch example):

```bash
sudo pacman -S --needed base-devel pkg-config libpulse opus gtk3 libappindicator-gtk3
```

Then:

```bash
cargo build --release
```

Output: `target/release/localrecord`

### Cross-compile Windows from Linux/WSL

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/localrecord.exe`

### Code signing (optional)

If you have a certificate, use `scripts/sign.ps1` after building. See the script for usage.

## How it works

### Windows

Same core approach as OBS on Windows:

1. **Desktop audio** — WASAPI loopback on the default render device
2. **Microphone** — WASAPI capture on the default input device
3. **Auto-level** — each stream levelled independently towards −20 dBFS RMS (unless `agc=off`)
4. **Mix** — both streams mixed to 48 kHz stereo in software, then soft-limited at −1 dBFS
5. **Output** — Opus (`.opus`) by default, or 16-bit PCM WAV via settings; clipboard gets the file (WAV paste when using `format=wav`)

### Linux

1. **Desktop audio** — PulseAudio/PipeWire monitor source (`@DEFAULT_MONITOR@`)
2. **Microphone** — default input source (`@DEFAULT_SOURCE@`)
3. **Auto-level** — each stream levelled independently towards −20 dBFS RMS (unless `agc=off`)
4. **Mix** — both streams mixed to 48 kHz stereo in software, then soft-limited at −1 dBFS
5. **Output** — same Opus/WAV pipeline as Windows; clipboard gets the file path

## Limitations

- DRM-protected content may not capture
- Apps in exclusive audio mode may be missing from loopback
- Large WAV recordings can be slow to copy to the clipboard (Opus copies the file path only; use `format=wav` if you need paste-as-audio)
- Not all apps accept audio from the clipboard
- Auto-levelling raises a quiet source's noise floor along with its signal, and cannot recover a source that never reaches the −55 dBFS gate

## Contributing

Issues and pull requests are welcome.

## License

[MIT](LICENSE) © Antoine Art
