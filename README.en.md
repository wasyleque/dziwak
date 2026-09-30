# Dziwak

[Polski](README.md) | **English**

Lightweight raster image editor written in Rust for Omarchy Linux (Wayland/Hyprland), with a GIMP 2.10-style interface.
License: GPL-3.0-or-later.

Approximately 100 MB RAM, 0% CPU when idle, no graphics card requirements.

## Features

- **Layers:** opacity, 6 blend modes, thumbnails
- **Selections:** rectangular, elliptical, freehand (lasso), fuzzy select, by color; combining (Shift = add, Ctrl = subtract, Shift+Ctrl = intersect)
- **Painting:** brush, pencil, airbrush, eraser, bucket fill, gradient, clone, smudge, dodge/burn, blur/sharpen
- **Transformations:** move, crop, rotate, scale, flip; image scaling and canvas size
- **Text:** system fonts (via fontconfig), text on a new layer
- **Filters with live preview:** brightness/contrast, hue/saturation, Gaussian blur, sharpen
- **Background removal:** uniform (from edges) and AI (U²-Net-p locally on CPU, ~1–2 s, no GPU)
- **Files:** PNG, JPEG, WebP and custom `.dziwak` format (layers, LZ4 compression)
- **Interface:** GIMP 2.10 Dark layout with Omarchy theme accent; Polish, English, Spanish (Edit → Language)
- Undo/redo with memory limit (copy-on-write tile snapshots)

## Building

Rust stable required (e.g., `mise use -g rust@stable`).

```bash
cargo run -p dziwak --release
```

AI-free version (smaller binary, faster build): `cargo build --release -p dziwak --no-default-features`.

## Installation

```bash
scripts/install-local.sh
```

Installs to `~/.local` (program, icon, launcher entry) and downloads the AI model (4.6 MB, Apache-2.0) to `~/.local/share/dziwak/models/`.

## Keyboard Shortcuts

Tools have shortcuts like in GIMP 2.10.

| Shortcut | Tool | Shortcut | Tool |
|---|---|---|---|
| R | Rectangle Select | P | Paintbrush |
| E | Ellipse Select | N | Pencil |
| F | Free Select | A | Airbrush |
| U | Fuzzy Select | Shift+E | Eraser |
| Shift+O | Select by Color | Shift+B | Bucket Fill |
| M | Move | G | Gradient |
| Shift+C | Crop | C | Clone (Ctrl+click = source) |
| Shift+R | Rotate | Shift+S | Smudge |
| Shift+T | Scale | Shift+D | Dodge/Burn |
| Shift+F | Flip | Shift+U | Blur/Sharpen |
| O | Color Picker | T | Text |
| Z | Zoom | Shift+M | Measure |

| Shortcut | Action |
|---|---|
| Ctrl+O / Ctrl+Shift+S | Open / Save As |
| Ctrl+Z / Ctrl+Shift+Z, Ctrl+Y | Undo / Redo |
| Ctrl+A / Ctrl+Shift+A / Ctrl+I | Select All / Deselect / Invert Selection |
| X / D | Swap Colors / Default Colors |
| Tab | Show/Hide docks |
| Shift+J / Shift+Ctrl+J | Center Image / Fit to Window |
| Mouse Wheel | Zoom around cursor |
| Middle Button, Space+LMB | Pan view |

## Architecture

- `crates/core`: UI-agnostic logic. Layers as 64×64 tiles (copy-on-write), premultiplied RGBA8 pixels, tile snapshot history, filters, selections, transformations, text, `.dziwak` format.
- `crates/app`: `egui`/`eframe` interface (backend `glow`); only changed tiles go to GPU.
- `crates/ai`: background removal with U²-Net-p model via `tract` (pure Rust, no ONNX Runtime).

## Development

Project rules and roadmap: `AGENTS.md`. Tests: `cargo test --workspace` (AI model test: `cargo test -p dziwak-ai -- --ignored`).
