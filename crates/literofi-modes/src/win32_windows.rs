//! Windows-native top-level HWND enumeration, foreground activation, and close helpers.
//! All Win32 `unsafe` calls for `window`/`windowcd` are isolated here.

/// Live top-level window descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredWindow {
    pub hwnd: isize,
    pub title: String,
    pub class_name: String,
    pub process_name: String,
    pub monitor_index: usize,
    pub is_active: bool,
    pub is_minimized: bool,
    pub is_on_current_monitor: bool,
}

/// Enumerates visible, non-cloaked top-level user windows on Windows.
pub fn enumerate_windows(exclude_hwnd: Option<isize>) -> Vec<DiscoveredWindow> {
    #[cfg(windows)]
    {
        win::enumerate_windows_impl(exclude_hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = exclude_hwnd;
        Vec::new()
    }
}

/// Restores if minimized and focuses the target window.
pub fn activate_window(hwnd: isize) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::activate_window_impl(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Err("window activation requires Windows".to_string())
    }
}

/// Requests closing a top-level window via `WM_CLOSE`.
pub fn close_window(hwnd: isize) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::close_window_impl(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Err("window closing requires Windows".to_string())
    }
}

#[cfg(windows)]
mod win {
    use super::DiscoveredWindow;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        PostMessageW, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE, WM_CLOSE,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    struct CollectState {
        exclude_hwnd: Option<isize>,
        fg_hwnd: isize,
        fg_monitor: isize,
        monitors: Vec<isize>,
        windows: Vec<DiscoveredWindow>,
    }

    pub fn enumerate_windows_impl(exclude_hwnd: Option<isize>) -> Vec<DiscoveredWindow> {
        // SAFETY: Standard Win32 EnumWindows enumeration collecting valid top-level window handles.
        unsafe {
            let fg = GetForegroundWindow();
            let fg_mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
            let mut state = CollectState {
                exclude_hwnd,
                fg_hwnd: fg.0 as isize,
                fg_monitor: fg_mon.0 as isize,
                monitors: Vec::new(),
                windows: Vec::new(),
            };
            let ptr = (&mut state as *mut CollectState) as isize;
            let _ = EnumWindows(Some(enum_cb), LPARAM(ptr));
            state.windows
        }
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut CollectState);
        let raw_hwnd = hwnd.0 as isize;
        if state.exclude_hwnd == Some(raw_hwnd) || !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        let title_len = GetWindowTextLengthW(hwnd);
        if title_len <= 0 {
            return BOOL(1);
        }

        let mut cloaked: u32 = 0;
        let dwm_ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        if dwm_ok.is_ok() && cloaked != 0 {
            return BOOL(1);
        }

        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let is_appwindow = (exstyle & WS_EX_APPWINDOW.0) != 0;
        let is_toolwindow = (exstyle & WS_EX_TOOLWINDOW.0) != 0;
        if is_toolwindow && !is_appwindow {
            return BOOL(1);
        }
        let owner = GetWindow(hwnd, GW_OWNER);
        if owner.as_ref().is_ok_and(|o| !o.0.is_null()) && !is_appwindow {
            return BOOL(1);
        }

        let mut title_buf = vec![0u16; (title_len as usize) + 2];
        let copied = GetWindowTextW(hwnd, &mut title_buf);
        if copied <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&title_buf[..copied as usize]);

        let mut class_buf = [0u16; 256];
        let clen = GetClassNameW(hwnd, &mut class_buf).max(0) as usize;
        let class_name = String::from_utf16_lossy(&class_buf[..clen]);

        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process_name = query_process_stem(pid).unwrap_or_else(|| class_name.clone());

        let mon: HMONITOR = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mon_id = mon.0 as isize;
        let monitor_index = match state.monitors.iter().position(|&m| m == mon_id) {
            Some(i) => i,
            None => {
                state.monitors.push(mon_id);
                state.monitors.len() - 1
            }
        };

        state.windows.push(DiscoveredWindow {
            hwnd: raw_hwnd,
            title,
            class_name,
            process_name,
            monitor_index,
            is_active: raw_hwnd == state.fg_hwnd,
            is_minimized: IsIconic(hwnd).as_bool(),
            is_on_current_monitor: mon_id == state.fg_monitor,
        });

        BOOL(1)
    }

    unsafe fn query_process_stem(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok || size == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        Path::new(&full)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    }

    pub fn activate_window_impl(hwnd_raw: isize) -> Result<(), String> {
        let hwnd = HWND(hwnd_raw as *mut _);
        // SAFETY: Restores minimized HWND if iconic and calls SetForegroundWindow.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if !SetForegroundWindow(hwnd).as_bool() {
                return Err(format!("SetForegroundWindow failed for hwnd {hwnd_raw}"));
            }
        }
        Ok(())
    }

    pub fn close_window_impl(hwnd_raw: isize) -> Result<(), String> {
        let hwnd = HWND(hwnd_raw as *mut _);
        // SAFETY: Posts WM_CLOSE to an existing top-level HWND.
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0))
                .map_err(|e| format!("PostMessageW(WM_CLOSE) failed: {e}"))
        }
    }
}
