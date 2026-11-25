#![cfg(windows)]

use literofi_core::config::Config;
use literofi_render::win32_d2d::{probe_icon_bitmap_size, run_render_self_test};
use literofi_theme::ThemeSheet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static BMP_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn minimal_2x2_bmp_bytes() -> Vec<u8> {
    // 14-byte BITMAPFILEHEADER + 40-byte BITMAPINFOHEADER + 16-byte 2x2 BGRA pixels = 70 bytes
    let mut b = Vec::with_capacity(70);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&70u32.to_le_bytes()); // file size
    b.extend_from_slice(&0u32.to_le_bytes()); // reserved
    b.extend_from_slice(&54u32.to_le_bytes()); // pixel offset
    b.extend_from_slice(&40u32.to_le_bytes()); // header size
    b.extend_from_slice(&2i32.to_le_bytes()); // width = 2
    b.extend_from_slice(&2i32.to_le_bytes()); // height = 2
    b.extend_from_slice(&1u16.to_le_bytes()); // planes
    b.extend_from_slice(&32u16.to_le_bytes()); // bpp = 32
    b.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    b.extend_from_slice(&16u32.to_le_bytes()); // image size
    b.extend_from_slice(&0i32.to_le_bytes());
    b.extend_from_slice(&0i32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..4 {
        b.extend_from_slice(&[255, 128, 64, 255]); // opaque BGRA pixel
    }
    b
}

#[test]
fn renders_real_windows_icons_and_respects_hide_icons() {
    let sheet = ThemeSheet::default_theme().expect("default theme");
    let cfg = Config::default();
    let report = run_render_self_test(&cfg, &[("active", &sheet)]).expect("render self-test");
    assert!(report.frames_painted >= 1);
    assert!(
        report.icons_loaded >= 3,
        "expected >= 3 real Windows icons decoded, got {}",
        report.icons_loaded
    );

    let hidden_cfg = Config {
        hide_icons: true,
        ..Config::default()
    };
    let hidden_report =
        run_render_self_test(&hidden_cfg, &[("active", &sheet)]).expect("hidden self-test");
    assert_eq!(hidden_report.icons_loaded, 0);
}

#[test]
fn decodes_direct_bmp_and_windows_shell_exe_to_d2d_bitmap() {
    let seq = BMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("literofi_ico_{}_{seq}.bmp", std::process::id()));
    fs::write(&path, minimal_2x2_bmp_bytes()).expect("write temp bmp");
    let _guard = TempFileGuard { path: path.clone() };

    let bmp_sz = probe_icon_bitmap_size(&path.to_string_lossy(), 32)
        .expect("probe bmp")
        .expect("decoded bmp bitmap");
    assert_eq!(bmp_sz, (2.0, 2.0));

    let shell_sz = probe_icon_bitmap_size("C:\\Windows\\explorer.exe", 32)
        .expect("probe explorer.exe")
        .expect("decoded explorer icon");
    assert!(shell_sz.0 >= 16.0 && shell_sz.1 >= 16.0);
}

#[test]
fn vertical_launchpad_grid_and_url_background_image_render_cleanly() {
    let seq = BMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let bmp_path =
        std::env::temp_dir().join(format!("literofi_bg_{}_{seq}.bmp", std::process::id()));
    fs::write(&bmp_path, minimal_2x2_bmp_bytes()).expect("write bg bmp");
    let _guard = TempFileGuard {
        path: bmp_path.clone(),
    };
    let bg_url = bmp_path.to_string_lossy().replace('\\', "/");

    let rasi = format!(
        r#"
@theme "/dev/null"
* {{
    background-color: #0d0f18;
    text-color: #a5b6cf;
}}
window {{
    width: 640px;
    border-radius: 8px;
    padding: 10px;
    background-image: url("{bg_url}", both);
}}
mainbox {{
    children: [ inputbar, listview ];
}}
listview {{
    columns: 4;
    lines: 3;
}}
element {{
    orientation: vertical;
    padding: 6px;
}}
element-icon {{
    size: 48px;
}}
"#
    );

    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_str(&rasi, &literofi_theme::ParseOptions::default())
        .expect("parse grid theme");
    let cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    let report = run_render_self_test(&cfg, &[("launchpad", &sheet)]).expect("render grid");
    assert!(report.frames_painted >= 1);
    assert!(report.icons_loaded >= 3);
    assert!(
        report.window_height >= 250,
        "expected vertical 3-row 48px-icon grid window height >= 250, got {}",
        report.window_height
    );
}

#[test]
fn tokyonight_theme_fits_all_rows_and_padding_within_window() {
    use literofi_render::layout::Rect;
    use literofi_render::state::{LauncherState, ListEntry};
    use literofi_render::tree::LaidOutTree;
    use literofi_theme::{DistanceMetrics, MonitorGeometry, ParseOptions};

    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme_path = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("themes")
        .join("tokyonight.rasi");

    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_file(&theme_path, &ParseOptions::default())
        .expect("parse tokyonight.rasi");

    let entries: Vec<ListEntry> = (0..12)
        .map(|i| ListEntry {
            text: format!("App Item {i}"),
            icon: Some("C:\\Windows\\System32\\notepad.exe".to_string()),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let metrics = DistanceMetrics {
        monitor_width: 1536.0,
        monitor_height: 864.0,
        char_height: 19.51,
        char_width: 8.08,
        dpi: 120.0,
    };
    let mut sc = sheet.clone();
    sc.process_conditionals(&MonitorGeometry {
        width: 1536,
        height: 864,
        monitor_id: 0,
    });

    let tree = LaidOutTree::build_with_options(
        &sc,
        &state,
        Rect::new(0.0, 0.0, 1536.0, 824.0),
        &metrics,
        &cfg.font,
        cfg.location,
        cfg.x_offset,
        cfg.y_offset,
        cfg.sidebar_mode,
        true,
        false,
    );

    assert_eq!(tree.cells.len(), 8, "all 8 lines must fit in viewport");
    for cell in &tree.cells {
        assert!(
            cell.rect.x + cell.rect.w <= tree.window_rect.w - 18.0 + 0.5,
            "cell right edge {} exceeds padded window width {}",
            cell.rect.x + cell.rect.w,
            tree.window_rect.w - 18.0
        );
    }
    let last = tree.cells.last().expect("last cell");
    assert!(
        last.rect.y + last.rect.h <= tree.window_rect.h - 18.0 + 0.5,
        "last cell bottom {} exceeds padded window height {}",
        last.rect.y + last.rect.h,
        tree.window_rect.h - 18.0
    );

    let report = run_render_self_test(&cfg, &[("tokyonight", &sheet)]).expect("render tokyonight");
    assert_eq!(report.frames_painted, 1);
    assert!(report.icons_loaded >= 1);
}

#[test]
fn resident_popup_prewarm_and_repaint_is_fast() {
    use literofi_render::state::{LauncherState, ListEntry};
    use literofi_render::win32_d2d::ResidentPopup;
    use literofi_theme::ParseOptions;
    use std::time::Instant;

    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme_path = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("themes")
        .join("tokyonight.rasi");

    let mut sheet = ThemeSheet::default_theme().expect("default");
    sheet
        .parse_file(&theme_path, &ParseOptions::default())
        .expect("parse tokyonight.rasi");

    let entries: Vec<ListEntry> = (0..10)
        .map(|i| ListEntry {
            text: format!("Fast Entry {i}"),
            icon: Some("C:\\Windows\\System32\\notepad.exe".to_string()),
            ..Default::default()
        })
        .collect();
    let cfg = Config {
        show_icons: true,
        ..Config::default()
    };
    let state = LauncherState::new(&cfg, entries);
    let mut popup = ResidentPopup::new(sheet, cfg, state).expect("create resident popup");
    assert!(!popup.is_visible());

    let t0 = Instant::now();
    for _ in 0..5 {
        let _ = popup.prewarm_frame().expect("prewarm repaint");
    }
    let avg_ms = t0.elapsed().as_secs_f64() * 1000.0 / 5.0;
    assert!(
        avg_ms < 50.0,
        "expected warm ResidentPopup repaint < 50ms, got {avg_ms:.2}ms"
    );
}
