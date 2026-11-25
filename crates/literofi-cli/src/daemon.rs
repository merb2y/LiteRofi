//! Lightweight Win32 message-only `--daemon` global hotkey listener.
//! Initializes zero Direct2D/DirectWrite/theme resources while sleeping (`GetMessageW`)
//! and spawns ephemeral `literofi -show <mode>` child processes on hotkey triggers.

use literofi_core::config::Config;
use literofi_core::keyb::parse_chord;
use literofi_theme::ThemeSheet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: Option<u64>,
}

fn stamp_file(path: PathBuf) -> FileStamp {
    match std::fs::metadata(&path) {
        Ok(meta) => FileStamp {
            path,
            modified: meta.modified().ok(),
            len: Some(meta.len()),
        },
        Err(_) => FileStamp {
            path,
            modified: None,
            len: None,
        },
    }
}

/// Collects metadata stamps for `config.rasi` and every dynamic color/palette file
/// written by `LiteRofi Color Palette` (`wallust/colors-rofi.rasi`, `colors.rasi`,
/// and `launchers/*/shared/colors.rasi`) so the resident daemon hot-reloads whenever
/// either the active theme or the active color palette changes on disk.
fn collect_watched_theme_stamps(config_path: Option<&Path>) -> Vec<FileStamp> {
    let mut stamps = Vec::new();
    let Some(cfg_path) = config_path else {
        return stamps;
    };
    stamps.push(stamp_file(cfg_path.to_path_buf()));
    if let Some(root) = cfg_path.parent() {
        stamps.push(stamp_file(root.join("wallust").join("colors-rofi.rasi")));
        stamps.push(stamp_file(root.join("colors.rasi")));
        let launchers = root.join("launchers");
        if let Ok(rd) = std::fs::read_dir(&launchers) {
            let mut dirs: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for dir in dirs {
                let colors = dir.join("shared").join("colors.rasi");
                if colors.exists() {
                    stamps.push(stamp_file(colors));
                }
            }
        }
    }
    stamps
}

const HOTKEY_ID: i32 = 1;
const MOD_ALT_BIT: u32 = 0x0001;
const MOD_CONTROL_BIT: u32 = 0x0002;
const MOD_SHIFT_BIT: u32 = 0x0004;
const MOD_WIN_BIT: u32 = 0x0008;
const MOD_NOREPEAT_BIT: u32 = 0x4000;

/// Summary report emitted by `--daemon-self-test`.
#[derive(Debug, Clone)]
pub struct DaemonSelfTestReport {
    pub hotkey: String,
    pub requested_registered: bool,
    pub probe_registered: bool,
    pub idle_kb: u64,
}

/// Converts a chord specification (e.g. `Alt+Space`, `Control+Shift+R`) into Win32
/// `(HOT_KEY_MODIFIERS, virtual_key)` values.
pub fn parse_daemon_hotkey(spec: &str) -> Result<(u32, u32), String> {
    let chord = parse_chord(spec).ok_or_else(|| format!("invalid daemon hotkey `{spec}`"))?;
    let mut mods = MOD_NOREPEAT_BIT;
    if chord.modifiers.alt {
        mods |= MOD_ALT_BIT;
    }
    if chord.modifiers.control {
        mods |= MOD_CONTROL_BIT;
    }
    if chord.modifiers.shift {
        mods |= MOD_SHIFT_BIT;
    }
    if chord.modifiers.super_key {
        mods |= MOD_WIN_BIT;
    }
    let vk = map_hotkey_vk(&chord.key)
        .ok_or_else(|| format!("unsupported daemon hotkey key `{}` in `{spec}`", chord.key))?;
    Ok((mods, vk))
}

fn map_hotkey_vk(key: &str) -> Option<u32> {
    match key.to_ascii_lowercase().as_str() {
        "space" => Some(0x20),
        "return" | "enter" => Some(0x0D),
        "tab" => Some(0x09),
        "escape" | "esc" => Some(0x1B),
        other => {
            if other.len() == 1 {
                let b = other.as_bytes()[0];
                if b.is_ascii_alphabetic() {
                    return Some(u32::from(b.to_ascii_uppercase()));
                }
                if b.is_ascii_digit() {
                    return Some(u32::from(b));
                }
            }
            if let Some(num_str) = other.strip_prefix('f') {
                if let Ok(n) = num_str.parse::<u32>() {
                    if (1..=12).contains(&n) {
                        return Some(0x70 + (n - 1));
                    }
                }
            }
            None
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{collect_watched_theme_stamps, FileStamp};
    use super::{parse_daemon_hotkey, DaemonSelfTestReport, HOTKEY_ID};
    use literofi_core::config::Config;
    use literofi_modes::registry::{build_mode, collect_session_modes, ModeSpec};
    use literofi_modes::ModeStep;
    use literofi_render::state::{LauncherState, UiOutcome};
    use literofi_render::win32_d2d::ResidentPopup;
    use literofi_theme::ThemeSheet;
    use std::os::windows::process::CommandExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowExW,
        GetMessageW, PostMessageW, PostQuitMessage, RegisterClassExW, TranslateMessage,
        HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_HOTKEY, WNDCLASSEXW,
    };

    const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
    const DETACHED_PROCESS_FLAG: u32 = 0x0000_0008;

    unsafe extern "system" fn daemon_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLOSE {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub fn stop_existing_daemons() -> usize {
        let mut stopped = 0usize;
        let cls = w!("LiteRofiDaemonMsgOnly");
        for _ in 0..8 {
            // SAFETY: Searches message-only windows matching our daemon class and sends WM_CLOSE.
            let found = unsafe { FindWindowExW(HWND_MESSAGE, None, cls, PCWSTR::null()) };
            let Ok(hwnd) = found else {
                break;
            };
            if hwnd.0.is_null() {
                break;
            }
            // SAFETY: Valid HWND found by FindWindowExW.
            let _ = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
            stopped += 1;
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        stopped
    }

    fn create_message_window() -> Result<HWND, String> {
        // SAFETY: Registers a lightweight message-only class and creates HWND_MESSAGE window.
        unsafe {
            let hmod = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| format!("GetModuleHandleW failed: {e}"))?;
            let hinst = HINSTANCE(hmod.0);
            let cls_name = w!("LiteRofiDaemonMsgOnly");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(daemon_wnd_proc),
                hInstance: hinst,
                lpszClassName: cls_name,
                ..Default::default()
            };
            let _ = RegisterClassExW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                cls_name,
                w!("LiteRofiDaemon"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                hinst,
                None,
            )
            .map_err(|e| format!("CreateWindowExW(HWND_MESSAGE) failed: {e}"))
        }
    }

    pub fn current_working_set_kb() -> u64 {
        let mut pmc = PROCESS_MEMORY_COUNTERS::default();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: Queries current process working set into initialized PROCESS_MEMORY_COUNTERS.
        unsafe {
            if GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, size).is_ok() {
                (pmc.WorkingSetSize as u64) / 1024
            } else {
                0
            }
        }
    }

    pub fn run_daemon_self_test_impl(cfg: &Config) -> Result<DaemonSelfTestReport, String> {
        let (mods, vk) = parse_daemon_hotkey(&cfg.daemon_hotkey)?;
        let hwnd = create_message_window()?;

        // SAFETY: Registers and immediately unregisters hotkey(s) on our valid message HWND.
        let (requested_registered, probe_registered) = unsafe {
            let req_ok = RegisterHotKey(hwnd, HOTKEY_ID, HOT_KEY_MODIFIERS(mods), vk).is_ok();
            if req_ok {
                let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
            }
            let probe_ok = if req_ok {
                true
            } else {
                let (pmods, pvk) = parse_daemon_hotkey("Control+Alt+Shift+F12")?;
                let ok = RegisterHotKey(hwnd, HOTKEY_ID + 1, HOT_KEY_MODIFIERS(pmods), pvk).is_ok();
                if ok {
                    let _ = UnregisterHotKey(hwnd, HOTKEY_ID + 1);
                }
                ok
            };
            let _ = DestroyWindow(hwnd);
            (req_ok, probe_ok)
        };

        let idle_kb = current_working_set_kb();
        Ok(DaemonSelfTestReport {
            hotkey: cfg.daemon_hotkey.clone(),
            requested_registered,
            probe_registered,
            idle_kb,
        })
    }

    fn read_theme_stamps(cfg: &Config) -> Vec<FileStamp> {
        let path = crate::discover_user_config_file(cfg);
        collect_watched_theme_stamps(path.as_deref())
    }

    pub fn run_daemon_impl(
        cli_cfg: &Config,
        cfg: &Config,
        base_sheet: &ThemeSheet,
    ) -> Result<(), String> {
        let _ = stop_existing_daemons();
        let (mut mods, mut vk) = parse_daemon_hotkey(&cfg.daemon_hotkey)?;
        let hwnd = create_message_window()?;

        // SAFETY: Registers global hotkey on HWND_MESSAGE and enters resident GetMessageW loop.
        unsafe {
            RegisterHotKey(hwnd, HOTKEY_ID, HOT_KEY_MODIFIERS(mods), vk).map_err(|e| {
                let _ = DestroyWindow(hwnd);
                format!(
                    "RegisterHotKey failed for `{}` (try `--daemon-hotkey Control+Space`): {e}",
                    cfg.daemon_hotkey
                )
            })?;
        }

        let cache_refreshed = Arc::new(AtomicBool::new(false));
        let refreshed_flag = Arc::clone(&cache_refreshed);
        std::thread::spawn(move || {
            let _ = literofi_modes::win32_apps::refresh_windows_apps_cache();
            refreshed_flag.store(true, Ordering::Release);
        });

        let initial_cfg = cli_cfg.clone();
        let mut runtime_cfg = cfg.clone();
        let mut current_base_sheet = base_sheet.clone();
        let mut watched_stamps = read_theme_stamps(&initial_cfg);
        let mut force_theme_reload = false;

        let (mut modes, mut default_mode_idx) =
            collect_session_modes(&runtime_cfg, Some(&runtime_cfg.daemon_mode))?;
        let mut active_idx = default_mode_idx;
        modes[active_idx].init()?;

        let mut init_state = LauncherState::new(&runtime_cfg, Vec::new());
        let working_sheet = crate::apply_mode_to_state(
            modes[active_idx].as_ref(),
            &runtime_cfg,
            &current_base_sheet,
            &mut init_state,
            true,
        )?;
        init_state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
        init_state.active_mode = active_idx;

        let mut popup = ResidentPopup::new(working_sheet, runtime_cfg.clone(), init_state)?;
        let mut needs_mode_reload = false;

        let mut msg = MSG::default();
        loop {
            // SAFETY: Standard Win32 GetMessageW loop serving both WM_HOTKEY and the resident popup.
            let has_msg = unsafe { GetMessageW(&mut msg, None, 0, 0).as_bool() };
            if !has_msg {
                break;
            }
            if msg.message == WM_HOTKEY && msg.wParam.0 == HOTKEY_ID as usize {
                if popup.is_visible() {
                    popup.hide();
                } else {
                    let latest_stamps = read_theme_stamps(&initial_cfg);
                    let config_changed = force_theme_reload || latest_stamps != watched_stamps;
                    if config_changed {
                        let mut reloaded_cfg = initial_cfg.clone();
                        if let Ok(reloaded_sheet) = crate::load_theme_and_config(&mut reloaded_cfg)
                        {
                            if reloaded_cfg.daemon_hotkey != runtime_cfg.daemon_hotkey {
                                if let Ok((nm, nvk)) =
                                    parse_daemon_hotkey(&reloaded_cfg.daemon_hotkey)
                                {
                                    // SAFETY: Re-registers updated daemon hotkey on valid owned `hwnd`.
                                    unsafe {
                                        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
                                        if RegisterHotKey(
                                            hwnd,
                                            HOTKEY_ID,
                                            HOT_KEY_MODIFIERS(nm),
                                            nvk,
                                        )
                                        .is_ok()
                                        {
                                            mods = nm;
                                            vk = nvk;
                                        } else {
                                            let _ = RegisterHotKey(
                                                hwnd,
                                                HOTKEY_ID,
                                                HOT_KEY_MODIFIERS(mods),
                                                vk,
                                            );
                                        }
                                    }
                                }
                            }
                            if let Ok((new_modes, new_idx)) = collect_session_modes(
                                &reloaded_cfg,
                                Some(&reloaded_cfg.daemon_mode),
                            ) {
                                modes = new_modes;
                                default_mode_idx = new_idx;
                                active_idx = new_idx;
                                let _ = modes[active_idx].init();
                                runtime_cfg = reloaded_cfg;
                                current_base_sheet = reloaded_sheet;
                                let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                                if let Ok(ws) = crate::apply_mode_to_state(
                                    modes[active_idx].as_ref(),
                                    &runtime_cfg,
                                    &current_base_sheet,
                                    &mut next_state,
                                    true,
                                ) {
                                    next_state.modes = modes
                                        .iter()
                                        .map(|m| m.display_name().to_string())
                                        .collect();
                                    next_state.active_mode = active_idx;
                                    popup.update_session(ws, runtime_cfg.clone(), next_state);
                                }
                                needs_mode_reload = false;
                            }
                        }
                        watched_stamps = latest_stamps;
                        force_theme_reload = false;
                    } else {
                        let is_dynamic_mode = matches!(
                            modes[active_idx].name(),
                            "window" | "windowcd" | "filebrowser"
                        );
                        let refreshed = cache_refreshed.swap(false, Ordering::AcqRel);
                        if needs_mode_reload
                            || refreshed
                            || active_idx != default_mode_idx
                            || is_dynamic_mode
                        {
                            active_idx = default_mode_idx.min(modes.len().saturating_sub(1));
                            let _ = modes[active_idx].init();
                            let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                            if let Ok(ws) = crate::apply_mode_to_state(
                                modes[active_idx].as_ref(),
                                &runtime_cfg,
                                &current_base_sheet,
                                &mut next_state,
                                true,
                            ) {
                                next_state.modes =
                                    modes.iter().map(|m| m.display_name().to_string()).collect();
                                next_state.active_mode = active_idx;
                                popup.set_state_and_sheet(ws, next_state);
                            }
                            needs_mode_reload = false;
                        } else {
                            popup.reset_query();
                        }
                    }
                    let _ = popup.prepare_and_show();
                }
            }

            // SAFETY: Standard Win32 message dispatch.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if let Some(outcome) = popup.take_outcome() {
                match outcome {
                    UiOutcome::Cancelled => {
                        let _ = modes[active_idx].handle_outcome(&UiOutcome::Cancelled, "");
                        popup.hide();
                        if matches!(
                            modes[active_idx].name().to_ascii_lowercase().as_str(),
                            "theme" | "colors"
                        ) {
                            needs_mode_reload = true;
                            force_theme_reload = true;
                        }
                    }
                    UiOutcome::ModeSwitch(next_idx) => {
                        active_idx = next_idx % modes.len();
                        let _ = modes[active_idx].init();
                        let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                        if let Ok(ws) = crate::apply_mode_to_state(
                            modes[active_idx].as_ref(),
                            &runtime_cfg,
                            &current_base_sheet,
                            &mut next_state,
                            true,
                        ) {
                            next_state.modes =
                                modes.iter().map(|m| m.display_name().to_string()).collect();
                            next_state.active_mode = active_idx;
                            popup.set_state_and_sheet(ws, next_state);
                        }
                    }
                    other => {
                        let query = popup.current_query();
                        match modes[active_idx].handle_outcome(&other, &query) {
                            Ok(ModeStep::Exit(_)) | Err(_) => {
                                popup.hide();
                                needs_mode_reload = true;
                            }
                            Ok(ModeStep::PreviewTheme(theme_path)) => {
                                if let Ok(preview_sheet) =
                                    crate::load_preview_theme_sheet(&theme_path, &runtime_cfg)
                                {
                                    popup.preview_theme_sheet(
                                        preview_sheet,
                                        modes[active_idx].message(),
                                    );
                                }
                            }
                            Ok(ModeStep::ReloadTheme) => {
                                let mut tmp_cfg = runtime_cfg.clone();
                                if let Ok(reloaded) = crate::load_theme_and_config(&mut tmp_cfg) {
                                    current_base_sheet = reloaded.clone();
                                    popup
                                        .preview_theme_sheet(reloaded, modes[active_idx].message());
                                }
                            }
                            Ok(ModeStep::Reload) => {
                                let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                                if let Ok(ws) = crate::apply_mode_to_state(
                                    modes[active_idx].as_ref(),
                                    &runtime_cfg,
                                    &current_base_sheet,
                                    &mut next_state,
                                    true,
                                ) {
                                    next_state.modes = modes
                                        .iter()
                                        .map(|m| m.display_name().to_string())
                                        .collect();
                                    next_state.active_mode = active_idx;
                                    popup.set_state_and_sheet(ws, next_state);
                                }
                            }
                            Ok(ModeStep::SwitchModeIndex(idx)) => {
                                active_idx = idx % modes.len();
                                let _ = modes[active_idx].init();
                                let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                                if let Ok(ws) = crate::apply_mode_to_state(
                                    modes[active_idx].as_ref(),
                                    &runtime_cfg,
                                    &current_base_sheet,
                                    &mut next_state,
                                    true,
                                ) {
                                    next_state.modes = modes
                                        .iter()
                                        .map(|m| m.display_name().to_string())
                                        .collect();
                                    next_state.active_mode = active_idx;
                                    popup.set_state_and_sheet(ws, next_state);
                                }
                            }
                            Ok(ModeStep::SwitchMode(target)) => {
                                if let Some(pos) = modes
                                    .iter()
                                    .position(|m| m.name().eq_ignore_ascii_case(&target))
                                {
                                    active_idx = pos;
                                } else if let Some(spec) = ModeSpec::parse_one(&target) {
                                    if let Ok(m) = build_mode(&spec, &runtime_cfg) {
                                        modes.push(m);
                                        active_idx = modes.len() - 1;
                                    }
                                }
                                let _ = modes[active_idx].init();
                                let mut next_state = LauncherState::new(&runtime_cfg, Vec::new());
                                if let Ok(ws) = crate::apply_mode_to_state(
                                    modes[active_idx].as_ref(),
                                    &runtime_cfg,
                                    &current_base_sheet,
                                    &mut next_state,
                                    true,
                                ) {
                                    next_state.modes = modes
                                        .iter()
                                        .map(|m| m.display_name().to_string())
                                        .collect();
                                    next_state.active_mode = active_idx;
                                    popup.set_state_and_sheet(ws, next_state);
                                }
                            }
                        }
                    }
                }
            }
        }

        drop(popup);
        // SAFETY: Unregisters hotkey and destroys message HWND on exit.
        unsafe {
            let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
            let _ = DestroyWindow(hwnd);
        }
        Ok(())
    }

    pub fn start_daemon_detached_impl(cfg: &Config) -> Result<(), String> {
        let _ = parse_daemon_hotkey(&cfg.daemon_hotkey)?;
        let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--daemon");
        if cfg.daemon_hotkey_set {
            cmd.arg("--daemon-hotkey").arg(&cfg.daemon_hotkey);
        }
        if cfg.daemon_mode_set {
            cmd.arg("--daemon-mode").arg(&cfg.daemon_mode);
        }
        if let Some(theme) = &cfg.theme {
            cmd.arg("-theme").arg(theme);
        }
        if let Some(cfg_file) = &cfg.config_file {
            cmd.arg("-config").arg(cfg_file);
        }
        if cfg.no_config {
            cmd.arg("-no-config");
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW_FLAG | DETACHED_PROCESS_FLAG);
        cmd.spawn()
            .map_err(|e| format!("failed to start background daemon: {e}"))?;
        Ok(())
    }
}

pub fn run_daemon_self_test(cfg: &Config) -> Result<DaemonSelfTestReport, String> {
    #[cfg(windows)]
    {
        win::run_daemon_self_test_impl(cfg)
    }
    #[cfg(not(windows))]
    {
        let _ = parse_daemon_hotkey(&cfg.daemon_hotkey)?;
        Err("daemon self-test requires Windows".to_string())
    }
}

pub fn run_daemon(cli_cfg: &Config, cfg: &Config, base_sheet: &ThemeSheet) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::run_daemon_impl(cli_cfg, cfg, base_sheet)
    }
    #[cfg(not(windows))]
    {
        let _ = (cli_cfg, cfg, base_sheet);
        Err("Win32 daemon requires Windows".to_string())
    }
}

pub fn start_daemon_detached(cfg: &Config) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::start_daemon_detached_impl(cfg)
    }
    #[cfg(not(windows))]
    {
        let _ = cfg;
        Err("Win32 daemon requires Windows".to_string())
    }
}

pub fn stop_daemon() -> usize {
    #[cfg(windows)]
    {
        win::stop_existing_daemons()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_daemon_hotkeys() {
        assert_eq!(
            parse_daemon_hotkey("Alt+Space").expect("Alt+Space"),
            (MOD_NOREPEAT_BIT | MOD_ALT_BIT, 0x20)
        );
        assert_eq!(
            parse_daemon_hotkey("Control+Shift+r").expect("Ctrl+Shift+R"),
            (
                MOD_NOREPEAT_BIT | MOD_CONTROL_BIT | MOD_SHIFT_BIT,
                b'R' as u32
            )
        );
        assert_eq!(
            parse_daemon_hotkey("Super+F12").expect("Super+F12"),
            (MOD_NOREPEAT_BIT | MOD_WIN_BIT, 0x7B)
        );
        assert!(parse_daemon_hotkey("Alt+UnknownKeyXYZ").is_err());
    }

    #[test]
    fn watched_theme_stamps_detect_palette_changes_without_config_edit() {
        let dir =
            std::env::temp_dir().join(format!("literofi_daemon_stamps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wallust_dir = dir.join("wallust");
        let adi_dir = dir.join("launchers").join("type-1").join("shared");
        std::fs::create_dir_all(&wallust_dir).expect("create wallust dir");
        std::fs::create_dir_all(&adi_dir).expect("create adi shared dir");

        let cfg_path = dir.join("config.rasi");
        let wallust_path = wallust_dir.join("colors-rofi.rasi");
        let adi_path = adi_dir.join("colors.rasi");
        std::fs::write(&cfg_path, "@theme \"saint-rofi\"\n").expect("write config");
        std::fs::write(&wallust_path, "* { color1: #111111; }\n").expect("write wallust");
        std::fs::write(&adi_path, "* { background: #111111FF; }\n").expect("write adi");

        let before = collect_watched_theme_stamps(Some(&cfg_path));
        assert_eq!(before.len(), 4);

        // Simulate `LiteRofi Color Palette` updating only wallust/colors-rofi.rasi (config.rasi untouched)
        std::fs::write(&wallust_path, "* { color1: #cba6f7; color2: #89b4fa; }\n")
            .expect("update wallust");
        let after_wallust = collect_watched_theme_stamps(Some(&cfg_path));
        assert_ne!(
            before, after_wallust,
            "daemon must detect wallust/colors-rofi.rasi changes even when config.rasi is untouched"
        );

        // Simulate updating only launchers/type-1/shared/colors.rasi
        std::fs::write(
            &adi_path,
            "* { background: #1e1e2eFF; selected: #cba6f7FF; }\n",
        )
        .expect("update adi");
        let after_adi = collect_watched_theme_stamps(Some(&cfg_path));
        assert_ne!(
            after_wallust, after_adi,
            "daemon must detect launchers/*/shared/colors.rasi changes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
