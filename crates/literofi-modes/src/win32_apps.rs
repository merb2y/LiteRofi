//! Windows-native application discovery (`FOLDERID_AppsFolder`, Start Menu `.lnk`/`.url`,
//! and `.desktop` parsing for XDG-style entries) with COM/Shell `unsafe` strictly isolated here.

use std::path::{Path, PathBuf};

/// How a discovered desktop application should be invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppLaunchTarget {
    ShellPath(PathBuf),
    AppsFolderAumid(String),
    Command(String),
}

/// Discovered desktop application metadata mapped to Rofi's `drun` model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredApp {
    pub id: String,
    pub name: String,
    pub generic: Option<String>,
    pub comment: Option<String>,
    pub exec: String,
    pub working_dir: Option<PathBuf>,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub icon: Option<String>,
    pub launch: AppLaunchTarget,
}

/// Discovers installed Windows applications from `FOLDERID_AppsFolder` and Start Menu folders.
/// Uses a fast validated on-disk cache (`%LOCALAPPDATA%\LiteRofi\cache\drun_apps_v1.tsv`) so
/// repeated launcher invocations start in milliseconds.
pub fn discover_windows_apps() -> Vec<DiscoveredApp> {
    #[cfg(windows)]
    {
        let path = drun_cache_path();
        if is_cache_fresh(&path) {
            if let Some(cached) = load_cache(&path) {
                if !cached.is_empty() {
                    return cached;
                }
            }
        }
        let fresh = discover_windows_apps_impl();
        let _ = save_cache(&path, &fresh);
        fresh
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Refreshes the on-disk Windows application discovery cache and returns the item count.
pub fn refresh_windows_apps_cache() -> usize {
    #[cfg(windows)]
    {
        let fresh = discover_windows_apps_impl();
        let count = fresh.len();
        let _ = save_cache(&drun_cache_path(), &fresh);
        count
    }
    #[cfg(not(windows))]
    {
        0
    }
}

const CACHE_HEADER: &str = "LITEROFI_DRUN_V2";
#[cfg(windows)]
const CACHE_TTL_SECS: u64 = 600;

#[cfg(windows)]
fn drun_cache_path() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return PathBuf::from(local)
                .join("LiteRofi")
                .join("cache")
                .join("drun_apps_v2.tsv");
        }
    }
    std::env::temp_dir().join("literofi_drun_cache_v2.tsv")
}

#[cfg(windows)]
fn is_cache_fresh(cache_path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(cache_path) else {
        return false;
    };
    let Ok(cache_mtime) = meta.modified() else {
        return false;
    };
    let Ok(age) = std::time::SystemTime::now().duration_since(cache_mtime) else {
        return false;
    };
    if age.as_secs() > CACHE_TTL_SECS {
        return false;
    }
    for root in win::start_menu_roots() {
        if let Ok(rmeta) = std::fs::metadata(&root) {
            if let Ok(rmtime) = rmeta.modified() {
                if rmtime > cache_mtime {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(windows)]
fn load_cache(cache_path: &Path) -> Option<Vec<DiscoveredApp>> {
    let content = std::fs::read_to_string(cache_path).ok()?;
    decode_cache(&content)
}

#[cfg(windows)]
fn save_cache(cache_path: &Path, apps: &[DiscoveredApp]) -> std::io::Result<()> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = cache_path.with_extension(format!("tsv.tmp.{}", std::process::id()));
    std::fs::write(&tmp, encode_cache(apps))?;
    if std::fs::rename(&tmp, cache_path).is_err() {
        std::fs::copy(&tmp, cache_path)?;
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(())
}

fn escape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(crate) fn encode_cache(apps: &[DiscoveredApp]) -> String {
    let mut out = String::from(CACHE_HEADER);
    out.push('\n');
    for app in apps {
        let (kind, target) = match &app.launch {
            AppLaunchTarget::AppsFolderAumid(a) => ("A", a.as_str()),
            AppLaunchTarget::ShellPath(p) => ("P", p.to_str().unwrap_or_default()),
            AppLaunchTarget::Command(c) => ("C", c.as_str()),
        };
        let cats = app
            .categories
            .iter()
            .map(|c| c.replace('\x1f', " "))
            .collect::<Vec<_>>()
            .join("\x1f");
        let cols = [
            escape_field(&app.id),
            escape_field(&app.name),
            escape_field(app.generic.as_deref().unwrap_or("")),
            escape_field(app.comment.as_deref().unwrap_or("")),
            escape_field(&app.exec),
            escape_field(cats.as_str()),
            escape_field(app.icon.as_deref().unwrap_or("")),
            kind.to_string(),
            escape_field(target),
        ];
        out.push_str(&cols.join("\t"));
        out.push('\n');
    }
    out
}

pub(crate) fn decode_cache(raw: &str) -> Option<Vec<DiscoveredApp>> {
    let mut lines = raw.lines();
    if lines.next()?.trim() != CACHE_HEADER {
        return None;
    }
    let mut apps = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 9 {
            return None;
        }
        let id = unescape_field(cols[0]);
        let name = unescape_field(cols[1]);
        let generic_s = unescape_field(cols[2]);
        let comment_s = unescape_field(cols[3]);
        let exec = unescape_field(cols[4]);
        let cats_s = unescape_field(cols[5]);
        let icon_s = unescape_field(cols[6]);
        let kind = cols[7];
        let target = unescape_field(cols[8]);
        let launch = match kind {
            "A" => AppLaunchTarget::AppsFolderAumid(target),
            "P" => AppLaunchTarget::ShellPath(PathBuf::from(target)),
            "C" => AppLaunchTarget::Command(target),
            _ => return None,
        };
        let categories = if cats_s.is_empty() {
            Vec::new()
        } else {
            cats_s.split('\x1f').map(str::to_string).collect()
        };
        apps.push(DiscoveredApp {
            id,
            name,
            generic: (!generic_s.is_empty()).then_some(generic_s),
            comment: (!comment_s.is_empty()).then_some(comment_s),
            exec,
            working_dir: None,
            categories,
            keywords: Vec::new(),
            icon: (!icon_s.is_empty()).then_some(icon_s),
            launch,
        });
    }
    Some(apps)
}

/// Launches a discovered application using Win32 `ShellExecuteW` or child command spawn.
pub fn launch_windows_app(target: &AppLaunchTarget) -> Result<(), String> {
    match target {
        AppLaunchTarget::Command(cmd) => {
            let argv = literofi_core::cli::split_shell_argv(cmd)?;
            crate::spawn_command_argv(&argv, None, None)
        }
        #[cfg(windows)]
        AppLaunchTarget::ShellPath(path) => shell_open(&path.to_string_lossy(), None),
        #[cfg(windows)]
        AppLaunchTarget::AppsFolderAumid(aumid) => {
            shell_open("explorer.exe", Some(&format!("shell:AppsFolder\\{aumid}")))
        }
        #[cfg(not(windows))]
        _ => Err("Windows Shell launch requires Windows".to_string()),
    }
}

/// Parses `.desktop` files inside `dir` recursively (`[Desktop Entry]` standard keys).
pub fn scan_desktop_dir(dir: &Path) -> Vec<DiscoveredApp> {
    let mut files = Vec::new();
    collect_files_with_ext(dir, &["desktop"], &mut files);
    let mut apps = Vec::new();
    for file in files {
        if let Ok(content) = std::fs::read_to_string(&file) {
            if let Some(app) = parse_desktop_file(&file, &content) {
                apps.push(app);
            }
        }
    }
    apps
}

fn parse_desktop_file(path: &Path, content: &str) -> Option<DiscoveredApp> {
    let mut in_main = false;
    let mut entry_type: Option<String> = None;
    let mut name: Option<String> = None;
    let mut generic: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut workdir: Option<PathBuf> = None;
    let mut categories = Vec::new();
    let mut keywords = Vec::new();
    let mut hidden = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let val = val.trim();
        match key.trim() {
            "Type" => entry_type = Some(val.to_string()),
            "Name" => name = Some(val.to_string()),
            "GenericName" => generic = (!val.is_empty()).then(|| val.to_string()),
            "Comment" => comment = (!val.is_empty()).then(|| val.to_string()),
            "Exec" => exec = (!val.is_empty()).then(|| strip_desktop_field_codes(val)),
            "Icon" => icon = (!val.is_empty()).then(|| val.to_string()),
            "Path" => {
                if !val.is_empty() {
                    workdir = Some(PathBuf::from(val));
                }
            }
            "Categories" => {
                categories = val
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "Keywords" => {
                keywords = val
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "NoDisplay" | "Hidden" if val.eq_ignore_ascii_case("true") => {
                hidden = true;
            }
            _ => {}
        }
    }

    if hidden || entry_type.as_deref().is_some_and(|t| t != "Application") {
        return None;
    }
    let name = name?;
    let exec = exec.unwrap_or_default();
    let id = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());

    Some(DiscoveredApp {
        id,
        name,
        generic,
        comment,
        exec: exec.clone(),
        working_dir: workdir,
        categories,
        keywords,
        icon,
        launch: AppLaunchTarget::Command(exec),
    })
}

fn strip_desktop_field_codes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if next == '%' {
                    out.push('%');
                    chars.next();
                    continue;
                }
                if matches!(
                    next,
                    'f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm'
                ) {
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_files_with_ext(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_files_with_ext(&p, exts, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
        {
            out.push(p);
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{collect_files_with_ext, AppLaunchTarget, DiscoveredApp};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use windows::core::{w, Interface, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellLinkW,
        SHGetKnownFolderItem, ShellExecuteW, ShellLink, KF_FLAG_DEFAULT,
        SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    struct ComGuard {
        should_uninit: bool,
    }

    impl ComGuard {
        fn init() -> Self {
            // SAFETY: Initializes thread apartment for Shell COM calls; only calls CoUninitialize if S_OK/S_FALSE.
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let should_uninit = hr == S_OK || hr == S_FALSE;
            if hr.is_err() && hr != RPC_E_CHANGED_MODE {
                return Self {
                    should_uninit: false,
                };
            }
            Self { should_uninit }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.should_uninit {
                // SAFETY: Matches a successful S_OK/S_FALSE CoInitializeEx on this thread.
                unsafe { CoUninitialize() };
            }
        }
    }

    pub fn shell_open(file: &str, params: Option<&str>) -> Result<(), String> {
        let wfile = HSTRING::from(file);
        let wparams = params.map(HSTRING::from);
        let params_ptr = wparams
            .as_ref()
            .map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr()));
        // SAFETY: Valid HSTRING/PCWSTR pointers passed to ShellExecuteW.
        let hinst = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &wfile,
                params_ptr,
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if (hinst.0 as isize) > 32 {
            Ok(())
        } else {
            Err(format!(
                "ShellExecuteW failed for `{file}` (code {})",
                hinst.0 as isize
            ))
        }
    }

    pub fn discover_windows_apps_impl() -> Vec<DiscoveredApp> {
        let _com = ComGuard::init();
        let mut by_key: HashMap<String, DiscoveredApp> = HashMap::new();

        // 1. Enumerate FOLDERID_AppsFolder (Win32 + UWP + Store + system apps)
        for app in enumerate_apps_folder() {
            by_key.entry(app.name.to_ascii_lowercase()).or_insert(app);
        }

        // 2. Enumerate Start Menu shortcuts (.lnk / .url) reusing one ShellLink COM instance
        // SAFETY: Allocates one ShellLink/IPersistFile pair shared across all `.lnk` files on this thread.
        let shared_link: Option<(IShellLinkW, IPersistFile)> = unsafe {
            CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .ok()
                .and_then(|l| l.cast::<IPersistFile>().ok().map(|pf| (l, pf)))
        };
        for root in start_menu_roots() {
            let mut shortcuts = Vec::new();
            collect_files_with_ext(&root, &["lnk", "url"], &mut shortcuts);
            for path in shortcuts {
                if let Some(app) = read_start_menu_shortcut(&root, &path, shared_link.as_ref()) {
                    let key = app.name.to_ascii_lowercase();
                    by_key
                        .entry(key)
                        .and_modify(|existing| {
                            // Prefer concrete Start Menu `.lnk`/`.url` `ShellPath` over `AppsFolderAumid`
                            // so shortcuts carrying CLI arguments (e.g. `LiteRofi Music.lnk` ->
                            // `-show music -theme music-player`) always launch with their full args.
                            if matches!(existing.launch, AppLaunchTarget::AppsFolderAumid(_))
                                && matches!(app.launch, AppLaunchTarget::ShellPath(_))
                            {
                                existing.launch = app.launch.clone();
                            }
                            if existing.comment.is_none() {
                                existing.comment = app.comment.clone();
                            }
                            if existing.exec.is_empty() && !app.exec.is_empty() {
                                existing.exec = app.exec.clone();
                            }
                            if existing.categories.is_empty() {
                                existing.categories = app.categories.clone();
                            }
                            if existing.icon.is_none()
                                || existing.icon.as_deref().is_some_and(|ic| {
                                    ic.contains("steam://") || ic.contains("com.epicgames")
                                }) && app.icon.is_some()
                            {
                                existing.icon = app.icon.clone();
                            }
                        })
                        .or_insert(app);
                }
            }
        }

        let mut list: Vec<DiscoveredApp> = by_key.into_values().collect();
        list.sort_by_key(|a| a.name.to_ascii_lowercase());
        list
    }

    fn is_non_app_target_or_name(name: &str, target: &str) -> bool {
        let n = name.trim().to_ascii_lowercase();
        if n.contains("uninstall") || n.contains("kaldır") || n.starts_with("unins") {
            return true;
        }
        let t = target.trim().trim_matches('"').to_ascii_lowercase();
        if t.starts_with("http://")
            || t.starts_with("https://")
            || t.starts_with("mailto:")
            || t.starts_with("file://")
        {
            return true;
        }
        const DOC_EXTS: [&str; 18] = [
            ".chm",
            ".hlp",
            ".pdf",
            ".txt",
            ".rtf",
            ".md",
            ".doc",
            ".docx",
            ".html",
            ".htm",
            ".log",
            ".ini",
            ".cfg",
            "\\unins000.exe",
            "\\uninstall.exe",
            "\\uninst.exe",
            "/unins000.exe",
            "/uninstall.exe",
        ];
        DOC_EXTS.iter().any(|ext| t.ends_with(ext))
    }

    fn enumerate_apps_folder() -> Vec<DiscoveredApp> {
        let mut out = Vec::new();
        // SAFETY: Standard Shell known-folder enumeration via SHGetKnownFolderItem + IEnumShellItems.
        unsafe {
            let Ok(folder): windows::core::Result<IShellItem> =
                SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
            else {
                return out;
            };
            let Ok(enumerator): windows::core::Result<IEnumShellItems> =
                folder.BindToHandler(None, &BHID_EnumItems)
            else {
                return out;
            };

            let mut batch: [Option<IShellItem>; 32] = Default::default();
            loop {
                let mut fetched = 0u32;
                let _ = enumerator.Next(&mut batch, Some(&mut fetched));
                if fetched == 0 {
                    break;
                }
                for slot in batch.iter_mut().take(fetched as usize) {
                    if let Some(item) = slot.take() {
                        let name = read_shell_display(&item, SIGDN_NORMALDISPLAY);
                        let parsing = read_shell_display(&item, SIGDN_DESKTOPABSOLUTEPARSING);
                        if let (Some(name), Some(aumid)) = (name, parsing) {
                            if !name.trim().is_empty()
                                && !aumid.trim().is_empty()
                                && !is_non_app_target_or_name(&name, &aumid)
                            {
                                out.push(DiscoveredApp {
                                    id: aumid.clone(),
                                    name,
                                    generic: Some("Application".to_string()),
                                    comment: None,
                                    exec: aumid.clone(),
                                    working_dir: None,
                                    categories: vec!["Application".to_string()],
                                    keywords: Vec::new(),
                                    icon: Some(format!("shell:AppsFolder\\{aumid}")),
                                    launch: AppLaunchTarget::AppsFolderAumid(aumid),
                                });
                            }
                        }
                    }
                }
            }
        }
        out
    }

    unsafe fn read_shell_display(
        item: &IShellItem,
        sigdn: windows::Win32::UI::Shell::SIGDN,
    ) -> Option<String> {
        let pwstr = item.GetDisplayName(sigdn).ok()?;
        let s = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.0 as *const _));
        s
    }

    pub(super) fn start_menu_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Ok(pd) = std::env::var("ProgramData") {
            let p = PathBuf::from(pd)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if p.is_dir() {
                roots.push(p);
            }
        }
        if let Ok(ad) = std::env::var("APPDATA") {
            let p = PathBuf::from(ad)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if p.is_dir() {
                roots.push(p);
            }
        }
        roots
    }

    fn read_url_file_fields(path: &Path) -> (String, String) {
        let mut url_target = String::new();
        let mut icon_file = String::new();
        if let Ok(raw_bytes) = std::fs::read(path) {
            let text = String::from_utf8_lossy(&raw_bytes);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.len() >= 4 && trimmed[..4].eq_ignore_ascii_case("url=") {
                    url_target = trimmed[4..].trim().to_string();
                } else if trimmed.len() >= 9 && trimmed[..9].eq_ignore_ascii_case("iconfile=") {
                    icon_file = trimmed[9..].trim().trim_matches('"').to_string();
                }
            }
        }
        (url_target, icon_file)
    }

    fn read_start_menu_shortcut(
        root: &Path,
        path: &Path,
        shared_link: Option<&(IShellLinkW, IPersistFile)>,
    ) -> Option<DiscoveredApp> {
        let stem = path.file_stem()?.to_string_lossy().into_owned();
        if is_non_app_target_or_name(&stem, "") {
            return None;
        }
        let mut categories = Vec::new();
        if let Ok(rel) = path.strip_prefix(root) {
            if let Some(parent) = rel.parent() {
                for comp in parent.components() {
                    let c = comp.as_os_str().to_string_lossy().into_owned();
                    if !c.is_empty() && c != "." {
                        categories.push(c);
                    }
                }
            }
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if ext.eq_ignore_ascii_case("url") {
            let (url_target, icon_file) = read_url_file_fields(path);
            if url_target.is_empty() || is_non_app_target_or_name(&stem, &url_target) {
                return None;
            }
            let icon = if !icon_file.is_empty() && Path::new(&icon_file).is_file() {
                Some(icon_file)
            } else {
                Some(path.to_string_lossy().into_owned())
            };
            return Some(DiscoveredApp {
                id: path.to_string_lossy().into_owned(),
                name: stem,
                generic: Some("Application".to_string()),
                comment: None,
                exec: url_target,
                working_dir: path.parent().map(Path::to_path_buf),
                categories,
                keywords: Vec::new(),
                icon,
                launch: AppLaunchTarget::ShellPath(path.to_path_buf()),
            });
        }

        let mut target_exec = path.to_string_lossy().into_owned();
        let mut comment: Option<String> = None;
        let mut workdir: Option<PathBuf> = None;
        let mut icon_override: Option<String> = None;

        // SAFETY: Reads `.lnk` properties via the reused ShellLink + IPersistFile with SLGP_RAWPATH.
        unsafe {
            if let Some((link, pf)) = shared_link {
                let wpath = HSTRING::from(path.as_os_str());
                if pf.Load(&wpath, STGM_READ).is_ok() {
                    let mut pbuf = [0u16; 1024];
                    if link.GetPath(&mut pbuf, std::ptr::null_mut(), 4).is_ok() {
                        let s = utf16_z(&pbuf);
                        if !s.is_empty() {
                            target_exec = s;
                        }
                    }
                    let mut dbuf = [0u16; 1024];
                    if link.GetDescription(&mut dbuf).is_ok() {
                        let d = utf16_z(&dbuf);
                        if !d.is_empty() {
                            comment = Some(d);
                        }
                    }
                    let mut wbuf = [0u16; 1024];
                    if link.GetWorkingDirectory(&mut wbuf).is_ok() {
                        let w = utf16_z(&wbuf);
                        if !w.is_empty() {
                            workdir = Some(PathBuf::from(w));
                        }
                    }
                    let mut ibuf = [0u16; 1024];
                    let mut icon_idx = 0i32;
                    if link.GetIconLocation(&mut ibuf, &mut icon_idx).is_ok() {
                        let iloc = utf16_z(&ibuf);
                        let iloc_trimmed = iloc.trim().trim_matches('"');
                        if !iloc_trimmed.is_empty()
                            && icon_idx == 0
                            && Path::new(iloc_trimmed).is_file()
                            && iloc_trimmed
                                .rsplit('.')
                                .next()
                                .is_some_and(|e| e.eq_ignore_ascii_case("ico"))
                        {
                            icon_override = Some(iloc_trimmed.to_string());
                        }
                    }
                }
            }
        }

        if target_exec.to_ascii_lowercase().ends_with(".url") {
            let url_path = Path::new(&target_exec);
            let (url_target, url_icon) = read_url_file_fields(url_path);
            if url_target.is_empty() || is_non_app_target_or_name(&stem, &url_target) {
                return None;
            }
            if icon_override.is_none() && !url_icon.is_empty() && Path::new(&url_icon).is_file() {
                icon_override = Some(url_icon);
            }
        }

        let t_clean = target_exec.trim().trim_matches('"');
        let t_lower = t_clean.to_ascii_lowercase();
        if t_clean.len() > 3
            && t_clean.as_bytes()[1] == b':'
            && (t_clean.as_bytes()[2] == b'\\' || t_clean.as_bytes()[2] == b'/')
            && t_lower.ends_with(".exe")
            && !t_lower.contains("\\windows\\installer\\")
            && !Path::new(t_clean).is_file()
        {
            return None;
        }

        if is_non_app_target_or_name(&stem, &target_exec) {
            return None;
        }

        Some(DiscoveredApp {
            id: path.to_string_lossy().into_owned(),
            name: stem,
            generic: categories.last().cloned(),
            comment,
            exec: target_exec,
            working_dir: workdir,
            categories,
            keywords: Vec::new(),
            icon: icon_override.or_else(|| Some(path.to_string_lossy().into_owned())),
            launch: AppLaunchTarget::ShellPath(path.to_path_buf()),
        })
    }

    fn utf16_z(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(windows)]
use win::{discover_windows_apps_impl, shell_open};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drun_cache_tsv_roundtrips_tabs_backslashes_and_targets() {
        let apps = vec![
            DiscoveredApp {
                id: "Valve.Steam\\App".to_string(),
                name: "Steam\tClient".to_string(),
                generic: Some("Application".to_string()),
                comment: Some("Play\ngames".to_string()),
                exec: "C:\\Program Files (x86)\\Steam\\steam.exe".to_string(),
                working_dir: None,
                categories: vec!["Games".to_string(), "Store".to_string()],
                keywords: Vec::new(),
                icon: Some("shell:AppsFolder\\Valve.Steam\\App".to_string()),
                launch: AppLaunchTarget::AppsFolderAumid("Valve.Steam\\App".to_string()),
            },
            DiscoveredApp {
                id: "C:\\Start\\Notepad.lnk".to_string(),
                name: "Notepad".to_string(),
                generic: None,
                comment: None,
                exec: "C:\\Windows\\notepad.exe".to_string(),
                working_dir: None,
                categories: Vec::new(),
                keywords: Vec::new(),
                icon: Some("C:\\Start\\Notepad.lnk".to_string()),
                launch: AppLaunchTarget::ShellPath(PathBuf::from("C:\\Start\\Notepad.lnk")),
            },
        ];

        let encoded = encode_cache(&apps);
        let decoded = decode_cache(&encoded).expect("decode TSV cache");
        assert_eq!(decoded, apps);
    }
}
