# LiteRofi — Whole-App Port Map

## Goal
Build **LiteRofi** (`C:\Users\merby\OC\LiteRofi`): a standalone, Windows 11 native, pure-Rust reimplementation of **Rofi** achieving 1:1 behavioral parity across CLI options, `.rasi` themes, matching engines, widget layout, and modes, with minimal binary footprint and `0 MB` idle RAM.

## Upstream Reference & `rust-anything` Analysis
- **Reference repo**: `davatorium/rofi` (version `2.1.1-dev`, MIT, commit `171c7d79c461756f4c8abce0b4755baafab49c64`, `2026-11-23`), cloned under gitignored `reference/rofi/`.
- **Analysis artifacts**: [rofi-analyze.md](rofi-analyze.md), [rofi-analyze.json](rofi-analyze.json) (`rust-anything analyze --whole-app --lang c`).
- **Scope stats from upstream**: 50 C source files (`28,731` lines), 12 upstream C tests (`reference/rofi/test/`), 35+ bundled `.rasi` themes (`reference/rofi/themes/`), grammar files (`lexer/theme-lexer.l`, `lexer/theme-parser.y`).
- **Porting boundaries**:
  - *Portable C logic* -> faithful Rust port + golden tests: matchers (`helper.c`), tokenization, config/CLI parsing (`xrmoptions.c`, `config/config.c`, `rofi.c`), `.rasi` lexer/parser/cascade (`theme-lexer.l`, `theme-parser.y`, `theme.c`, `css-colors.c`), `dmenu`/script entry protocol (`dmenuscriptshared.c`), box/listview layout math (`box.c`, `listview.c`, `scrollbar.c`), history (`history.c`), mode abstractions (`mode.c`).
  - *Linux-only display/system (`keep` / OS replacement)* -> pure-Rust Windows native implementation: Win32 + Direct2D + DirectWrite UI (`source/xcb/`, `source/wayland/`, Cairo/Pango, XKB bindings), Windows Shell/`.lnk`/PATH/AppX discovery (`drun`/`run`), Win32 HWND enumeration (`window`/`windowcd`), Windows WIC/Shell icon loading (`rofi-icon-fetcher.c`).

## Architecture Index
- Design decisions: [DECISIONS.md](DECISIONS.md) (`ADR-001` .. `ADR-008`)
- Domain terminology & type map: [GLOSSARY.md](GLOSSARY.md)

## Wave Plan & Unit Status

| Wave | Unit / Crate | Upstream Reference Scope | Status | Verification Gate |
| --- | --- | --- | --- | --- |
| **0** | Repo, reference clone & whole-app plan | `davatorium/rofi` + `rust-anything analyze` | **done** | `git status`, `rofi-analyze.{md,json}`, `DECISIONS.md`, `GLOSSARY.md`, `MAP.md` |
| **1** | `literofi-core` + `literofi-cli` (CLI/Config, Matchers, History, `dmenu` protocol) | `helper.c`, `xrmoptions.c`, `config/config.c`, `history.c`, `dmenuscriptshared.c`, `keyb.c`, `test/*` | **done** | `6/6` golden test suites pass (`fmt`, `clippy -D warnings`, `release = 175616 B`) |
| **2** | `literofi-theme` (`.rasi` lexer, parser, cascade, CSS colors, `calc()`) | `lexer/theme-lexer.l`, `lexer/theme-parser.y`, `source/theme.c`, `source/css-colors.c`, `themes/*.rasi` | **done** | `33/33` resmi `.rasi` tema + `default_theme`/`default_configuration` + `rasi_golden` geçti (`release = 372736 B`) |
| **3** | `literofi-render` (Win32 + Direct2D + DirectWrite widget & layout engine) | `source/view.c`, `source/widgets/*.c`, `doc/default_theme.rasi` | **done** | `4/4` `layout_golden` (`6/6` `rofi_golden`, `3/3` `rasi_golden`) + `d2d=true, dwrite=true, dpi=120` `--render-self-test` (default, `gruvbox-dark`, `fullscreen-preview`) + `release = 1935360 B` |
| **4** | `literofi-modes` Core + `literofi` CLI (`dmenu`, `run`, `drun`, `window`, `windowcd`, `combi`, `script`) | `source/modes/{dmenu,run,drun,window,combi,script}.c`, `rofi.c` | **done** | `6/6` `modes_e2e` (`6/6` `rofi_golden`, `4/4` `layout_golden`, `3/3` `rasi_golden`) + `--modes-self-test` (`run=1032`, `drun=282`, `combi=1032`) + `--render-self-test` + `release = 2272768 B` |
| **5** | Extended Modes + Daemon + Optimization (`filebrowser`, `recursivebrowser`, `ssh`, `keys`, `--daemon`) | `source/modes/{filebrowser,recursivebrowser,ssh,help-keys}.c` | **done** | `24/24` test (`1` CLI + `6` `rofi_golden` + `10` `modes_e2e` + `4` `layout_golden` + `3` `rasi_golden`) + `--daemon-self-test` (`33 ms`, `idle_kb=8436`) + `--modes-self-test` + `--render-self-test` + `release = 2390528 B` (`2.28 MiB`) |
