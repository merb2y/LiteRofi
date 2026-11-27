# LiteRofi Theme Studio Pure Rust Architecture Decision Records (ADRs)

## ADR-001: Standalone Pure-Rust Desktop Application Shape
- **Status:** accepted (2026-11-27)
- **Context:** The existing `literofi-studio.exe` served an HTML/JS frontend (`studio/index.html`) over a local HTTP server (`crates/literofi-cli/src/studio.rs`, ported from `studio/server.py`) and launched an external Chromium `--app=` window. This incurred browser cold-start latency, IPC/HTTP serialization overhead, and extra memory consumption.
- **Decision:** Build `literofi-studio.exe` as a standalone, GPU-accelerated pure-Rust native desktop application in `crates/literofi-studio` using `eframe` / `egui` 0.29 (`glow` backend), removing `[[bin]] name = "literofi-studio"` from `crates/literofi-cli/Cargo.toml` so `literofi.exe` remains dependency-free from GUI crates.
- **Consequences:** Instant native startup, ~20–35 MB resident footprint, zero browser dependency, single native x86_64 Windows PE executable.

## ADR-002: Exact `eldertuner-rs` Mango-Minimal Design System
- **Status:** accepted (2026-11-27)
- **Context:** The user requested that Theme Studio share the exact visual identity and ergonomics of `eldertuner-rs`.
- **Decision:** Adopt `eldertuner-rs`'s `mango-minimal` sharp `0px` rounding tokens (`PAPER #080706`, `PAPER_2 #141210`, `BG_SEL #1f1b17`, `INK #f2e5bc`, `INK_2 #d8ccb8`, `MUTED #7c7265`, `OUTER_BORDER #a89984`, `RULE #2b2621`, `ACCENT #c8a97e`, `OK #b5bd68`, `WARN #cc6666`), frameless top navigation with drag/maximize/close controls and `2px ACCENT` underline, `1px OUTER_BORDER` frame, DWM sharp dark titlebar attributes, JetBrainsMono font priority, and reusable `ui_widgets` (`telemetry_ribbon`, `equal_two_columns`, `square_toggle`, `segmented_row`, `slider_row`, `draw_corner_ticks`).
- **Consequences:** Visual and ergonomic parity with `eldertuner-rs`, crisp hardware-rendered UI, 0% idle GPU/CPU drain via adaptive repaint.

## ADR-003: Non-Blocking Async Backend & GPU Thumbnail Pipeline
- **Status:** accepted (2026-11-27)
- **Context:** Scanning 200+ `.rasi` themes, querying Windows installed fonts via `reg.exe`, and decoding/rendering `.png` previews synchronously on Frame 1 would stall the UI thread.
- **Decision:** Dispatch theme discovery, palette discovery, and font enumeration on background threads delivered via `mpsc::Receiver`, and run thumbnail loading through `preview_loader.rs` (prioritizing currently visible cards, downscaling cached PNGs to `<= 480px` width on 2 worker threads, rendering missing PNGs via `literofi.exe --dump-png` on 4 worker threads, and capping GPU texture uploads to 8 per frame).
- **Consequences:** Frame 1 opens immediately cleanly; scrolling stays fluid at 60+ FPS even with 200+ themes.

## ADR-004: 100% Configuration & Cache Compatibility
- **Status:** accepted (2026-11-27)
- **Context:** Users already have active themes, palettes, favorites, widget configs, and rendered PNG caches on disk.
- **Decision:** Read and write the exact existing paths: `%APPDATA%\LiteRofi\config.rasi`, `%APPDATA%\LiteRofi\widgets.toml`, `%APPDATA%\LiteRofi\studio-favorites.json`, `%APPDATA%\LiteRofi\wallust\colors-rofi.rasi`, `%APPDATA%\LiteRofi\launchers\*\shared\colors.rasi`, and `%LOCALAPPDATA%\LiteRofi\studio-cache\<name>.png`.
- **Consequences:** Seamless drop-in upgrade with zero data migration or settings loss.

## ADR-005: Reference Preservation Policy
- **Status:** accepted (2026-11-27)
- **Context:** Per `rust-anything` `whole-app.md` guidelines, original reference sources are preserved during migration.
- **Decision:** Keep `studio/server.py`, `studio/index.html`, and `crates/literofi-cli/src/studio.rs` untouched on disk as reference artifacts.
- **Consequences:** Full traceability against original HTTP/UI behavior; zero accidental deletion.
