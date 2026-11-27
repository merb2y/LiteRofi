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
const HOTKEY_ID_EXTRA_BASE: i32 = 100;
const MOD_ALT_BIT: u32 = 0x0001;
const MOD_CONTROL_BIT: u32 = 0x0002;
const MOD_SHIFT_BIT: u32 = 0x0004;
const MOD_WIN_BIT: u32 = 0x0008;
const MOD_NOREPEAT_BIT: u32 = 0x4000;

/// Extra global hotkey binding parsed from `daemon-hotkeys` (`"Alt+G=games:game-launcher"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraHotkeyBinding {
    pub chord: String,
    pub mode: String,
    pub theme: Option<String>,
}

/// Parses `"Chord=mode"` or `"Chord=mode:theme"` into a validated [`ExtraHotkeyBinding`].
pub fn parse_extra_hotkey_binding(spec: &str) -> Result<ExtraHotkeyBinding, String> {
    let trimmed = spec.trim();
    let (chord_raw, target_raw) = trimmed.split_once('=').ok_or_else(|| {
        format!("invalid daemon-hotkeys entry `{trimmed}` (expected Chord=mode[:theme])")
    })?;
    let chord = chord_raw.trim();
    let _ = parse_daemon_hotkey(chord)?;
    let target = target_raw.trim();
    let (mode_raw, theme_opt) = match target.split_once(':') {
        Some((m, t)) => {
            let t = t.trim();
            (m.trim(), (!t.is_empty()).then(|| t.to_string()))
        }
        None => (target, None),
    };
    if mode_raw.is_empty() {
        return Err(format!("missing mode in daemon-hotkeys entry `{trimmed}`"));
    }
    Ok(ExtraHotkeyBinding {
        chord: chord.to_string(),
        mode: mode_raw.to_string(),
        theme: theme_opt,
    })
}

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
    use super::{
        collect_watched_theme_stamps, parse_extra_hotkey_binding, ExtraHotkeyBinding, FileStamp,
        HOTKEY_ID_EXTRA_BASE,
    };
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
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, RemoveClipboardFormatListener,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, ProcessMemoryPriority, ProcessPowerThrottling, SetPriorityClass,
        SetProcessInformation, MEMORY_PRIORITY_INFORMATION, MEMORY_PRIORITY_NORMAL,
        NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
        DestroyWindow, DispatchMessageW, FindWindowExW, GetCursorPos, GetMessageW, LoadIconW,
        LoadImageW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
        SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, HWND_MESSAGE,
        IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE, MF_SEPARATOR, MF_STRING, MSG,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE,
        WINDOW_STYLE, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP,
        WNDCLASSEXW,
    };

    const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
    const DETACHED_PROCESS_FLAG: u32 = 0x0000_0008;
    const WM_TRAY_ICON: u32 = WM_APP + 42;
    const WM_TRAY_CMD: u32 = WM_APP + 43;
    const WM_CLIPBOARDUPDATE_MSG: u32 = 0x031D;
    const TRAY_UID: u32 = 1;

    const CMD_OPEN_LAUNCHER: usize = 1001;
    const CMD_OPEN_STUDIO: usize = 1002;
    const CMD_OPEN_GAMES: usize = 1003;
    const CMD_OPEN_MUSIC: usize = 1004;
    const CMD_OPEN_POWER: usize = 1005;
    const CMD_OPEN_WALLPAPER: usize = 1010;
    const CMD_OPEN_CLIPBOARD: usize = 1011;
    const CMD_CLIPBOARD_UPDATE: usize = 1012;
    const CMD_OPEN_CONTROL: usize = 1013;
    const CMD_OPEN_SHORTCUTS: usize = 1014;
    const CMD_OPEN_CALC: usize = 1015;
    const CMD_OPEN_EMOJI: usize = 1016;
    const CMD_OPEN_SEARCH: usize = 1017;
    const CMD_OPEN_WALL_EFFECTS: usize = 1018;
    const CMD_OPEN_SNIPPETS: usize = 1019;
    const CMD_OPEN_TRANSLATE: usize = 1020;
    const CMD_OPEN_MENU: usize = 1021;
    const CMD_RELOAD_ALL: usize = 1006;
    const CMD_OPEN_SETTINGS: usize = 1007;
    const CMD_OPEN_THEMES_DIR: usize = 1008;
    const CMD_EXIT: usize = 1009;

    struct ClipboardListenerGuard {
        hwnd: HWND,
        registered: bool,
    }

    impl ClipboardListenerGuard {
        fn register(hwnd: HWND) -> Self {
            // SAFETY: Registers daemon's hidden top-level HWND for OS WM_CLIPBOARDUPDATE events.
            let registered = unsafe { AddClipboardFormatListener(hwnd).is_ok() };
            Self { hwnd, registered }
        }
    }

    impl Drop for ClipboardListenerGuard {
        fn drop(&mut self) {
            if self.registered {
                // SAFETY: Paired with successful AddClipboardFormatListener on the same HWND.
                unsafe {
                    let _ = RemoveClipboardFormatListener(self.hwnd);
                }
            }
        }
    }

    static TASKBAR_CREATED_MSG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    unsafe fn show_tray_context_menu(hwnd: HWND) {
        let Ok(hmenu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_LAUNCHER,
            w!("Open Launcher  (Alt+Space)"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_MENU,
            w!("System Menu  (Ctrl+Alt+Space)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_STUDIO, w!("Theme Studio"));
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_GAMES, w!("Games  (Alt+G)"));
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_MUSIC, w!("Music  (Alt+M)"));
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_POWER, w!("Power Menu  (Alt+P)"));
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_WALLPAPER,
            w!("Wallpapers  (Alt+W)"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_CLIPBOARD,
            w!("Clipboard  (Alt+V)"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_CONTROL,
            w!("Control Center  (Alt+D)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_CALC, w!("Calculator  (Alt+C)"));
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_EMOJI,
            w!("Emoji & Icons  (Alt+E)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_SEARCH, w!("Web Search  (Alt+S)"));
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_WALL_EFFECTS,
            w!("Wallpaper Effects  (Alt+Shift+W)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_SNIPPETS, w!("Snippets  (Alt+N)"));
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_TRANSLATE,
            w!("Translate  (Alt+T)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_SHORTCUTS, w!("Shortcuts Guide"));
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_RELOAD_ALL,
            w!("Reload Config & Icons"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_SETTINGS,
            w!("Settings (config.rasi)"),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_THEMES_DIR, w!("Themes Folder"));
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hmenu, MF_STRING, CMD_EXIT, w!("Exit LiteRofi"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
        if chosen.0 != 0 {
            let _ = PostMessageW(hwnd, WM_TRAY_CMD, WPARAM(chosen.0 as usize), LPARAM(0));
        }
    }

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
        if msg == WM_CLIPBOARDUPDATE_MSG {
            let _ = PostMessageW(hwnd, WM_TRAY_CMD, WPARAM(CMD_CLIPBOARD_UPDATE), LPARAM(0));
            return LRESULT(0);
        }
        if msg == WM_TRAY_ICON {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_LBUTTONUP {
                let _ = PostMessageW(hwnd, WM_TRAY_CMD, WPARAM(CMD_OPEN_LAUNCHER), LPARAM(0));
                return LRESULT(0);
            }
            if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                show_tray_context_menu(hwnd);
                return LRESULT(0);
            }
        }
        let tb = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
        if tb != 0 && msg == tb {
            let _ = PostMessageW(hwnd, WM_TRAY_CMD, WPARAM(0), LPARAM(1));
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub fn stop_existing_daemons() -> usize {
        let mut stopped = 0usize;
        let cls = w!("LiteRofiDaemonMsgOnly");
        for parent in [HWND::default(), HWND_MESSAGE] {
            for _ in 0..8 {
                // SAFETY: Searches top-level and message-only windows matching our daemon class and sends WM_CLOSE.
                let found = unsafe { FindWindowExW(parent, None, cls, PCWSTR::null()) };
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
        }
        stopped
    }

    fn create_message_window() -> Result<HWND, String> {
        // SAFETY: Registers a lightweight hidden top-level class so it receives both WM_HOTKEY and System Tray messages.
        unsafe {
            let tb = RegisterWindowMessageW(w!("TaskbarCreated"));
            TASKBAR_CREATED_MSG.store(tb, Ordering::Relaxed);
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
                None,
                None,
                hinst,
                None,
            )
            .map_err(|e| format!("CreateWindowExW(LiteRofiDaemon) failed: {e}"))
        }
    }

    struct TrayIconGuard {
        hwnd: HWND,
        hicon: HICON,
        owned_icon: bool,
    }

    impl TrayIconGuard {
        fn add(hwnd: HWND) -> Self {
            let (hicon, owned_icon) = load_tray_hicon();
            let guard = Self {
                hwnd,
                hicon,
                owned_icon,
            };
            guard.register();
            guard
        }

        fn register(&self) {
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_UID,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: WM_TRAY_ICON,
                hIcon: self.hicon,
                ..Default::default()
            };
            let tip: Vec<u16> = "LiteRofi — Alt+Space".encode_utf16().collect();
            let n = tip.len().min(nid.szTip.len() - 1);
            nid.szTip[..n].copy_from_slice(&tip[..n]);
            // SAFETY: Adds or re-adds our tray icon to the Windows notification area.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            }
        }
    }

    impl Drop for TrayIconGuard {
        fn drop(&mut self) {
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_UID,
                ..Default::default()
            };
            // SAFETY: Removes tray icon and frees owned HICON on daemon exit.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                if self.owned_icon && !self.hicon.0.is_null() {
                    let _ = DestroyIcon(self.hicon);
                }
            }
        }
    }

    fn load_tray_hicon() -> (HICON, bool) {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for rel in ["icons\\literofi.ico", "studio.ico"] {
                    let p = dir.join(rel);
                    if p.is_file() {
                        let wide: Vec<u16> = p
                            .to_string_lossy()
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();
                        // SAFETY: Loads local .ico file as HICON via LoadImageW.
                        if let Ok(h) = unsafe {
                            LoadImageW(
                                None,
                                PCWSTR(wide.as_ptr()),
                                IMAGE_ICON,
                                0,
                                0,
                                LR_LOADFROMFILE | LR_DEFAULTSIZE,
                            )
                        } {
                            if !h.0.is_null() {
                                return (HICON(h.0), true);
                            }
                        }
                    }
                }
            }
        }
        // SAFETY: Standard system fallback icon.
        let h = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() };
        (h, false)
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

    /// Ensures the hotkey daemon never stays at Task Scheduler's `BelowNormal` + low memory
    /// priority or Windows EcoQoS, preventing aggressive working-set page-out during long idle
    /// periods and keeping hotkey wakeup instantaneous (on par with DosyaBul).
    fn boost_daemon_responsiveness() {
        // SAFETY: Configures priority, memory priority, and power throttling on the current process.
        unsafe {
            let proc = GetCurrentProcess();
            let _ = SetPriorityClass(proc, NORMAL_PRIORITY_CLASS);
            let mem_prio = MEMORY_PRIORITY_INFORMATION {
                MemoryPriority: MEMORY_PRIORITY_NORMAL,
            };
            let _ = SetProcessInformation(
                proc,
                ProcessMemoryPriority,
                &mem_prio as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
            );
            let power_state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            let _ = SetProcessInformation(
                proc,
                ProcessPowerThrottling,
                &power_state as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
        }
    }

    pub fn run_daemon_impl(
        cli_cfg: &Config,
        cfg: &Config,
        base_sheet: &ThemeSheet,
    ) -> Result<(), String> {
        boost_daemon_responsiveness();
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
        let mut extra_hotkeys = register_extra_hotkeys(hwnd, &cfg.daemon_hotkeys);
        let tray_guard = TrayIconGuard::add(hwnd);
        let _clip_guard = ClipboardListenerGuard::register(hwnd);
        let _ = literofi_modes::clipboard::capture_current_clipboard();

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
        let mut active_extra_child: Option<(i32, std::process::Child)> = None;

        let mut msg = MSG::default();
        loop {
            // SAFETY: Standard Win32 GetMessageW loop serving both WM_HOTKEY and the resident popup.
            let has_msg = unsafe { GetMessageW(&mut msg, None, 0, 0).as_bool() };
            if !has_msg {
                break;
            }
            if msg.message == WM_HOTKEY && msg.wParam.0 == HOTKEY_ID as usize {
                close_active_extra_child(&mut active_extra_child);
                if popup.is_visible() && popup.is_foreground() {
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
                            if reloaded_cfg.daemon_hotkeys != runtime_cfg.daemon_hotkeys {
                                unregister_extra_hotkeys(hwnd, &extra_hotkeys);
                                extra_hotkeys =
                                    register_extra_hotkeys(hwnd, &reloaded_cfg.daemon_hotkeys);
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
                    if !popup.is_alive() || popup.prepare_and_show().is_err() {
                        // Self-heal: rebuild ResidentPopup if its HWND or D2D target was invalidated.
                        let mut fresh_state = LauncherState::new(&runtime_cfg, Vec::new());
                        if let Ok(ws) = crate::apply_mode_to_state(
                            modes[active_idx].as_ref(),
                            &runtime_cfg,
                            &current_base_sheet,
                            &mut fresh_state,
                            true,
                        ) {
                            fresh_state.modes =
                                modes.iter().map(|m| m.display_name().to_string()).collect();
                            fresh_state.active_mode = active_idx;
                            if let Ok(mut rebuilt) =
                                ResidentPopup::new(ws, runtime_cfg.clone(), fresh_state)
                            {
                                let _ = rebuilt.prepare_and_show();
                                popup = rebuilt;
                            }
                        }
                    }
                }
            } else if msg.message == WM_HOTKEY {
                let id = msg.wParam.0 as i32;
                if let Some((_, binding)) = extra_hotkeys.iter().find(|(hid, _)| *hid == id) {
                    if popup.is_visible() {
                        popup.hide();
                    }
                    let closed_same = close_active_extra_child(&mut active_extra_child) == Some(id);
                    if !closed_same {
                        if let Some(child) = spawn_extra_hotkey_target(binding) {
                            active_extra_child = Some((id, child));
                        }
                    }
                }
            } else if msg.message == WM_TRAY_CMD {
                if msg.lParam.0 == 1 {
                    tray_guard.register();
                } else {
                    match msg.wParam.0 {
                        CMD_OPEN_LAUNCHER => {
                            // SAFETY: Triggers the same Alt+Space popup flow on our owned daemon HWND.
                            unsafe {
                                let _ = PostMessageW(
                                    hwnd,
                                    WM_HOTKEY,
                                    WPARAM(HOTKEY_ID as usize),
                                    LPARAM(0),
                                );
                            }
                        }
                        CMD_OPEN_STUDIO => {
                            if let Ok(exe) = std::env::current_exe() {
                                if let Some(dir) = exe.parent() {
                                    let studio = dir.join("literofi-studio.exe");
                                    if studio.is_file() {
                                        let _ = std::process::Command::new(studio).spawn();
                                    }
                                }
                            }
                        }
                        CMD_OPEN_GAMES => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+G".into(),
                                mode: "games".into(),
                                theme: Some("game-launcher".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE, ch));
                            }
                        }
                        CMD_OPEN_MUSIC => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+M".into(),
                                mode: "music".into(),
                                theme: Some("music-player".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 1, ch));
                            }
                        }
                        CMD_OPEN_POWER => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+P".into(),
                                mode: "power".into(),
                                theme: Some("powermenu".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 2, ch));
                            }
                        }
                        CMD_OPEN_WALLPAPER => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+W".into(),
                                mode: "wallpaper".into(),
                                theme: Some("wallpaper-picker".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 3, ch));
                            }
                        }
                        CMD_OPEN_CLIPBOARD => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+V".into(),
                                mode: "clipboard".into(),
                                theme: Some("clipboard".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 4, ch));
                            }
                        }
                        CMD_OPEN_CONTROL => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+D".into(),
                                mode: "control".into(),
                                theme: Some("control-center".into()),
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 5, ch));
                            }
                        }
                        CMD_OPEN_SHORTCUTS => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+K".into(),
                                mode: "shortcuts".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 6, ch));
                            }
                        }
                        CMD_OPEN_CALC => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+C".into(),
                                mode: "calc".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 7, ch));
                            }
                        }
                        CMD_OPEN_EMOJI => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+E".into(),
                                mode: "emoji".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 8, ch));
                            }
                        }
                        CMD_OPEN_SEARCH => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+S".into(),
                                mode: "search".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 9, ch));
                            }
                        }
                        CMD_OPEN_WALL_EFFECTS => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+Shift+W".into(),
                                mode: "wallpaper-effects".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 10, ch));
                            }
                        }
                        CMD_OPEN_SNIPPETS => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+N".into(),
                                mode: "snippets".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 11, ch));
                            }
                        }
                        CMD_OPEN_TRANSLATE => {
                            let b = ExtraHotkeyBinding {
                                chord: "Alt+T".into(),
                                mode: "translate".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 12, ch));
                            }
                        }
                        CMD_OPEN_MENU => {
                            let b = ExtraHotkeyBinding {
                                chord: "Ctrl+Alt+Space".into(),
                                mode: "menu".into(),
                                theme: None,
                            };
                            close_active_extra_child(&mut active_extra_child);
                            if let Some(ch) = spawn_extra_hotkey_target(&b) {
                                active_extra_child = Some((HOTKEY_ID_EXTRA_BASE + 13, ch));
                            }
                        }
                        CMD_CLIPBOARD_UPDATE => {
                            let _ = literofi_modes::clipboard::capture_current_clipboard();
                        }
                        CMD_RELOAD_ALL => {
                            let flag = Arc::clone(&cache_refreshed);
                            std::thread::spawn(move || {
                                let _ = literofi_modes::win32_apps::refresh_windows_apps_cache();
                                flag.store(true, Ordering::Release);
                            });
                            force_theme_reload = true;
                            needs_mode_reload = true;
                        }
                        CMD_OPEN_SETTINGS => {
                            if let Ok(exe) = std::env::current_exe() {
                                let _ = std::process::Command::new(exe)
                                    .arg("--open-settings")
                                    .creation_flags(CREATE_NO_WINDOW_FLAG | DETACHED_PROCESS_FLAG)
                                    .spawn();
                            }
                        }
                        CMD_OPEN_THEMES_DIR => {
                            if let Ok(exe) = std::env::current_exe() {
                                let _ = std::process::Command::new(exe)
                                    .arg("--open-themes")
                                    .creation_flags(CREATE_NO_WINDOW_FLAG | DETACHED_PROCESS_FLAG)
                                    .spawn();
                            }
                        }
                        CMD_EXIT => {
                            // SAFETY: Exits the daemon message loop cleanly (Drop removes tray icon).
                            unsafe { PostQuitMessage(0) };
                        }
                        _ => {}
                    }
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
                        // Hide the TOPMOST launcher popup *before* spawning the target app on
                        // one-shot launcher modes so the UI vanishes in 0ms and never blocks
                        // foreground focus transfer to the launched application.
                        let pre_hide = matches!(
                            &other,
                            UiOutcome::Accepted { exit_code, .. } if *exit_code < 10
                        ) || matches!(&other, UiOutcome::AcceptedAlt { .. });
                        let is_instant_exit_mode = matches!(
                            modes[active_idx].name().to_ascii_lowercase().as_str(),
                            "drun" | "run" | "ssh" | "window" | "combi"
                        );
                        if pre_hide && is_instant_exit_mode {
                            popup.hide();
                        }
                        match modes[active_idx].handle_outcome(&other, &query) {
                            Ok(ModeStep::Exit(_)) | Err(_) => {
                                popup.hide();
                                // Refresh default mode + history order immediately while hidden so
                                // the next hotkey press opens in <5ms with zero mode reload lag.
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
                                    next_state.modes = modes
                                        .iter()
                                        .map(|m| m.display_name().to_string())
                                        .collect();
                                    next_state.active_mode = active_idx;
                                    popup.set_state_and_sheet(ws, next_state);
                                    needs_mode_reload = false;
                                } else {
                                    needs_mode_reload = true;
                                }
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

        close_active_extra_child(&mut active_extra_child);
        drop(popup);
        unregister_extra_hotkeys(hwnd, &extra_hotkeys);
        // SAFETY: Unregisters hotkey and destroys message HWND on exit.
        unsafe {
            let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
            let _ = DestroyWindow(hwnd);
        }
        Ok(())
    }

    fn close_active_extra_child(slot: &mut Option<(i32, std::process::Child)>) -> Option<i32> {
        let (prev_id, mut child) = slot.take()?;
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
            return Some(prev_id);
        }
        None
    }

    fn register_extra_hotkeys(hwnd: HWND, specs: &[String]) -> Vec<(i32, ExtraHotkeyBinding)> {
        let mut out = Vec::new();
        for (idx, spec) in specs.iter().enumerate() {
            let Ok(binding) = parse_extra_hotkey_binding(spec) else {
                continue;
            };
            let Ok((mods, vk)) = parse_daemon_hotkey(&binding.chord) else {
                continue;
            };
            let id = HOTKEY_ID_EXTRA_BASE + idx as i32;
            // SAFETY: Registers optional extra global shortcut on owned `hwnd`.
            if unsafe { RegisterHotKey(hwnd, id, HOT_KEY_MODIFIERS(mods), vk).is_ok() } {
                out.push((id, binding));
            }
        }
        out
    }

    fn unregister_extra_hotkeys(hwnd: HWND, registered: &[(i32, ExtraHotkeyBinding)]) {
        for (id, _) in registered {
            // SAFETY: Unregisters previously registered extra hotkey ID on owned `hwnd`.
            unsafe {
                let _ = UnregisterHotKey(hwnd, *id);
            }
        }
    }

    fn spawn_extra_hotkey_target(binding: &ExtraHotkeyBinding) -> Option<std::process::Child> {
        let exe = std::env::current_exe().ok()?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("-show").arg(&binding.mode);
        if let Some(theme) = &binding.theme {
            cmd.arg("-theme").arg(theme);
        }
        cmd.creation_flags(CREATE_NO_WINDOW_FLAG | DETACHED_PROCESS_FLAG);
        cmd.spawn().ok()
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
    fn parses_extra_hotkey_bindings() {
        let g = parse_extra_hotkey_binding("Alt+G=games:game-launcher").expect("games");
        assert_eq!(g.chord, "Alt+G");
        assert_eq!(g.mode, "games");
        assert_eq!(g.theme.as_deref(), Some("game-launcher"));

        let w = parse_extra_hotkey_binding("Alt+W=window").expect("window");
        assert_eq!(w.chord, "Alt+W");
        assert_eq!(w.mode, "window");
        assert_eq!(w.theme, None);

        assert!(parse_extra_hotkey_binding("Alt+G").is_err());
        assert!(parse_extra_hotkey_binding("Alt+UnknownKey=games").is_err());
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
