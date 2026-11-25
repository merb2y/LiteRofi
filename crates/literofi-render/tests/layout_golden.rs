use literofi_core::config::{Config, MatchingMethod, SortingMethod, WindowLocation};
use literofi_core::keyb::KeyBindingAction;
use literofi_render::font::parse_rofi_font;
use literofi_render::layout::{
    compute_window_rect, layout_box, layout_listview, BoxChildSpec, ListViewLayoutSpec, Rect,
};
use literofi_render::state::{LauncherState, ListEntry, UiOutcome};
use literofi_render::tree::LaidOutTree;
use literofi_theme::{DistanceMetrics, MonitorGeometry, Orientation, ParseOptions, ThemeSheet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn golden_box_test_horizontal_and_vertical() {
    let hor = Rect::new(0.0, 0.0, 100.0, 20.0);
    // wid1 disabled -> (0,0)
    let r = layout_box(
        hor,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[BoxChildSpec {
            enabled: false,
            expand: true,
            desired_w: 0,
            desired_h: 0,
        }],
    );
    assert_eq!(r[0], Rect::ZERO);

    // wid1 enabled expand -> 100x20
    let r = layout_box(
        hor,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[BoxChildSpec {
            enabled: true,
            expand: true,
            desired_w: 0,
            desired_h: 0,
        }],
    );
    assert_eq!((r[0].w, r[0].h), (100.0, 20.0));

    // +wid2 expand -> 49/49
    let r = layout_box(
        hor,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
        ],
    );
    assert_eq!((r[0].w, r[0].h), (49.0, 20.0));
    assert_eq!((r[1].w, r[1].h), (49.0, 20.0));

    // +wid3 fixed (0) -> 48/48
    let r = layout_box(
        hor,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 0,
                desired_h: 0,
            },
        ],
    );
    assert_eq!((r[0].w, r[1].w), (48.0, 48.0));

    // wid3 resized to (20, 10) -> 38/38/20
    let r = layout_box(
        hor,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 10,
            },
        ],
    );
    assert_eq!((r[0].w, r[1].w, r[2].w, r[2].h), (38.0, 38.0, 20.0, 20.0));

    // box resized to 200x20 -> 88/88/20; +wid4(20) at x=180; +wid5(expand) at x=149
    let hor2 = Rect::new(0.0, 0.0, 200.0, 20.0);
    let r = layout_box(
        hor2,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 10,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 20,
            },
        ],
    );
    assert_eq!(r[3].x, 180.0);

    let r = layout_box(
        hor2,
        Orientation::Horizontal,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 10,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 20,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 20,
                desired_h: 20,
            },
        ],
    );
    assert_eq!(r[4].x, 149.0);

    // Vertical mirror
    let vert2 = Rect::new(0.0, 0.0, 20.0, 200.0);
    let rv = layout_box(
        vert2,
        Orientation::Vertical,
        (0, 0, 0, 0),
        2,
        &[
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 0,
                desired_h: 0,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 10,
                desired_h: 20,
            },
            BoxChildSpec {
                enabled: true,
                expand: false,
                desired_w: 20,
                desired_h: 20,
            },
            BoxChildSpec {
                enabled: true,
                expand: true,
                desired_w: 20,
                desired_h: 20,
            },
        ],
    );
    assert_eq!((rv[0].h, rv[1].h, rv[2].h), (50.0, 51.0, 20.0));
    assert_eq!(rv[3].y, 127.0);
    assert_eq!(rv[4].y, 149.0);
}

#[test]
fn listview_grid_scrollbar_and_window_gravity() {
    let lv = layout_listview(&ListViewLayoutSpec {
        area: Rect::new(10.0, 20.0, 400.0, 200.0),
        columns: 4,
        lines: 2,
        spacing_x: 4.0,
        spacing_y: 4.0,
        flow: Orientation::Horizontal,
        show_scrollbar: true,
        scrollbar_width: 6.0,
        total_items: 20,
        scroll_offset: 8,
    });
    assert_eq!(lv.page_size, 8);
    assert_eq!(lv.cells.len(), 8);
    assert_eq!(lv.cells[0].0, 8);
    assert_eq!(lv.cells[1].0, 9);
    // Horizontal flow places cell 1 to the right of cell 0
    assert!(lv.cells[1].1.x > lv.cells[0].1.x);
    assert_eq!(lv.cells[1].1.y, lv.cells[0].1.y);
    assert!(lv.scrollbar_track.is_some());
    assert!(lv.scrollbar_thumb.is_some());

    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let centered = compute_window_rect(
        wa,
        600.0,
        400.0,
        WindowLocation::Center,
        WindowLocation::Center,
        0.0,
        0.0,
        false,
    );
    assert_eq!(centered, Rect::new(660.0, 340.0, 600.0, 400.0));
    let nw = compute_window_rect(
        wa,
        600.0,
        400.0,
        WindowLocation::NorthWest,
        WindowLocation::NorthWest,
        15.0,
        20.0,
        false,
    );
    assert_eq!(nw, Rect::new(15.0, 20.0, 600.0, 400.0));
    let fs = compute_window_rect(
        wa,
        600.0,
        400.0,
        WindowLocation::Center,
        WindowLocation::Center,
        0.0,
        0.0,
        true,
    );
    assert_eq!(fs, wa);

    let pf = parse_rofi_font("Segoe UI Bold Italic 13.5");
    assert_eq!(pf.family, "Segoe UI");
    assert!(pf.bold && pf.italic);
    assert!((pf.size_pt - 13.5).abs() < 0.01);
}

#[test]
fn builds_layout_trees_for_official_themes() {
    let themes_dir = repo_root().join("reference").join("rofi").join("themes");
    let opts = ParseOptions {
        search_dirs: vec![themes_dir.clone()],
        dmenu_mode: false,
    };
    let entries: Vec<ListEntry> = ["Firefox", "Code", "Terminal", "Files", "Notes"]
        .into_iter()
        .map(|s| ListEntry {
            text: s.to_string(),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        sidebar_mode: true,
        mesg: Some("Quick Pick".to_string()),
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics::default();
    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);

    for rel in ["gruvbox-dark.rasi", "fullscreen-preview.rasi", "iggy.rasi"] {
        let mut sheet = ThemeSheet::default_theme().expect("default");
        sheet
            .parse_file(&themes_dir.join(rel), &opts)
            .unwrap_or_else(|e| panic!("failed {rel}: {e}"));
        sheet.process_conditionals(&MonitorGeometry::default());

        let tree = LaidOutTree::build(
            &sheet,
            &state,
            wa,
            &metrics,
            &cfg.font,
            cfg.location,
            cfg.x_offset,
            cfg.y_offset,
            cfg.sidebar_mode,
            cfg.show_icons,
        );
        assert!(tree.window_rect.w > 200.0 && tree.window_rect.h > 100.0);
        assert!(tree.entry_rect.w > 20.0 && tree.entry_rect.h > 10.0);
        assert!(tree.listview_rect.w > 50.0 && tree.listview_rect.h > 20.0);
        assert_eq!(tree.cells.len(), 5);
    }
}

#[test]
fn interactive_launcher_state_filtering_editing_and_actions() {
    let mut cfg = Config {
        sort: true,
        sorting_method: SortingMethod::FzfV2,
        matching: MatchingMethod::Fuzzy,
        ..Config::default()
    };
    cfg.modes = "drun,run".to_string();

    let entries = vec![
        ListEntry {
            text: "--- Section ---".to_string(),
            nonselectable: true,
            permanent: true,
            ..Default::default()
        },
        ListEntry {
            text: "Şifre Kasası".to_string(),
            ..Default::default()
        },
        ListEntry {
            text: "Firefox".to_string(),
            ..Default::default()
        },
        ListEntry {
            text: "Files".to_string(),
            ..Default::default()
        },
    ];
    let mut st = LauncherState::new(&cfg, entries);
    // Initial selection skips nonselectable row 0
    assert_eq!(st.selected, 1);

    // UTF-8 insert + backspace + edit actions
    for c in "fi".chars() {
        st.insert_char(c);
    }
    assert_eq!(st.query, "fi");
    assert_eq!(
        st.filtered
            .iter()
            .map(|&i| st.entries[i].text.as_str())
            .collect::<Vec<_>>(),
        vec!["Files", "Firefox", "--- Section ---"]
    );
    assert_eq!(st.selected, 0); // Files

    // Navigation & custom exit code
    let _ = st.handle_action(KeyBindingAction::RowDown, 1, 5);
    assert_eq!(st.selected, 1); // Firefox
    let out = st.handle_action(KeyBindingAction::Custom(3), 1, 5);
    assert_eq!(
        out,
        Some(UiOutcome::Accepted {
            index: 2,
            text: "Firefox".to_string(),
            exit_code: 12
        })
    );

    // Mode switch & cancel
    assert_eq!(
        st.handle_action(KeyBindingAction::ModeNext, 1, 5),
        Some(UiOutcome::ModeSwitch(1))
    );
    assert_eq!(
        st.handle_action(KeyBindingAction::Cancel, 1, 5),
        Some(UiOutcome::Cancelled)
    );

    // UTF-8 editing boundaries
    st.clear_line();
    st.insert_str("şifre aç");
    st.move_cursor_left();
    st.backspace();
    assert_eq!(st.query, "şifre ç");
    st.remove_to_sol();
    assert_eq!(st.query, "ç");
}

#[test]
fn steam_exact_name_ranks_above_meta_folder_matches() {
    let entries = vec![
        ListEntry {
            text: "EA SPORTS FC 27 Lite".to_string(),
            meta: Some("Steam".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Half-Life".to_string(),
            meta: Some("Steam".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Hotline Miami".to_string(),
            meta: Some("Steam".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Rust".to_string(),
            meta: Some("Steam".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Slay the Spire".to_string(),
            meta: Some("Steam".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Steam Link".to_string(),
            meta: Some("Application".to_string()),
            ..Default::default()
        },
        ListEntry {
            text: "Steam".to_string(),
            meta: Some("Application".to_string()),
            ..Default::default()
        },
    ];

    for sort in [false, true] {
        let cfg = Config {
            sort,
            sorting_method: SortingMethod::FzfV2,
            ..Config::default()
        };
        let mut st = LauncherState::new(&cfg, entries.clone());
        st.insert_str("steam");
        let labels: Vec<&str> = st
            .filtered
            .iter()
            .map(|&i| st.entries[i].text.as_str())
            .collect();
        assert_eq!(
            labels[0], "Steam",
            "exact match must be first (sort={sort})"
        );
        assert_eq!(
            labels[1], "Steam Link",
            "prefix match must outrank folder matches (sort={sort})"
        );
        assert_eq!(st.selected, 0);
        assert_eq!(labels.len(), 7);
    }
}

#[test]
fn sidebar_v2_dmenu_and_fancy_layout_fidelity() {
    use literofi_render::tree::RenderWidgetKind;
    use literofi_theme::ThemeColor;

    let themes_dir = repo_root().join("reference").join("rofi").join("themes");
    let opts = ParseOptions {
        search_dirs: vec![themes_dir.clone()],
        dmenu_mode: false,
    };
    let entries: Vec<ListEntry> = (0..24)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            icon: Some(format!("icon-{i}")),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    let mut state = LauncherState::new(&cfg, entries);
    state.modes = vec!["drun".to_string(), "run".to_string()];
    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let metrics = DistanceMetrics::default();

    // 1. sidebar-v2.rasi: `lines: 0` expands to multiple rows, icon-only cells, compact current-entry widgets
    let mut sb_sheet = ThemeSheet::new();
    sb_sheet
        .parse_file(&themes_dir.join("sidebar-v2.rasi"), &opts)
        .expect("parse sidebar-v2");
    sb_sheet.process_conditionals(&MonitorGeometry::default());
    let sb_tree = LaidOutTree::build(
        &sb_sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );
    assert_eq!(sb_tree.listview_columns, 8);
    assert!(
        sb_tree.listview_lines >= 3,
        "lines: 0 should compute multi-row capacity, got {}",
        sb_tree.listview_lines
    );
    assert!(sb_tree.cells.len() >= 24);
    assert!(sb_tree.cells[0].icon_rect.is_some());
    assert_eq!(sb_tree.cells[0].text_rect, Rect::ZERO);
    assert_eq!(sb_tree.cells[1].row_style.foreground, ThemeColor::WHITE);

    let prompt_w = sb_tree
        .widgets
        .iter()
        .find(|w| w.name == "prompt")
        .expect("prompt");
    assert_eq!(prompt_w.style.foreground, ThemeColor::BLACK);

    let ico_cur = sb_tree
        .widgets
        .iter()
        .find(|w| w.name == "icon-current-entry")
        .expect("icon-current-entry");
    assert_eq!(
        ico_cur.kind,
        RenderWidgetKind::Icon(Some("icon-current-entry".to_string()))
    );

    let tb_cur = sb_tree
        .widgets
        .iter()
        .find(|w| w.name == "textbox-current-entry")
        .expect("textbox-current-entry");
    assert_eq!(tb_cur.kind, RenderWidgetKind::CurrentEntryTextbox);
    assert!(
        tb_cur.rect.h < 80.0,
        "textbox-current-entry must not expand vertically, got h={}",
        tb_cur.rect.h
    );

    // 2. dmenu.rasi: horizontal single-line top bar (`layout: horizontal`)
    let mut dm_sheet = ThemeSheet::new();
    dm_sheet
        .parse_file(&themes_dir.join("dmenu.rasi"), &opts)
        .expect("parse dmenu");
    dm_sheet.process_conditionals(&MonitorGeometry::default());
    let dm_tree = LaidOutTree::build(
        &dm_sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        false,
    );
    assert!(
        dm_tree.window_rect.h < 80.0,
        "dmenu horizontal bar should be single-row tall, got h={}",
        dm_tree.window_rect.h
    );
    assert!(dm_tree.cells.len() >= 5);
    assert!(dm_tree.cells[1].rect.x > dm_tree.cells[0].rect.x);
    assert_eq!(dm_tree.cells[1].rect.y, dm_tree.cells[0].rect.y);

    // 3. fancy.rasi: `entry` inside `wrapper` inherits `wrapper`'s black text-color
    let mut fancy_sheet = ThemeSheet::new();
    fancy_sheet
        .parse_file(&themes_dir.join("fancy.rasi"), &opts)
        .expect("parse fancy");
    fancy_sheet.process_conditionals(&MonitorGeometry::default());
    let fancy_tree = LaidOutTree::build(
        &fancy_sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );
    let entry_w = fancy_tree
        .widgets
        .iter()
        .find(|w| w.name == "entry")
        .expect("fancy entry");
    assert_eq!(entry_w.style.foreground, ThemeColor::BLACK);
    assert!(entry_w.inner_rect.w > 100.0 && entry_w.inner_rect.h >= 16.0);
}

#[test]
fn fancy_v2_margin_normal_state_overlay_and_message_textbox() {
    use literofi_render::tree::RenderWidgetKind;
    use literofi_theme::ThemeColor;

    let themes_dir = repo_root().join("reference").join("rofi").join("themes");
    let opts = ParseOptions {
        search_dirs: vec![themes_dir.clone()],
        dmenu_mode: false,
    };
    let entries: Vec<ListEntry> = (0..12)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            icon: Some(format!("icon-{i}")),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        sort: true,
        mesg: Some("System Message".to_string()),
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let metrics = DistanceMetrics::default();

    let mut f2_sheet = ThemeSheet::default_theme().expect("default");
    f2_sheet
        .parse_file(&themes_dir.join("fancy2.rasi"), &opts)
        .expect("parse fancy2");
    f2_sheet.process_conditionals(&MonitorGeometry::default());

    let tree = LaidOutTree::build(
        &f2_sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );

    // 5. `window` (`widgets[0]`) links hierarchically to `mainbox`
    assert_eq!(tree.widgets[0].name, "window");
    assert!(
        !tree.widgets[0].children_indices.is_empty(),
        "window must link children_indices to mainbox"
    );
    let first_child_idx = tree.widgets[0].children_indices[0];
    assert_eq!(tree.widgets[first_child_idx].name, "mainbox");

    // 1. `listview.paint_rect` is inset by margin (`20 30 30 30`) relative to `listview.rect`
    let lv_w = tree
        .widgets
        .iter()
        .find(|w| w.name == "listview")
        .expect("listview widget");
    assert!((lv_w.paint_rect.y - (lv_w.rect.y + 20.0)).abs() < 0.5);
    assert!((lv_w.paint_rect.x - (lv_w.rect.x + 30.0)).abs() < 0.5);
    assert!((lv_w.paint_rect.w - (lv_w.rect.w - 60.0)).abs() < 0.5);
    assert!((lv_w.paint_rect.h - (lv_w.rect.h - 50.0)).abs() < 0.5);

    // 2. `prompt`, `entry`, `case-indicator` match `normal.normal` rules (#00000000 bg, #ffffff fg)
    for wname in ["prompt", "entry", "case-indicator"] {
        let w = tree
            .widgets
            .iter()
            .find(|w| w.name == wname)
            .unwrap_or_else(|| panic!("missing widget {wname}"));
        assert_eq!(
            w.style.background.a, 0.0,
            "{wname} must match normal.normal transparent background, got {:?}",
            w.style.background
        );
        assert_eq!(
            w.style.foreground,
            ThemeColor::WHITE,
            "{wname} must match normal.normal white foreground, got {:?}",
            w.style.foreground
        );
    }

    // 3. `overlay` is omitted/disabled when `state.overlay.is_none()`
    assert!(
        !tree.widgets.iter().any(|w| w.name == "overlay"),
        "overlay should be disabled when state.overlay is None"
    );

    // 4. `message` contains child `"textbox"` rendered as `MessageTextbox`
    let msg_w = tree
        .widgets
        .iter()
        .find(|w| w.name == "message")
        .expect("message widget");
    assert_eq!(msg_w.kind, RenderWidgetKind::Box);
    assert_eq!(msg_w.children_indices.len(), 1);
    let tb_w = &tree.widgets[msg_w.children_indices[0]];
    assert_eq!(tb_w.name, "textbox");
    assert_eq!(tb_w.kind, RenderWidgetKind::MessageTextbox);
}

#[test]
fn rofi_engine_south_anchor_fixed_row_height_state_padding_inherit_and_vertical_icon_pct() {
    // 1. `location: south` without explicit `anchor` must anchor to `south` (stays on-screen),
    //    `element selected normal { padding: 20px; }` must NOT override base `padding: 8px`
    //    (`widget.c:63-78`), and `listview` cells use natural `element_height` (never stretched).
    let win11_like = r#"
        * {
            bg0: #ffffff80;
            bg1: #f9f9f9bf;
            fg0: #1a1a1a;
            accent: #005fb8;
            background-color: transparent;
            text-color: @fg0;
            margin: 0;
            padding: 0;
            spacing: 0;
        }
        window {
            location: south;
            width: 40%;
            height: 50%;
            y-offset: -10px;
            background-color: @bg1;
            border-radius: 8px;
        }
        mainbox { padding: 24px; spacing: 24px; }
        inputbar {
            padding: 8px;
            spacing: 4px;
            children: [ icon-search, entry ];
            border: 0 0 2px 0 solid;
            border-color: @accent;
            background-color: @bg0;
        }
        icon-search { expand: false; filename: "search-symbolic"; size: 24px; }
        entry { horizontal-align: 0.5; background-color: inherit; }
        listview { columns: 2; spacing: 8px; fixed-height: true; fixed-columns: true; }
        element { spacing: 1em; padding: 8px; border-radius: 2px; }
        element-text selected,
        element selected normal {
            background-color: @bg0;
            text-color: @accent;
            padding: 20px;
            border-radius: 12px;
        }
        element-icon { size: 1.5em; }
        element-text { text-color: inherit; }
    "#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(win11_like, &ParseOptions::default())
        .expect("parse win11_like");

    let entries: Vec<ListEntry> = (0..24)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            icon: Some(format!("icon-{i}")),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let wa = Rect::new(0.0, 0.0, 2048.0, 1112.0);
    let metrics = DistanceMetrics {
        monitor_width: 2048.0,
        monitor_height: 1152.0,
        char_height: 19.5,
        char_width: 8.0,
        dpi: 120.0,
    };

    let tree = LaidOutTree::build(
        &sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );

    // Window stays 100% inside workarea, anchored to south - 10px
    assert!(
        tree.window_rect.y >= 0.0 && (tree.window_rect.y + tree.window_rect.h) <= (wa.h - 9.0),
        "south-located window must anchor to south above taskbar, got {:?}",
        tree.window_rect
    );
    // Natural row height fits >= 8 rows per column (16+ visible items in 2 columns)
    assert!(
        tree.listview_lines >= 8 && tree.cells.len() >= 16,
        "listview should fit compact natural rows without stretching, got lines={} cells={}",
        tree.listview_lines,
        tree.cells.len()
    );
    // Selected element keeps base padding 8px (NOT 20px) while updating border-radius to 12px
    let sel_cell = &tree.cells[0];
    assert_eq!(sel_cell.row_style.padding, [8.0, 8.0, 8.0, 8.0]);
    assert_eq!(sel_cell.row_style.border_radius, [12.0, 12.0, 12.0, 12.0]);
    assert_eq!(sel_cell.text_style.padding, [0.0, 0.0, 0.0, 0.0]);
    // Selected icon is NOT shrunk by selected padding
    let sel_ico = sel_cell.icon_rect.expect("sel icon");
    let norm_ico = tree.cells[1].icon_rect.expect("norm icon");
    assert!(
        (sel_ico.h - norm_ico.h).abs() < 1.0,
        "selected icon must match normal icon height: sel={} norm={}",
        sel_ico.h,
        norm_ico.h
    );
    // `entry { background-color: inherit; }` inherits `#ffffff80` from `inputbar` (NOT transparent)
    let entry_w = tree
        .widgets
        .iter()
        .find(|w| w.name == "entry")
        .expect("entry");
    assert!(
        entry_w.style.background.a > 0.4,
        "entry background-color: inherit must inherit inputbar @bg0, got {:?}",
        entry_w.style.background
    );

    // 2. `element-icon { size: 3%; }` scales along Vertical monitor height (`icon.c:174`),
    //    and `mode-switcher` is NOT double-inset by its own margin/padding.
    let pct_theme = r#"
        * { background-color: transparent; text-color: white; }
        window { width: 50%; background-color: black; }
        mainbox { children: [ mode-switcher, listview ]; }
        mode-switcher { margin: 0px 10%; spacing: 10px; }
        button { padding: 6px; background-color: blue; }
        listview { columns: 2; lines: 4; }
        element { padding: 8px; }
        element-icon { size: 3%; }
    "#;
    let mut sheet2 = ThemeSheet::new();
    sheet2
        .parse_str(pct_theme, &ParseOptions::default())
        .expect("parse pct_theme");
    let mut st2 = LauncherState::new(&cfg, state.entries.clone());
    st2.modes = vec!["drun".to_string(), "run".to_string()];
    let tree2 = LaidOutTree::build(
        &sheet2,
        &st2,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        true,
        true,
    );
    let ico2 = tree2.cells[0].icon_rect.expect("pct icon");
    let expected_v = (1152.0 * 0.03) as f32; // ~34.56, NOT 2048 * 0.03 = 61.44
    assert!(
        (ico2.h - expected_v).abs() < 2.0,
        "element-icon 3% must scale by monitor height (~{expected_v}), got {}",
        ico2.h
    );
    let ms_w = tree2
        .widgets
        .iter()
        .find(|w| w.name == "mode-switcher")
        .expect("mode-switcher");
    let first_btn = &tree2.mode_buttons[0];
    assert!(
        (first_btn.rect.x - ms_w.inner_rect.x).abs() < 2.0,
        "mode-switcher button must not be double-inset: btn.x={} ms.inner.x={}",
        first_btn.rect.x,
        ms_w.inner_rect.x
    );
}

#[test]
fn scrollbar_box_padding_measurer_width_and_emoji_prompt_width() {
    let rasi = r#"
window { width: 420px; }
inputbar { children: [ prompt, textbox-prompt-colon, entry ]; spacing: 4px; }
textbox-prompt-colon { expand: false; str: " 🔎"; }
listview { lines: 4; scrollbar: true; spacing: 2px; }
scrollbar { handle-width: 4px; padding: 0 4px; border-radius: 2px; }
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse scrollbar and emoji theme");
    let cfg = Config {
        prompt: Some("Search Apps".to_string()),
        ..Config::default()
    };
    let entries: Vec<ListEntry> = (0..12)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            ..Default::default()
        })
        .collect();
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics {
        monitor_width: 1920.0,
        monitor_height: 1080.0,
        char_height: 18.0,
        char_width: 9.0,
        dpi: 96.0,
    };
    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);

    let tree_fallback = LaidOutTree::build(
        &sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        false,
    );
    let track = tree_fallback
        .scrollbar_track
        .expect("scrollbar outer track");
    let thumb = tree_fallback.scrollbar_thumb.expect("scrollbar thumb");
    assert!(
        (track.w - 12.0).abs() < 0.5,
        "expected outer scrollbar track width 12px (4px handle + 2*4px padding), got {}",
        track.w
    );
    assert!(
        (thumb.w - 4.0).abs() < 0.5,
        "expected handle width 4px inside padding, got {}",
        thumb.w
    );
    assert!(
        (thumb.x - (track.x + 4.0)).abs() < 0.5,
        "expected thumb x inset by 4px padding: thumb.x={} track.x={}",
        thumb.x,
        track.x
    );
    assert!(
        thumb.h >= 20.0,
        "expected minimum readable thumb height >= 20px, got {}",
        thumb.h
    );

    let colon_fb = tree_fallback
        .widgets
        .iter()
        .find(|w| w.name == "textbox-prompt-colon")
        .expect("textbox-prompt-colon");
    assert!(
        colon_fb.rect.w >= 28.0,
        "wide emoji prompt colon must not shrink to 2 narrow chars, got {}",
        colon_fb.rect.w
    );

    let measured_tree = LaidOutTree::build_with_measurer(
        &sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        false,
        false,
        Some(&|_font, text| match text {
            "Search Apps" => (73.0, 20.0),
            " 🔎" => (24.0, 20.0),
            _ => (text.len() as f32 * 7.0, 20.0),
        }),
    );
    let prompt_m = measured_tree
        .widgets
        .iter()
        .find(|w| w.name == "prompt")
        .expect("measured prompt");
    assert!(
        (prompt_m.rect.w - 77.0).abs() <= 2.0,
        "expected measured prompt width ~77px, got {}",
        prompt_m.rect.w
    );
    let colon_m = measured_tree
        .widgets
        .iter()
        .find(|w| w.name == "textbox-prompt-colon")
        .expect("measured colon");
    assert!(
        (colon_m.rect.w - 28.0).abs() <= 2.0,
        "expected measured colon width ~28px, got {}",
        colon_m.rect.w
    );
}

#[test]
fn multiline_message_height_iconless_mode_and_element_text_color_inherit() {
    let rasi = r#"
* { background-color: transparent; text-color: white; }
window { width: 400px; height: 500px; }
mainbox { padding: 20px; spacing: 12px; children: [ inputbar, message, listview ]; }
textbox { padding: 10px; }
listview { columns: 2; lines: 8; spacing: 8px; }
element { padding: 8px; spacing: 1em; }
element selected { background-color: lightgrey; text-color: black; }
element-icon { size: 1.5em; }
element-text { horizontal-align: 0.5; }
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse docu/win11 hybrid theme");
    let cfg = Config {
        show_icons: true,
        mesg: Some(
            "Select a theme and press Enter (applies automatically on next Alt+Space)".to_string(),
        ),
        ..Config::default()
    };
    // Icon-less mode (like `-show theme` / `-show run`): no entries carry icons
    let entries: Vec<ListEntry> = (0..10)
        .map(|i| ListEntry {
            text: format!("ThemeName-{i}"),
            icon: None,
            ..Default::default()
        })
        .collect();
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics {
        monitor_width: 1920.0,
        monitor_height: 1080.0,
        char_height: 18.0,
        char_width: 9.0,
        dpi: 96.0,
    };
    let wa = Rect::new(0.0, 0.0, 1920.0, 1080.0);

    let tree = LaidOutTree::build(
        &sheet,
        &state,
        wa,
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );

    // 1. Multiline message textbox expands beyond 1 line (18px) + 20px padding = 38px
    let msg_tb = tree
        .widgets
        .iter()
        .find(|w| w.name == "textbox")
        .expect("message textbox");
    assert!(
        msg_tb.inner_rect.h >= 34.0,
        "wrapped 2-line message textbox inner height must fit >=2 lines, got {}",
        msg_tb.inner_rect.h
    );

    // 2. Icon-less mode must NOT reserve dead ghost icon slots when no entries have icons
    assert!(
        tree.cells[0].icon_rect.is_none(),
        "icon-less mode must not reserve empty icon space"
    );
    assert!(
        (tree.cells[0].text_rect.x - (tree.cells[0].rect.x + 8.0)).abs() < 1.0,
        "element-text must start at element padding without ghost icon gap: text.x={} cell.x={}",
        tree.cells[0].text_rect.x,
        tree.cells[0].rect.x
    );

    // 3. `element-text` without explicit `text-color` inherits `element selected`'s black (not `*` white)
    assert_eq!(
        tree.cells[0].text_style.foreground,
        literofi_theme::ThemeColor::BLACK,
        "element-text must inherit parent element selected text-color"
    );
}

#[test]
fn oneline_bar_height_win11_ten_rows_and_rounded_height() {
    let entries: Vec<ListEntry> = (0..30)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            icon: Some(format!("icon-{i}")),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        prompt: Some("drun".to_string()),
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let wa = Rect::new(0.0, 0.0, 2048.0, 1104.0);

    // 1. Horizontal oneline bar (`nord-oneline`) must NOT clamp to 40px minimum height
    let oneline = r#"
* { font: "Fira Code 10"; margin: 0px; padding: 0px; spacing: 0px; background-color: transparent; text-color: #d8dee9; }
window { location: north; width: 100%; background-color: #2e3440; children: [ mainbox, message ]; }
mainbox { orientation: horizontal; children: [ inputbar, listview ]; }
inputbar { width: 25%; padding: 1px 8px; spacing: 8px; children: [ prompt, entry ]; }
listview { layout: horizontal; }
element { padding: 1px 8px; spacing: 4px; }
element-icon { size: 0.75em; }
"#;
    let mut s1 = ThemeSheet::new();
    s1.parse_str(oneline, &ParseOptions::default())
        .expect("parse oneline");
    let m10 = DistanceMetrics {
        monitor_width: 2048.0,
        monitor_height: 1152.0,
        char_height: 16.0,
        char_width: 8.0,
        dpi: 120.0,
    };
    let t1 = LaidOutTree::build(
        &s1,
        &state,
        wa,
        &m10,
        "Fira Code 10",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );
    assert!(
        t1.window_rect.h <= 24.0,
        "nord-oneline bar must stay compact (~18-20px), got {}",
        t1.window_rect.h
    );

    // 2. `windows11-list` 600x600 fixed-height with `Roboto 10` (`char_height = 16.0`) must fit 10 rows x 2 cols
    let win11_list = r#"
* { font: "Roboto 10"; margin: 0; padding: 0; spacing: 0; background-color: transparent; text-color: #ffffff; }
window { location: south; width: 600px; height: 600px; y-offset: -4px; background-color: #202020bf; border-radius: 8px; }
mainbox { padding: 24px; spacing: 24px; }
inputbar { padding: 8px; spacing: 4px; children: [ icon-search, entry ]; border: 0 0 2px 0 solid; }
icon-search { expand: false; size: 24px; }
entry { font: "Roboto 12"; }
listview { columns: 2; spacing: 8px; fixed-height: true; fixed-columns: true; }
element { spacing: 1em; padding: 8px; }
element-icon { size: 1.5em; }
"#;
    let mut s2 = ThemeSheet::new();
    s2.parse_str(win11_list, &ParseOptions::default())
        .expect("parse win11_list");
    let t2 = LaidOutTree::build(
        &s2,
        &state,
        wa,
        &m10,
        "Roboto 10",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );
    assert_eq!(
        t2.listview_lines, 10,
        "windows11-list must fit 10 rows per column (got {})",
        t2.listview_lines
    );
    assert_eq!(
        t2.cells.len(),
        20,
        "windows11-list must display 20 items (10x2)"
    );

    // 3. `rounded-template` (`Roboto 12`, `char_height = 19.0`) auto-height window matches upstream 355px
    let rounded = r#"
* { font: "Roboto 12"; margin: 0px; padding: 0px; spacing: 0px; background-color: transparent; text-color: #d8dee9; }
window { location: north; y-offset: calc(50% - 176px); width: 480; border-radius: 24px; background-color: #2e3440; }
mainbox { padding: 12px; }
inputbar { border: 2px; border-radius: 16px; padding: 8px 16px; spacing: 8px; children: [ prompt, entry ]; }
listview { margin: 12px 0 0; lines: 8; columns: 1; fixed-height: false; }
element { padding: 8px 16px; spacing: 8px; border-radius: 16px; }
element-icon { size: 1em; }
"#;
    let mut s3 = ThemeSheet::new();
    s3.parse_str(rounded, &ParseOptions::default())
        .expect("parse rounded");
    let m12 = DistanceMetrics {
        monitor_width: 2048.0,
        monitor_height: 1152.0,
        char_height: 19.0,
        char_width: 8.5,
        dpi: 120.0,
    };
    let t3 = LaidOutTree::build(
        &s3,
        &state,
        wa,
        &m12,
        "Roboto 12",
        WindowLocation::Center,
        0,
        0,
        false,
        true,
    );
    assert!(
        (t3.window_rect.h - 355.0).abs() <= 2.0,
        "rounded-template window height should match upstream 355px, got {}",
        t3.window_rect.h
    );
}

#[test]
fn theme_config_show_icons_false_hides_drun_icons() {
    let rasi = r#"
configuration { show-icons: false; }
* { background-color: transparent; text-color: white; }
window { width: 480px; background-color: #1e222a; }
listview { lines: 6; }
element { padding: 8px; }
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse show-icons false theme");
    let mut cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    sheet.apply_configuration(&mut cfg);
    assert!(!cfg.show_icons);
    assert!(cfg.hide_icons);

    let entries: Vec<ListEntry> = (0..6)
        .map(|i| ListEntry {
            text: format!("App {i}"),
            icon: Some(format!("icon-{i}")),
            ..Default::default()
        })
        .collect();
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics {
        monitor_width: 1920.0,
        monitor_height: 1080.0,
        char_height: 16.0,
        char_width: 8.0,
        dpi: 96.0,
    };
    let tree = LaidOutTree::build_with_options(
        &sheet,
        &state,
        Rect::new(0.0, 0.0, 1920.0, 1080.0),
        &metrics,
        "Segoe UI 10",
        WindowLocation::Center,
        0,
        0,
        false,
        cfg.show_icons,
        cfg.hide_icons,
    );
    assert!(
        tree.cells[0].icon_rect.is_none(),
        "configuration {{ show-icons: false; }} must suppress drun icons"
    );
}

#[test]
fn element_text_does_not_bleed_global_star_background_over_selected_pill() {
    let rasi = r#"
* { background-color: #0c0816; text-color: #8fc5c6; }
window { width: 420px; }
listview { lines: 4; }
element { padding: 10px; border-radius: 50%; }
element selected.normal { background-color: rgb(170, 70, 104); text-color: #0c0816; }
"#;
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_str(rasi, &ParseOptions::default())
        .expect("parse flamingo-like theme");
    let entries = vec![ListEntry {
        text: "Steam".to_string(),
        ..Default::default()
    }];
    let cfg = Config::default();
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics {
        monitor_width: 1920.0,
        monitor_height: 1080.0,
        char_height: 16.0,
        char_width: 8.0,
        dpi: 96.0,
    };
    let tree = LaidOutTree::build(
        &sheet,
        &state,
        Rect::new(0.0, 0.0, 1920.0, 1080.0),
        &metrics,
        "Segoe UI 11",
        WindowLocation::Center,
        0,
        0,
        false,
        false,
    );
    assert_eq!(
        tree.cells[0].text_style.background,
        literofi_theme::ThemeColor::TRANSPARENT,
        "element-text must stay transparent instead of inheriting opaque * background-color"
    );
}
