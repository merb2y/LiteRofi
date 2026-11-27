# LiteRofi Theme Studio Whole-App Rust Port Map (`rust/MAP.md`)

- **Goal:** Standalone GPU-accelerated pure-Rust desktop application (`crates/literofi-studio` -> `literofi-studio.exe`) replacing the Chromium `--app=` + localhost HTTP Theme Studio while matching the exact `eldertuner-rs` Mango-Minimal design system.
- **Decisions:** [DECISIONS.md](file:///C:/Users/merby/OC/LiteRofi/rust/DECISIONS.md) (`ADR-001`–`ADR-005`)
- **Glossary:** [GLOSSARY.md](file:///C:/Users/merby/OC/LiteRofi/rust/GLOSSARY.md)

## Port Waves & Unit Status

| Wave | Module (`crates/literofi-studio/src/`) | Original Source(s) | Status | Verification |
|---|---|---|---|---|
| **Wave 1** | `theme.rs`, `ui_widgets.rs`, `win32_chrome.rs`, `i18n.rs` | `eldertuner-rs` design system + `studio/index.html` i18n | `done` | Clippy `-D warnings` + headless width tests (`1220x780`, `920x580`) |
| **Wave 2** | `backend.rs`, `preview_loader.rs` | `studio/server.py`, `crates/literofi-cli/src/studio.rs` | `done` | Unit tests (import, filter, palette round-trip, validation) |
| **Wave 3** | `view_themes.rs`, `view_palettes.rs`, `view_widgets.rs`, `view_editor.rs`, `main.rs` | `studio/index.html` + `eldertuner-rs` shell + new `.rasi` editor | `done` | Headless `all_four_tabs_render_without_panic_and_fit_width`, release build & live smoke check |
