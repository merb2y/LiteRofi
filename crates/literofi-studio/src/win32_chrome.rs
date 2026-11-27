#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::mem;
    use std::sync::Mutex;
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::Foundation::{
        GetLastError, BOOL, ERROR_ALREADY_EXISTS, FALSE, HANDLE, HWND, LPARAM, RECT, TRUE, WPARAM,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, LoadImageW, SendMessageW, SetForegroundWindow, SetWindowPos,
        ShowWindow, IMAGE_FLAGS, IMAGE_ICON, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
        SWP_NOZORDER, SW_RESTORE, SW_SHOW, WM_SETICON,
    };

    #[allow(dead_code)]
    struct MutexHandle(usize);
    static INSTANCE_MUTEX: Mutex<Option<MutexHandle>> = Mutex::new(None);

    fn wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn try_acquire_single_instance(title: &str) -> bool {
        let name = wide_null(r"Local\LiteRofiStudioSingleInstance");
        // SAFETY: Valid null-terminated wide string; handle intentionally held for process lifetime.
        unsafe {
            let Ok(handle) = CreateMutexW(None, false, PCWSTR(name.as_ptr())) else {
                return true;
            };
            let already = GetLastError() == ERROR_ALREADY_EXISTS;
            if already {
                let w_title = wide_null(title);
                if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(w_title.as_ptr())) {
                    if !hwnd.0.is_null() {
                        if IsIconic(hwnd).as_bool() {
                            let _ = ShowWindow(hwnd, SW_RESTORE);
                        } else {
                            let _ = ShowWindow(hwnd, SW_SHOW);
                        }
                        let _ = SetForegroundWindow(hwnd);
                    }
                }
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                return false;
            }
            if let Ok(mut guard) = INSTANCE_MUTEX.lock() {
                *guard = Some(MutexHandle(handle.0 as usize));
            }
            true
        }
    }

    pub fn ensure_window_min_size_centered(
        hwnd_raw: isize,
        min_w: i32,
        min_h: i32,
        target_w: i32,
        target_h: i32,
    ) {
        if hwnd_raw == 0 {
            return;
        }
        // SAFETY: Validated non-zero HWND obtained from Win32 window enumeration.
        unsafe {
            let hwnd = HWND(hwnd_raw as *mut c_void);
            let mut rc = RECT::default();
            if GetWindowRect(hwnd, &mut rc).is_err() {
                return;
            }
            let mut dpi: u32 = 96;
            if let Ok(u32_lib) = LoadLibraryW(PCWSTR(wide_null("user32.dll").as_ptr())) {
                if let Some(p) =
                    GetProcAddress(u32_lib, PCSTR(c"GetDpiForWindow".as_ptr().cast::<u8>()))
                {
                    type FnGetDpi = unsafe extern "system" fn(HWND) -> u32;
                    let get_dpi: FnGetDpi = mem::transmute(p);
                    let d = get_dpi(hwnd);
                    if d >= 96 {
                        dpi = d;
                    }
                }
            }
            let scale = dpi as f64 / 96.0;
            let phys_min_w = (min_w as f64 * scale).round() as i32;
            let phys_min_h = (min_h as f64 * scale).round() as i32;
            let phys_w = (target_w as f64 * scale).round() as i32;
            let phys_h = (target_h as f64 * scale).round() as i32;
            let cur_w = rc.right - rc.left;
            let cur_h = rc.bottom - rc.top;
            if cur_w < phys_min_w || cur_h < phys_min_h {
                let sw = GetSystemMetrics(SM_CXSCREEN).max(phys_w);
                let sh = GetSystemMetrics(SM_CYSCREEN).max(phys_h);
                let x = ((sw - phys_w) / 2).max(0);
                let y = ((sh - phys_h) / 2).max(0);
                let _ = SetWindowPos(
                    hwnd,
                    HWND::default(),
                    x,
                    y,
                    phys_w,
                    phys_h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    pub fn apply_dwm_window_chrome(title: &str) -> Option<isize> {
        // SAFETY: Standard Win32 title/PID lookup, resource icon binding, and DWM attributes.
        unsafe {
            let mut hwnd =
                FindWindowW(PCWSTR::null(), PCWSTR(wide_null(title).as_ptr())).unwrap_or_default();
            if hwnd.0.is_null() {
                struct EnumCtx {
                    target_pid: u32,
                    found: HWND,
                }
                unsafe extern "system" fn enum_cb(w: HWND, lp: LPARAM) -> BOOL {
                    let ctx = &mut *(lp.0 as *mut EnumCtx);
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(w, Some(&mut pid));
                    if pid == ctx.target_pid && IsWindowVisible(w).as_bool() {
                        ctx.found = w;
                        return FALSE;
                    }
                    TRUE
                }
                let mut ctx = EnumCtx {
                    target_pid: GetCurrentProcessId(),
                    found: HWND::default(),
                };
                let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut EnumCtx as isize));
                hwnd = ctx.found;
            }
            if hwnd.0.is_null() {
                return None;
            }

            #[expect(
                clippy::manual_dangling_ptr,
                reason = "Win32 MAKEINTRESOURCEW(1) icon resource ID"
            )]
            let icon_res = PCWSTR(1usize as *const u16);

            if let Ok(hmod) = GetModuleHandleW(None) {
                if let Ok(h_sm) = LoadImageW(hmod, icon_res, IMAGE_ICON, 16, 16, IMAGE_FLAGS(0)) {
                    if !h_sm.is_invalid() {
                        let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(h_sm.0 as isize));
                    }
                }
                if let Ok(h_lg) = LoadImageW(hmod, icon_res, IMAGE_ICON, 32, 32, IMAGE_FLAGS(0)) {
                    if !h_lg.is_invalid() {
                        let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(1), LPARAM(h_lg.0 as isize));
                    }
                }
            }

            if let Ok(lib) = LoadLibraryW(PCWSTR(wide_null("dwmapi.dll").as_ptr())) {
                if let Some(proc_ptr) =
                    GetProcAddress(lib, PCSTR(c"DwmSetWindowAttribute".as_ptr().cast::<u8>()))
                {
                    type FnDwmSet = unsafe extern "system" fn(HWND, u32, *const c_void, u32) -> i32;
                    let set_attr: FnDwmSet = mem::transmute(proc_ptr);
                    let corner_pref: u32 = 1; // DWMWCP_DONOTROUND (sharp 0px corners)
                    let _ = set_attr(hwnd, 33, &corner_pref as *const u32 as *const c_void, 4);
                    let dark: u32 = 1; // DWMWA_USE_IMMERSIVE_DARK_MODE
                    let _ = set_attr(hwnd, 20, &dark as *const u32 as *const c_void, 4);
                    let caption_color: u32 = 0x0006_0708; // mango-minimal #080706 BGR
                    let _ = set_attr(hwnd, 35, &caption_color as *const u32 as *const c_void, 4);
                }
            }

            let raw = hwnd.0 as isize;
            ensure_window_min_size_centered(raw, 920, 580, 1220, 780);
            Some(raw)
        }
    }

    pub fn bring_to_foreground(hwnd_raw: isize) {
        if hwnd_raw == 0 {
            return;
        }
        // SAFETY: Validated non-zero HWND owned by current process.
        unsafe {
            let _: HANDLE = HANDLE(hwnd_raw as *mut c_void);
            let hwnd = HWND(hwnd_raw as *mut c_void);
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn try_acquire_single_instance(_title: &str) -> bool {
        true
    }
    pub fn ensure_window_min_size_centered(
        _hwnd: isize,
        _min_w: i32,
        _min_h: i32,
        _target_w: i32,
        _target_h: i32,
    ) {
    }
    pub fn apply_dwm_window_chrome(_title: &str) -> Option<isize> {
        None
    }
    pub fn bring_to_foreground(_hwnd: isize) {}
}

pub use imp::{
    apply_dwm_window_chrome, bring_to_foreground, ensure_window_min_size_centered,
    try_acquire_single_instance,
};
