use literofi_core::config::{Config, MatchingMethod, WindowLocation};
use literofi_theme::{
    highlight_flags, lookup_css_color, parse_hex_color, CursorType, Distance, DistanceMetrics,
    GradientDirection, HighlightColorStyle, ImageScale, ImageValue, LineStyle, MonitorGeometry,
    Orientation, Padding, ParseOptions, PropertyValue, ThemeColor, ThemeSheet, WidgetQuery,
};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn parses_all_official_rofi_themes_and_roundtrips_dump() {
    let root = repo_root();
    let themes_dir = root.join("reference").join("rofi").join("themes");
    assert!(themes_dir.is_dir(), "reference/rofi/themes missing");

    let opts = ParseOptions {
        search_dirs: vec![themes_dir.clone()],
        dmenu_mode: false,
    };

    let mut count = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(&themes_dir)
        .expect("read themes dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("rasi"))
        .collect();
    entries.sort();

    for path in entries {
        let mut sheet = ThemeSheet::default_theme().expect("default theme parses");
        if let Err(err) = sheet.parse_file(&path, &opts) {
            panic!("failed parsing official theme {}: {err}", path.display());
        }
        sheet.process_conditionals(&MonitorGeometry::default());
        let dumped = sheet.dump_theme();
        let mut reparsed = ThemeSheet::new();
        if let Err(err) = reparsed.parse_str(&dumped, &opts) {
            panic!(
                "failed reparsing dumped theme for {}: {err}\n{dumped}",
                path.display()
            );
        }
        count += 1;
    }

    assert!(count >= 30, "expected >=30 official themes, parsed {count}");
}

#[test]
fn parses_official_default_theme_and_default_configuration() {
    let root = repo_root();
    let doc_dir = root.join("reference").join("rofi").join("doc");
    let opts = ParseOptions::default();

    let mut sheet = ThemeSheet::new();
    sheet
        .parse_file(&doc_dir.join("default_theme.rasi"), &opts)
        .expect("default_theme.rasi parses");

    let q_elem = WidgetQuery::new("element");
    let q_sel = q_elem.with_state("selected.normal");
    let q_text = WidgetQuery::new("element-text").with_parent(&q_sel);

    let bg = sheet.get_color(&q_sel, "background-color", ThemeColor::BLACK);
    let fg = sheet.get_color(&q_text, "text-color", ThemeColor::BLACK);
    // solarized-style defaults: selected-normal-background = lightfg (88,104,117)
    assert!(approx(bg.r, 88.0 / 255.0));
    assert!(approx(bg.g, 104.0 / 255.0));
    assert!(approx(bg.b, 117.0 / 255.0));
    // element-text inherits text-color from element selected.normal -> lightbg (238,232,213)
    assert!(approx(fg.r, 238.0 / 255.0));
    assert!(approx(fg.g, 232.0 / 255.0));
    assert!(approx(fg.b, 213.0 / 255.0));

    let q_msg = WidgetQuery::new("message");
    let border = sheet.get_padding(&q_msg, "border", Padding::all(Distance::px(0.0)));
    assert_eq!(border.top.style, LineStyle::Dash);
    assert_eq!(border.right.style, LineStyle::Solid);
    assert!(approx(
        border
            .top
            .to_pixels(Orientation::Vertical, &DistanceMetrics::default()),
        2.0
    ));

    let mut cfg_sheet = ThemeSheet::default_theme().expect("default");
    cfg_sheet
        .parse_file(&doc_dir.join("default_configuration.rasi"), &opts)
        .expect("default_configuration.rasi parses");
    assert!(cfg_sheet.config_root.find_child("timeout").is_some());
    assert!(cfg_sheet.config_root.find_child("filebrowser").is_some());
}

#[test]
fn golden_theme_parser_vectors() {
    let opts = ParseOptions::default();

    // Empty, * {}, #test {}, comments (nested + unclosed at EOF), CRLF
    for src in [
        "",
        " * {}",
        " #test {}",
        "// c++\n/* nested /* inner */ outer */",
        "// last line",
        "/* unclosed at eof",
        "\r\n\n\r\n/*\r\n*/",
    ] {
        let mut s = ThemeSheet::new();
        s.parse_str(src, &opts).expect("valid snippet");
    }

    // Invalid root must fail gracefully
    let mut bad = ThemeSheet::new();
    assert!(bad.parse_str("Blaat", &opts).is_err());

    // Booleans + references + var(name, fallback) + cycle guard
    let mut s = ThemeSheet::new();
    s.parse_str(
        "* { test: true; test2: /* inline */ false; } * { a: @test; b: var(test2); c: var(missing, true); loop1: @loop2; loop2: @loop1; }",
        &opts,
    )
    .expect("refs");
    let q = WidgetQuery::new("blaat");
    assert!(s.get_boolean(&q, "test", false));
    assert!(!s.get_boolean(&q, "test2", true));
    assert!(s.get_boolean(&q, "a", false));
    assert!(!s.get_boolean(&q, "b", true));
    assert!(s.get_boolean(&q, "c", false));
    assert_eq!(s.find_property(&q, "loop1"), None);

    // Distances, calc(), units, padding 1..4 edges
    let mut dsheet = ThemeSheet::new();
    dsheet
        .parse_str(
            "* { sol: 1.3em solid; dash: 1.5em dash; p2: 10px 20px; p3: 1px 2px 3px; p4: 1px 2px 3px 4px; calcw: calc(((100% - 8em) / 7)); ops: calc((10px max 20px) + (2ch * 3) + (10px modulo 3px)); }",
            &opts,
        )
        .expect("distances");
    let metrics = DistanceMetrics::default(); // 1920x1080, char_height=16, char_width=8
    let sol = dsheet.get_distance(&q, "sol", Distance::px(0.0));
    assert_eq!(sol.style, LineStyle::Solid);
    assert!(approx(
        sol.to_pixels(Orientation::Horizontal, &metrics),
        1.3 * 16.0
    ));
    let dash = dsheet.get_distance(&q, "dash", Distance::px(0.0));
    assert_eq!(dash.style, LineStyle::Dash);
    let p4 = dsheet.get_padding(&q, "p4", Padding::all(Distance::px(0.0)));
    assert!(approx(
        p4.top.to_pixels(Orientation::Vertical, &metrics),
        1.0
    ));
    assert!(approx(
        p4.right.to_pixels(Orientation::Horizontal, &metrics),
        2.0
    ));
    assert!(approx(
        p4.bottom.to_pixels(Orientation::Vertical, &metrics),
        3.0
    ));
    assert!(approx(
        p4.left.to_pixels(Orientation::Horizontal, &metrics),
        4.0
    ));
    let calcw = dsheet.get_distance(&q, "calcw", Distance::px(0.0));
    assert!(approx(
        calcw.to_pixels(Orientation::Horizontal, &metrics),
        (1920.0 - 8.0 * 16.0) / 7.0
    ));
    let ops = dsheet.get_distance(&q, "ops", Distance::px(0.0));
    assert!(approx(
        ops.to_pixels(Orientation::Horizontal, &metrics),
        20.0 + (16.0 * 3.0) + 1.0
    ));

    // Colors: hex, argb, rgb/rgba, hsl, hwb, cmyk, CSS names, alpha slash
    assert!(approx(parse_hex_color("#fff").unwrap().r, 1.0));
    assert!(approx(
        parse_hex_color("#00ff0080").unwrap().a,
        128.0 / 255.0
    ));
    assert!(approx(
        parse_hex_color("argb:80ff0000").unwrap().a,
        128.0 / 255.0
    ));
    assert!(parse_hex_color("argb:1234567").is_err());
    assert!(lookup_css_color("RebeccaPurple").is_some());

    let mut csheet = ThemeSheet::new();
    csheet
        .parse_str(
            "* { c1: rgba(255, 0, 0, 50%); c2: rgb(0 255 0 / 0.25); c3: hsl(120deg, 100%, 50%); c4: hwb(0, 0%, 0%); c5: cmyk(0%, 100%, 100%, 0%); c6: black/80%; c7: transparent; }",
            &opts,
        )
        .expect("colors");
    let c1 = csheet.get_color(&q, "c1", ThemeColor::BLACK);
    assert!(approx(c1.r, 1.0) && approx(c1.a, 0.5));
    let c2 = csheet.get_color(&q, "c2", ThemeColor::BLACK);
    assert!(approx(c2.g, 1.0) && approx(c2.a, 0.25));
    let c3 = csheet.get_color(&q, "c3", ThemeColor::BLACK);
    assert!(approx(c3.g, 1.0));
    let c4 = csheet.get_color(&q, "c4", ThemeColor::BLACK);
    assert!(approx(c4.r, 1.0));
    let c5 = csheet.get_color(&q, "c5", ThemeColor::BLACK);
    assert!(approx(c5.r, 1.0) && approx(c5.g, 0.0));
    let c6 = csheet.get_color(&q, "c6", ThemeColor::BLACK);
    assert!(approx(c6.a, 0.8));
    assert_eq!(
        csheet.get_color(&q, "c7", ThemeColor::WHITE),
        ThemeColor::TRANSPARENT
    );
    assert!(ThemeSheet::new()
        .parse_str("* { bad: rgb(300, 0, 0); }", &opts)
        .is_err());

    // Highlights, positions, orientations, cursors, images, color->text-color alias, @media
    let mut misc = ThemeSheet::new();
    misc.parse_str(
        "window { location: west north; orientation: horizontal; cursor: pointer; background-image: linear-gradient(to bottom, black/10%, white/90%); } element-icon { background-image: url(\"iggy.jpg\", width); } entry { color: red; } element-text { highlight: bold italic #ff0000; children: [ icon, text ]; } @media (min-width: 1000px) { window { lines: 12; } } @media (enabled: false) { window { lines: 99; } }",
        &opts,
    )
    .expect("misc");
    misc.process_conditionals(&MonitorGeometry {
        width: 1920,
        height: 1080,
        monitor_id: 0,
    });

    let q_win = WidgetQuery::new("window");
    assert_eq!(
        misc.get_position(&q_win, "location", WindowLocation::Center),
        WindowLocation::NorthWest
    );
    assert_eq!(
        misc.get_orientation(&q_win, "orientation", Orientation::Vertical),
        Orientation::Horizontal
    );
    assert_eq!(
        misc.get_cursor(&q_win, "cursor", CursorType::Default),
        CursorType::Pointer
    );
    assert_eq!(misc.get_integer(&q_win, "lines", 0), 12);
    assert_eq!(
        misc.get_image(&q_win, "background-image"),
        Some(&ImageValue::LinearGradient {
            dir: GradientDirection::Bottom,
            colors: vec![
                ThemeColor::BLACK.with_alpha(0.1),
                ThemeColor::WHITE.with_alpha(0.9)
            ],
        })
    );
    assert_eq!(
        misc.get_image(&WidgetQuery::new("element-icon"), "background-image"),
        Some(&ImageValue::Url {
            url: "iggy.jpg".to_string(),
            scale: ImageScale::Width,
        })
    );
    assert!(approx(
        misc.get_color(&WidgetQuery::new("entry"), "text-color", ThemeColor::BLACK)
            .r,
        1.0
    ));
    let hl = misc.get_highlight(
        &WidgetQuery::new("element-text"),
        "highlight",
        HighlightColorStyle::default(),
    );
    assert_eq!(
        hl.style,
        highlight_flags::BOLD | highlight_flags::ITALIC | highlight_flags::COLOR
    );
    assert_eq!(
        misc.get_list_strings(&WidgetQuery::new("element-text"), "children"),
        Some(vec!["icon".to_string(), "text".to_string()])
    );
    assert_eq!(
        misc.find_property(&q_win, "nonexistent"),
        None::<&PropertyValue>
    );

    // configuration {} applies onto Config
    let mut cfg_sheet = ThemeSheet::new();
    cfg_sheet
        .parse_str(
            "configuration { matching: \"fuzzy\"; sort: true; kb-row-up: \"Up,k\"; location: 3; }",
            &opts,
        )
        .expect("cfg");
    let mut cfg = Config::default();
    cfg_sheet.apply_configuration(&mut cfg);
    assert_eq!(cfg.matching, MatchingMethod::Fuzzy);
    assert!(cfg.sort);
    assert_eq!(cfg.keybindings.get_by_name("kb-row-up"), Some("Up,k"));
    assert_eq!(cfg.location, WindowLocation::NorthEast);
}
