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

- **Dual capture** — desktop audio (WASAPI loopback on Windows, PulseAudio/PipeWire on Linux) plus microphone, mixed to one file
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

1. Download [`localrecord-x.y.z.exe`](https://github.com/AntoineArt/localrecord/releases/latest) from the latest release (filename includes the version)
2. Place it anywhere you like (Downloads, `Program Files`, etc.)
3. Run it — it appears in the system tray

### Linux

1. Download [`localrecord-x.y.z-x86_64-linux`](https://github.com/AntoineArt/localrecord/releases/latest) from the latest release, or build from source (see below)
2. Make it executable: `chmod +x localrecord-*-x86_64-linux`
3. Run it — it appears in the system tray

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

1. Launch the downloaded `.exe`
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

## Shortcut on Wayland

The global shortcut is grabbed through X11. On a **native Wayland session**
(Hyprland, Sway, GNOME Wayland…) nothing reaches that grab, so the shortcut
never fires — and the Linux tray backend has no click action either, leaving
only the right-click menu.

LocalRecord listens for **SIGUSR1** on Linux and toggles recording when it
arrives, so your compositor can bind a key to it directly:

```bash
pkill -USR1 -x localrecord
```

Hyprland, in `~/.config/hypr/bindings.conf`:

```
bindd = CTRL SHIFT, R, Audio recording, exec, pkill -USR1 -x localrecord
```

Sway, in `~/.config/sway/config`:

```
bindsym Ctrl+Shift+r exec pkill -USR1 -x localrecord
```

This works on X11 too, so it is a reasonable binding to keep either way.

### Picking the shortcut from the tray, on Hyprland

Under Hyprland the tray's **Change shortcut** entry stays usable: LocalRecord
writes the binding above into a file of its own and reloads the compositor, so
the shortcut you pick takes effect immediately.

- Lua config (`hyprland.lua` present): `~/.config/hypr/localrecord.lua`, loaded
  by a `dofile` line appended once to `hyprland.lua`.
- Legacy config: `~/.config/hypr/localrecord.conf`, loaded by a `source` line
  appended once to `hyprland.conf`.

Nothing else in your config is touched. If you already bind
`pkill -USR1 -x localrecord` by hand, remove that line — the two bindings would
otherwise both fire, and on the same key they would cancel each other out.
LocalRecord names the offending files in the message it shows after a change.

To take the binding back into your own hands, delete the generated file and its
load line.

On every other Wayland session the entry is greyed out and reads *"Change
shortcut (unavailable on Wayland)"*, since anything set there would be
registered and never fire. Change the compositor binding above instead.

## Desktop integration (Linux)

The tray icon is all LocalRecord shows on Linux, and that backend has no click
action — right-clicking for a menu is the only thing it offers. So the app also
publishes what it is doing, and takes instructions on signals, for anything that
wants to do better: a bar widget, a status script, a keybinding.

**Reading.** `~/.local/share/localrecord/state.json`, rewritten on every change
and renamed into place, so a reader never catches a half-written one:

```json
{
  "version": 1,
  "pid": 4242,
  "exe": "/usr/local/bin/localrecord",
  "recording": true,
  "started_at": 1786969263,
  "last_file": "/home/you/.local/share/localrecord/recordings/recording_2026-08-17_14-21-03.opus",
  "last_saved_at": 1786969327,
  "agc": true,
  "hotkey": "Ctrl+Shift+R",
  "format": "opus",
  "bitrate": 64,
  "startup": false,
  "tray": true,
  "recordings_dir": "/home/you/.local/share/localrecord/recordings"
}
```

`pid` is there to be checked: nothing else distinguishes a crashed app from an
idle one.

**Writing.** Two channels, so nothing has to edit the settings file behind the
app's back and leave its tray menu stale.

Signals, for a compositor binding — they need no path and no shell:

```bash
pkill -USR1 -x localrecord   # start/stop recording
pkill -USR2 -x localrecord   # toggle auto-levelling
```

And `~/.local/share/localrecord/command` for everything else, one command per
line, appended. A signal carries no value; a format or a bitrate has one:

```bash
echo "format wav"  >> ~/.local/share/localrecord/command
echo "bitrate 96"  >> ~/.local/share/localrecord/command
echo "tray"        >> ~/.local/share/localrecord/command   # show/hide the icon
echo "startup"     >> ~/.local/share/localrecord/command
echo "shortcut"    >> ~/.local/share/localrecord/command   # opens the picker
echo "folder"      >> ~/.local/share/localrecord/command   # opens the picker
echo "record"      >> ~/.local/share/localrecord/command
echo "agc"         >> ~/.local/share/localrecord/command
echo "quit"        >> ~/.local/share/localrecord/command
```

Each one is applied through the same code path the tray menu uses, so the menu,
the state file and any widget reading it stay in agreement.

### Hiding the tray icon

Where something else drives the app — the Omarchy widget below, or just the
shortcut — the icon is no longer needed. `tray=off` in `settings.ini`, or the
switch in the widget's panel, takes it out of the tray without stopping the app.
It is on by default, and Linux-only: everywhere else the icon is the whole
interface.

### Omarchy bar widget

[localrecord-omarchy-plugin](https://github.com/AntoineArt/localrecord-omarchy-plugin)
puts all of that in the Omarchy bar — a microphone glyph, a red REC dot with a
running clock while recording, and a panel holding every setting, the last
recording, and the switch that hides the tray icon:

```bash
omarchy plugin add https://github.com/AntoineArt/localrecord-omarchy-plugin.git --enable
```

The plugin cannot install this app for you — `omarchy plugin add` runs no plugin
code by design — but its panel offers a one-click **Install LocalRecord** that
pulls the latest release binary into `~/.local/bin`, without sudo.

## Tray menu

| Item | Action |
|------|--------|
| Start recording | Begin capture |
| Stop recording | Stop, save file, copy to clipboard |
| Open recordings folder | Open output directory in Explorer |
| Change recordings folder... | Pick a custom save location |
| Change shortcut | Pick a new global hotkey — writes the compositor binding on Hyprland, disabled on other Wayland sessions, see above |
| Auto-level mic and desktop audio | Toggle AGC — applies to the next recording |
| Launch at startup | Toggle auto-start (Windows registry or XDG autostart on Linux) |
| Exit | Quit the app |

On Linux the icon can be hidden entirely — see [Hiding the tray icon](#hiding-the-tray-icon).

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

1. **Desktop audio** — WASAPI loopback on the default render device, captured in that device's mix format and converted to 48 kHz stereo (Windows often ignores format conversion on loopback)
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
- On Wayland the built-in global shortcut cannot fire (X11 grab) — bind `pkill -USR1 -x localrecord` instead, see [Shortcut on Wayland](#shortcut-on-wayland)
- Left-clicking the Linux tray icon does nothing; the backend exposes a menu only, so use right-click
- Auto-levelling raises a quiet source's noise floor along with its signal, and cannot recover a source that never reaches the −55 dBFS gate

## Contributing

Issues and pull requests are welcome.

## License

[MIT](LICENSE) © Antoine Art
