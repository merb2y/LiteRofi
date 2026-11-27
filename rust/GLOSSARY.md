# LiteRofi Theme Studio Domain Glossary (`rust/GLOSSARY.md`)

| Domain Term | Meaning | Rust Representation (`crates/literofi-studio`) |
|---|---|---|
| **Theme Entry** | Valid `.rasi` theme file containing widget rules | `backend::ThemeEntry` |
| **Color Palette** | 7-token color scheme (`bg`, `bg_alt`, `fg`, `selected`, `active`, `urgent`, `border`) | `backend::Palette` |
| **Widgets Config** | Persistent plugin & filter config in `%APPDATA%\LiteRofi\widgets.toml` | `literofi_core::widgets_config::WidgetsConfig` |
| **Preview Loader** | Non-blocking disk PNG + `literofi.exe --dump-png` GPU texture cache | `preview_loader::PreviewLoader` |
| **Prerender State** | Multi-threaded batch preview render progress counters | `preview_loader::PrerenderProgress` |
| **Studio Tab** | Top-level view (`Themes`, `Palettes`, `Widgets`, `Editor`) | `main::StudioTab` |
| **Mango-Minimal Tokens** | Sharp `0px` geometry & warm dark palette ported 1:1 from `eldertuner-rs` | `theme::{PAPER, PAPER_2, BG_SEL, INK, INK_2, MUTED, OUTER_BORDER, RULE, ACCENT, OK, WARN}` |
