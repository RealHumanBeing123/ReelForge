# ReelForge

A lightweight timeline video editor for Windows, written in Rust. Layout follows Camtasia: media bin on the left, preview in the centre, properties on the right, a full-width multi-track timeline along the bottom, and Export top-right.

## Build

1. Install Rust (MSVC toolchain): https://rustup.rs
2. `cargo build --release`
3. Download an FFmpeg **full** build for Windows (version **7.1 or newer**, e.g. gyan.dev "full" or BtbN GPL build) and copy `ffmpeg.exe` and `ffprobe.exe` next to `target\release\reelforge.exe` (or put them on `PATH`).

The finished program is `target\release\reelforge.exe`.

## How it works

- UI: `egui`/`eframe` (GPU-rendered, small binary, no web runtime).
- Media decoding, preview frames, recording and final encoding: the FFmpeg executables, so every codec FFmpeg supports is supported (H.264, HEVC, VP8/9, AV1, ProRes, MPEG-2/4, DNxHD, MP3, AAC, Opus, Vorbis, FLAC, WAV, PNG, JPEG, WebP, GIF, BMP, TIFF, ...).
- The Rust side owns the project model, timeline editing, undo/redo and building the FFmpeg filter graph. Preview playback, paused preview and export all use the same filter graph, so what you see matches what you render.

## Features

### Editing
- **Import:** file dialog, multi-select, or drag files onto the window. Video, audio and images.
- **Timeline:** multiple tracks (higher track = on top), drag clips from the bin, move, trim edges, split, delete, snapping, mute/hide per track, zoom (Ctrl+wheel), scroll (Shift+wheel).
- **Track collision rules:**
  - Clips on the **same track never overlap**. Dragging a clip onto another slides it to the nearest free spot, trimming stops at the neighbouring clip, and dropping from the media bin lands in the nearest gap.
  - Clips on **different tracks can overlap freely**. They are layered (upper track on top) and their audio is mixed.
- **Per-clip volume** (0-200%) and **audio fade in / fade out**.
- **Canvas presets** (16:9, 9:16, 1:1, 4:3) and frame rate.
- **Save / open projects** (`.rforge`, JSON).

### Titles
- `+ Title` adds a text clip on the top track. Edit text, font (Segoe UI, Arial, Georgia, Impact, Consolas), size, colour and position (center / lower third / top).
- **Drag the title on the preview** to place it anywhere.
- **Resizable text box:** set Box width / Box height (0 = fit to text), or drag the corner handle on the preview. Text wraps inside a fixed-width box.
- **Solid background:** pick any colour (colour picker or quick swatches) and an opacity from 0-100%. 100% gives a solid box.

### Transitions, effects and animation
- **Transitions** (per clip, in and out): fade, slide left/right/up/down, zoom. Shown as ramps on the clip.
- **Effects** (stackable per clip): colour adjustment, blur, sharpen, grayscale, sepia, invert, vignette, flip H/V, chroma key (green screen).
- **Keyframes:** opacity, scale, position X/Y. Move the playhead over a clip, click `+ Keyframe at playhead`, set the value. Keyframes show as diamonds on the clip and blend linearly.

### Preview
- Real-time playback with audio (Space).
- **Preview quality selector** next to the transport controls: Draft 360p, Standard 540p, **High 720p (default)**, Full HD 1080p. Higher settings use Lanczos scaling and accurate colour conversion for a sharper picture. If playback stutters on a slower PC, drop to a lower setting. Export is not affected by this setting.

### Recording (Record menu)
Both recorders offer the same encoding options as Export: container, resolution, frame rate, video codec (including NVENC / AMF / QSV hardware encoders), bitrate or constant quality, encoder speed, audio codec and audio bitrate.

- **Record screen:**
  - Capture the entire desktop, a region (X / Y / width / height), or a single window (by its exact title bar text).
  - Optional mouse cursor, microphone, and system audio (mixed together when both are chosen).
  - Resolution can be **Native** or 480p-2160p. Recordings are only ever scaled **down**, never up.
  - A 3-second countdown starts the recording. ReelForge can minimise itself while recording; restore it from the taskbar and press **Stop**.
  - MP4/MOV recordings are written as fragmented MP4, so a crash or power loss still leaves a playable file.
- **Record voice:** pick a microphone and an audio codec (AAC, MP3, Opus, Vorbis, FLAC, WAV) and bitrate.
- Files are auto-named in `Videos\ReelForge Recordings` unless you choose a location. Finished recordings are imported into the media bin and, optionally, added to the timeline at the playhead.

### Export
MP4 / MKV / MOV / WebM; H.264, H.265, VP9, AV1, ProRes, plus NVENC / AMF / QSV hardware encoders; AAC, MP3, Opus, Vorbis, FLAC, PCM. Resolutions 480p to 2160p (4K), frame rate, video bitrate (or constant quality), audio bitrate, encoder speed, size estimate, progress bar, cancel.

## Shortcuts

| Key | Action |
|---|---|
| Space | Play / pause |
| S | Split at playhead |
| Del | Delete selected clip |
| Left / Right | Step one frame |
| Home / End | Jump to start / end |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+I / Ctrl+O / Ctrl+S / Ctrl+E | Import / open / save / export |

## Known limitations (v0.3)

- **This code has not been compiled or run on Windows by its author.** The FFmpeg commands it generates (export, preview, titles, recordings) were tested against FFmpeg 6.1 on Linux with synthetic inputs; the egui interface and the Windows capture devices (`gdigrab`, DirectShow) have not. Expect to fix a few compile errors on the first build.
- Screen and voice recording use FFmpeg's `gdigrab` and DirectShow, so they work on Windows only. System-audio capture needs a loopback device such as "Stereo Mix", which many PCs have disabled by default.
- ReelForge's own window is captured if you do not minimise it while recording a desktop or region.
- Preview playback streams from FFmpeg with mixed audio through `rodio`. Audio and video start together but are not sample-locked, so expect small drift on long playbacks. Full HD preview can drop frames on slower machines.
- Editing, seeking or stepping while playing stops playback.
- Keyframe times are relative to the clip start. Trimming a clip's left edge does not shift them. Splitting a clip keeps animation continuous.
- Starting playback in the middle of a fade/zoom-in skips that transition's ramp for that session.
- Title fonts are loaded from `C:\Windows\Fonts`. Titles are written to temp files, so a user name containing an apostrophe in the temp path can break title rendering.
- The title box outline on the preview is an estimate of the text size; FFmpeg does the real layout. Wrapping inside a fixed-width box uses an average character width, so line breaks can differ slightly from a true font measurement.
- Opacity keyframes use FFmpeg's `geq` filter, which is slow at 4K. Prefer a fade transition when you only need a fade.
- No audio transitions or effects beyond fades; no on-canvas dragging of clips.
- Export builds one FFmpeg command; very large timelines (hundreds of clips) can hit the Windows command-line length limit.
- Bundling FFmpeg builds with x264/x265 makes your distribution subject to the GPL.
