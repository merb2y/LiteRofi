use literofi_core::config::{Config, SortingMethod};
use literofi_theme::{ParseOptions, ThemeSheet, WidgetQuery};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static GH_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        let id = GH_SEQ.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("literofi_gh_{tag}_{}_{}", std::process::id(), id));
        fs::create_dir_all(&p).expect("create temp dir");
        Self(p)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parses_hyprzepyx_launchpad_and_lumen_rasi_themes() {
    let launchpad = r#"
configuration {
    modi: "drun";
    font: "Product Sans 10";
    show-icons: true;
    icon-theme: "Reversal-dark";
    display-drun: "";
    drun-display-format: "{name}";
    sidebar-mode: false;
}

@theme "/dev/null"

* {
    bg: #0d0f18;
    fg: #a5b6cf;
    accent: #c296eb;
    button: #11131c;
    background-color: @bg;
    text-color: @fg;
}

window {
    border-radius: 7px;
    width: 37%;
    padding: 5px;
}

prompt {
    background-color: @button;
    enabled: true;
    padding: 0.5% 32px 0% -0.5%;
    font: "Product Sans 10";
}

listview {
    columns: 4;
    lines: 3;
    cycle: false;
    dynamic: true;
    layout: vertical;
}

element {
    orientation: vertical;
    padding: 2% 0% 2% 0%;
}

element-icon {
    size: 48px;
    horizontal-align: 0.5;
}

element-text, element-icon {
    background-color: inherit;
    text-color: inherit;
}

element selected {
    background-color: @button;
    border-radius: 6px;
}
"#;

    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_str(launchpad, &ParseOptions::default())
        .expect("parse hyprzepyx launchpad");
    let mut cfg = Config::default();
    sheet.apply_configuration(&mut cfg);
    assert!(cfg.show_icons);
    assert_eq!(cfg.drun_display_format, "{name}");
    assert_eq!(cfg.display_drun.as_deref(), Some(""));
    assert_eq!(
        sheet.get_integer(&WidgetQuery::new("listview"), "columns", 1),
        4
    );
    assert_eq!(
        sheet.get_integer(&WidgetQuery::new("listview"), "lines", 1),
        3
    );

    // Round-trip dumped theme parses cleanly again
    let dumped = sheet.dump_theme();
    let mut roundtrip = ThemeSheet::new();
    roundtrip
        .parse_str(&dumped, &ParseOptions::default())
        .expect("roundtrip launchpad");

    let lumen = r#"
configuration {
    modi: "drun";
    show-icons: true;
    sorting-method: "fzf";
}
window {
    transparency: "real";
    background-color: rgba(255, 255, 255, 0.25);
    blur: true;
}
element normal, element alternate {
    background-color: rgba(255, 255, 255, 0.2);
}
"#;
    let mut s2 = ThemeSheet::default_theme().expect("default");
    s2.parse_str(lumen, &ParseOptions::default())
        .expect("parse lumen");
    let mut cfg2 = Config::default();
    s2.apply_configuration(&mut cfg2);
    assert_eq!(cfg2.sorting_method, SortingMethod::Fzf);
}

#[test]
fn parses_tokyonight_config_with_linux_usr_share_theme_fallback() {
    let tmp = TempDirGuard::new("tokyonight");
    let themes_dir = tmp.0.join("themes");
    fs::create_dir_all(&themes_dir).expect("themes dir");

    let tokyonight_rasi = r#"
* {
    bg: #24283b;
    hv: #9274ca;
    primary: #C5C8C6;
    font: "Monospace 11";
    background-color: @bg;
    kl: #7aa2f7;
    black: #000000;
    transparent: rgba(46,52,64,0);
}
window {
    width: 700;
    border-color: @primary;
    border: 3px;
    border-radius: 6px;
}
listview {
    layout: vertical;
    padding: 8px;
    lines: 7;
    columns: 2;
}
element {
    padding: 2px;
    color: @kl;
}
element-icon {
    size: 2.5em;
}
element selected active {
    background: @hv;
    foreground: @bg;
}
"#;
    fs::write(themes_dir.join("tokyonight.rasi"), tokyonight_rasi).expect("write tokyonight");

    let config_rasi = r#"
configuration {
    modi: ["drun", "window", "run"];
    show-icons: true;
    drun-display-format: "{icon} {name}";
    display-drun: "Search ";
    kb-row-up: "Up,Control+k";
    kb-row-down: "Down,Control+j";
}
@theme "/usr/share/rofi/themes/tokyonight.rasi"
"#;

    let opts = ParseOptions {
        search_dirs: vec![themes_dir],
        dmenu_mode: false,
    };
    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_str(config_rasi, &opts)
        .expect("parse tokyonight config with /usr/share/rofi/themes fallback");

    let mut cfg = Config::default();
    sheet.apply_configuration(&mut cfg);
    assert!(cfg.show_icons);
    assert_eq!(cfg.drun_display_format, "{icon} {name}");
    assert_eq!(cfg.display_drun.as_deref(), Some("Search "));
    assert_eq!(
        sheet.get_integer(&WidgetQuery::new("listview"), "columns", 1),
        2
    );
}

#[test]
fn applies_daemon_hotkey_and_kb_from_config_rasi() {
    let rasi = r#"
configuration {
    daemon-hotkey: "Control+Space";
    daemon-mode: "combi";
    kb-row-down: "Down,Control+j";
}
"#;
    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse config");

    let mut cfg = Config::default();
    sheet.apply_configuration(&mut cfg);
    assert_eq!(cfg.daemon_hotkey, "Control+Space");
    assert_eq!(cfg.daemon_mode, "combi");
    assert_eq!(
        cfg.keybindings.get_by_name("kb-row-down"),
        Some("Down,Control+j")
    );

    // Explicit CLI override takes precedence
    let mut cli_cfg = Config {
        daemon_hotkey: "Alt+R".to_string(),
        daemon_hotkey_set: true,
        daemon_mode: "window".to_string(),
        daemon_mode_set: true,
        ..Config::default()
    };
    sheet.apply_configuration(&mut cli_cfg);
    assert_eq!(cli_cfg.daemon_hotkey, "Alt+R");
    assert_eq!(cli_cfg.daemon_mode, "window");
}

#[test]
fn resolves_jakoolit_wallust_subpath_and_shared_fonts() {
    let tmp = TempDirGuard::new("jakoolit");
    let root = &tmp.0;
    let themes_dir = root.join("themes");
    let wallust_dir = root.join("wallust");
    fs::create_dir_all(&themes_dir).expect("themes dir");
    fs::create_dir_all(&wallust_dir).expect("wallust dir");

    fs::write(
        root.join("0-shared-fonts.rasi"),
        "configuration { font: \"Segoe UI 12\"; }\n",
    )
    .expect("shared fonts");
    fs::write(
        wallust_dir.join("colors-rofi.rasi"),
        "* { background: rgba(0,0,0,0.7); foreground: #FAE8E1; color12: #784CA0; }\n",
    )
    .expect("wallust colors");
    let theme_path = themes_dir.join("KooL_style-4.rasi");
    fs::write(
        &theme_path,
        r#"
@import "~/.config/rofi/0-shared-fonts.rasi"
@theme "~/.config/rofi/wallust/colors-rofi.rasi"
* { selected: @color12; }
window { width: 35%; border-radius: 10px; background-color: @background; }
"#,
    )
    .expect("kool style 4");

    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_file(
            &theme_path,
            &ParseOptions {
                search_dirs: vec![themes_dir, root.clone()],
                dmenu_mode: false,
            },
        )
        .expect("parse JaKooLit theme with sibling wallust/colors-rofi.rasi");

    let mut cfg = Config::default();
    sheet.apply_configuration(&mut cfg);
    assert_eq!(cfg.font, "Segoe UI 12");
}

#[test]
fn parses_compound_window_locations_and_empty_sheet_detection() {
    use literofi_core::config::WindowLocation;

    let mut empty = ThemeSheet::new();
    assert!(empty.is_theme_empty());
    empty
        .parse_str(
            "configuration { modi: \"drun\"; }",
            &ParseOptions::default(),
        )
        .expect("parse configuration block");
    assert!(
        empty.is_theme_empty(),
        "configuration-only sheet must still count as empty theme"
    );

    let rasi = r#"
window {
    location: northwest;
    anchor: eastnorth;
}
mainbox {
    location: south-west;
}
"#;
    empty
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse compound positions");
    assert!(!empty.is_theme_empty());

    let q_win = literofi_theme::WidgetQuery::new("window");
    assert_eq!(
        empty.get_position(&q_win, "location", WindowLocation::Center),
        WindowLocation::NorthWest
    );
    assert_eq!(
        empty.get_position(&q_win, "anchor", WindowLocation::Center),
        WindowLocation::NorthEast
    );
    let q_mb = literofi_theme::WidgetQuery::new("mainbox");
    assert_eq!(
        empty.get_position(&q_mb, "location", WindowLocation::Center),
        WindowLocation::SouthWest
    );
}

#[test]
fn parses_env_function_fallback_and_blur_radius_config() {
    let rasi = r#"
configuration {
    font: env(LITEROFI_NONEXISTENT_TEST_FONT_VAR, "JetBrainsMono Nerd Font Medium 12");
    blur-radius: 16;
}
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse env() fallback and blur-radius");
    let mut cfg = Config::default();
    sheet.apply_configuration(&mut cfg);
    assert_eq!(cfg.font, "JetBrainsMono Nerd Font Medium 12");
    assert_eq!(cfg.blur_radius, 16);
}

#[test]
fn parses_utf8_bom_hex_colors_and_builtin_default_import() {
    let rasi =
        "\u{FEFF}@import \"default\"\n* { bg: #1e1e2ee6; }\nwindow { background-color: @bg; }\n";
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse UTF-8 BOM hex color and @import default");
    let bg = sheet.get_color(
        &WidgetQuery::new("window"),
        "background-color",
        literofi_theme::ThemeColor::TRANSPARENT,
    );
    assert!((bg.r - (f64::from(0x1eu8) / 255.0)).abs() < 0.01);
    assert!((bg.a - (f64::from(0xe6u8) / 255.0)).abs() < 0.01);
}

#[test]
fn media_block_before_widget_merges_without_shadowing() {
    let rasi = r#"
* { bg: #1e1e2ee6; accent: #89b4fa; }
@media ( min-aspect-ratio: 1 ) {
    window { width: 760px; }
}
window {
    background-color: @bg;
    border: 2px dash;
    border-color: @accent;
}
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse media-before-widget theme");
    sheet.process_conditionals(&literofi_theme::MonitorGeometry {
        width: 1920,
        height: 1080,
        monitor_id: 0,
    });
    let q_win = WidgetQuery::new("window");
    let bg = sheet.get_color(
        &q_win,
        "background-color",
        literofi_theme::ThemeColor::TRANSPARENT,
    );
    assert!(
        bg.a > 0.8,
        "window background-color must survive @media block declared before it, got {:?}",
        bg
    );
    let w = sheet.get_distance(&q_win, "width", literofi_theme::Distance::px(0.0));
    assert_eq!(
        w.to_pixels(
            literofi_theme::Orientation::Horizontal,
            &literofi_theme::DistanceMetrics::default(),
        ),
        760.0
    );
}

#[test]
fn profile_can_select_a_theme_named_config_and_its_sibling_palette() {
    let tmp = TempDirGuard::new("profile_collision");
    let themes = tmp.0.join("themes");
    fs::create_dir(&themes).unwrap();
    fs::write(tmp.0.join("config.rasi"), r#"@theme "config""#).unwrap();
    fs::write(
        themes.join("config.rasi"),
        r#"@theme "tokyo"
window { width: 480px; }"#,
    )
    .unwrap();
    fs::write(themes.join("tokyo.rasi"), "* { accent: #f7768e; }").unwrap();
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_file(&tmp.0.join("config.rasi"), &Default::default())
        .unwrap();
    let dump = sheet.dump_theme();
    assert!(dump.contains("480px"));
    assert!(dump.contains("247, 118, 142"));
}
