# LiteRofi — Architecture Decision Records (ADRs)

## ADR-001: Windows 11 Native Target & Execution Lifecycle
Status: accepted (2026-11-24)
Context: Rofi (`davatorium/rofi`) is a Linux X11/Wayland launcher with an ephemeral launch-wait-exit lifecycle (`0 MB` idle RAM), whereas Windows launchers often stay resident in RAM.
Decision: Target Windows 11 native (`x86_64-pc-windows-msvc`). Default execution is ephemeral (`literofi -show drun`, `literofi -dmenu`, etc.) with `0 MB` idle RAM after exit. Provide an optional lightweight `literofi --daemon` mode that holds only a Win32 global hotkey listener and spawns/wakes launcher invocations cleanly.
Consequences: No X11/Wayland/D-Bus assumptions leak into core Rust crates; Windows-specific startup and process lifecycle stay fast to keep cold starts snappy.

## ADR-002: Pure Rust UI & Rendering Engine (Win32 + Direct2D + DirectWrite)
Status: accepted (2026-11-24)
Context: `rust-anything analyze` identifies `source/widgets/`, `source/xcb/`, and `source/wayland/` as coupled to `<cairo.h>`, `<pango/pango.h>`, `<xcb/xcb.h>`, `<wayland-client.h>`, and `<xkbcommon/xkbcommon.h>`. Framework GUIs (Slint, Electron, GTK) add ~15–20+ MB binary weight and cannot dynamically instantiate arbitrary nested `.rasi` `children` trees (`box`, `listview`, `scrollbar`, `element`, `textbox`, `icon`).
Decision: Build the UI layer directly on `windows` (`Win32` layered/frameless windowing, keyboard/IME/mouse input, Monitor/DPI handling, WIC image decoding) + **Direct2D** (rounded borders, solid/dashed strokes, gradients, clipping, opacity) + **DirectWrite** (font metrics `em`/`ch`, Pango-like text layout, ellipsize, inline `Pango`/highlight spans).
Consequences: Minimal binary size (~2–5 MB release target), zero GC/toolkit runtime, and 1:1 structural support for `.rasi` widget hierarchies (`window → mainbox → inputbar / message / listview / mode-switcher / ...`).

## ADR-003: Full 1:1 Rofi Feature Parity Scope
Status: accepted (2026-11-24)
Context: The goal for `LiteRofi` is complete behavioral parity with upstream `davatorium/rofi` (`2.1.1-dev`, commit `171c7d79c461756f4c8abce0b4755baafab49c64`), independent of any earlier launcher codebase.
Decision: Implement all upstream modes observed in `reference/rofi/source/modes/`:
- Wave 4 (core): `dmenu`, `run`, `drun` (Windows `.lnk`/`.exe`/AppX desktop discovery), `window`, `windowcd`, `combi`, `script`
- Wave 5 (extended): `filebrowser`, `recursivebrowser`, `ssh`, `keys` (`help-keys`)
Support all 5 matching methods (`MM_NORMAL`, `MM_REGEX`, `MM_GLOB`, `MM_FUZZY`, `MM_PREFIX`), sorting methods (`SORT_NORMAL`, `SORT_FZF`, `SORT_FZF_V2`), custom keybindings (`-kb-*`), mouse bindings (`-me-*` / `-ml-*`), and Rofi diagnostic flags (`-dump-config`, `-dump-theme`, `-dump-xresources`, `-version`, `-h`).
Consequences: Mode logic is defined behind a clean Rust `Mode` trait mirroring `include/mode.h` (`init`, `destroy`, `get_num_entries`, `get_display_value`, `get_icon`, `token_match`, `result`, `preprocess_input`, `get_message`, `completer`), isolating Windows data sources from matching/UI.

## ADR-004: Upstream Baseline & `rust-anything` Porting Strategy
Status: accepted (2026-11-24)
Context: Upstream `reference/rofi` (`davatorium/rofi`, commit `171c7d79c461756f4c8abce0b4755baafab49c64`, version `2.1.1-dev`, MIT) contains 50 C source files (28,731 lines) and 12 C test files in `test/`. `rust-anything analyze --whole-app --lang c` flagged `<glib.h>`, `<cairo.h>`, `<pango/pango.h>`, `<xcb/*>`, and `<wayland-*>` as Linux-specific (`keep`/system library) units while CLI config, matchers, history, `.rasi` lexer/parser (`lexer/theme-lexer.l`, `lexer/theme-parser.y`, `source/theme.c`, `source/css-colors.c`), `dmenuscriptshared`, and box layout math are portable in logic.
Decision: Pin `reference/rofi` (`171c7d79c461756f4c8abce0b4755baafab49c64`) under `.gitignore` strictly as our specification & golden test oracle. Port portable logic units faithfully first into modular Rust crates (`literofi-core`, `literofi-theme`, `literofi-render`, `literofi-modes`, `literofi-app`), backing each unit with unit tests + upstream-derived golden vectors (`reference/rofi/test/*` and `reference/rofi/themes/*.rasi`) and verifying every wave with `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test`.
Consequences: Linux IPC/display units (`source/xcb/`, `source/wayland/`, `libsn`, `nkutils-bindings`) are cleanly replaced by Windows equivalents rather than wrapped through FFI.

## ADR-005: CLI Parser & Minimal Crate Dependencies
Status: accepted (2026-11-24)
Context: Rofi uses single-dash multi-letter CLI options (`-show`, `-dmenu`, `-theme`, `-theme-str`, `-modi`/`-modes`, `-matching`, `-sort`, `-no-sort`, `-i`, `-p`, `-mesg`, `-kb-row-up`, `-dump-config`, `-dump-theme`) plus repeatable overrides. Standard GNU `clap` derives treat `-show` as bundled short flags `-s -h -o -w`.
Decision: Implement a dedicated Rofi-compatible `argv` scanner (`find_arg_*` semantics matching `source/helper.c` and `source/xrmoptions.c`) instead of forcing `clap` short-cluster parsing. Keep third-party crates minimal: `windows` (Win32, Direct2D, DirectWrite, WIC, Shell/COM), `regex` (matching engine & escaping), `globset` or custom wildcard/fnmatch matching, `thiserror` (structured errors where helpful), and `std` for Unicode/collections/processes.
Consequences: CLI syntax matches upstream Rofi scripts and docs identically (`-show drun -i -theme-str '...'`) with tiny compile & binary overhead.

## ADR-006: Behavioral & Protocol Equivalence Bar
Status: accepted (2026-11-24)
Context: Users and Rofi scripts rely on exact token matching, `dmenu`/script stdin/stdout metadata escaping, `.rasi` theme compatibility, and exit codes.
Decision:
1. **Matchers**: Tokenized space-separated AND matching (`helper_tokenize`) supporting negated/regex/fuzzy/glob/prefix/normal behavior + case-sensitivity toggle + FZF/fuzzy scoring parity.
2. **Themes (`.rasi`)**: Must parse all official themes in `reference/rofi/themes/*.rasi` without error (`theme-parser-test` baseline), support `@import` / `@theme`, `configuration { ... }`, `var(...)`, `calc(...)`, `rgb/rgba/hsl/hsla/#hex` & named CSS colors (`css-colors`), `@media`, `:hover` / `.normal` / `.selected` / `.alternate` / `.urgent` / `.active`, and `-theme-str`.
3. **dmenu & script protocol**: Exact `\0key\x1fvalue` metadata parsing (`icon`, `info`, `meta`, `nonselectable`, `permanent`, `urgent`, `active`, plus stream-level `\0prompt\x1f`, `\0message\x1f`, `\0markup-rows\x1f`, `\0no-custom\x1f`, `\0use-hot-keys\x1f`, `\0data\x1f`, `\0keep-selection\x1f`, `\0new-selection\x1f`, `\0theme\x1f`) and `-sep` / `-0` null-delimited I/O.
4. **Exit codes**: `0` accept, `1` cancel/EOF, `10..28` for custom keybindings `CUSTOM_1`..`CUSTOM_19` (`-kb-custom-1..19`).
Consequences: Existing Rofi scripts and `.rasi` theme files work out of the box on Windows (modulo Unix-only paths/commands).

## ADR-007: Footprint & Performance Targets
Status: accepted (2026-11-24)
Context: The user specifically wants Rofi-class lightness rather than heavy framework overhead.
Decision: Target release `literofi.exe` size <= 5 MB (LTO + `opt-level = "s"`/`"z"` or `"3"` tuned after measurement + `strip`), `0 MB` idle RAM in default ephemeral mode, <= 15 MB typical active RAM (without huge icon caches), and sub-50 ms UI ready latency in `dmenu` / `run`.
Consequences: Lazy icon loading (async/on-demand visible-row decoding), zero heavyweight serialization/async runtime dependencies (`no tokio`, `no serde` unless strictly warranted), and compact data structures.

## ADR-008: Repository Layout & Wave Checkpoints
Status: accepted (2026-11-24)
Context: Development proceeds step-by-step from `C:\Users\merby\OC\LiteRofi`.
Decision: Keep `reference/rofi` gitignored; keep architecture docs under `rust/` (`MAP.md`, `DECISIONS.md`, `GLOSSARY.md`, `rofi-analyze.*`); place Cargo workspace under `C:\Users\merby\OC\LiteRofi\Cargo.toml` with focused crates (`crates/literofi-core`, `crates/literofi-theme`, `crates/literofi-render`, `crates/literofi-modes`, `crates/literofi-cli`). Stop at the end of every Wave with passing checks (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`) and observed metrics for user approval.
Consequences: Clean multi-crate compile boundaries and deterministic wave handoffs.
