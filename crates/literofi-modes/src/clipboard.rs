//! Native Windows Clipboard Manager mode (`-show clipboard`, `Alt+V`):
//! - Captures both copied text (`CF_UNICODETEXT`) and copied images/screenshots (`CF_DIB`)
//!   event-driven from the resident daemon (`AddClipboardFormatListener` -> `WM_CLIPBOARDUPDATE`).
//! - Persists history atomically to `%LOCALAPPDATA%\LiteRofi\cache\clipboard\history.tsv`
//!   and saves image previews as lossless `.bmp` files for instant Direct2D rendering.
//! - Supports instant search, Enter to copy back to clipboard, `Alt+A`/`Shift+Enter` to Pin/Unpin,
//!   and a one-click "Clear Unpinned History" action.

use crate::active_palette::ActivePalette;
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_UNPINNED_ENTRIES: usize = 80;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Image,
}

#[derive(Debug, Clone)]
pub struct ClipItem {
    pub kind: ClipKind,
    pub pinned: bool,
    pub timestamp: u64,
    pub hash: u64,
    pub meta: String,
    pub payload: String,
}

impl ClipItem {
    fn preview_label(&self) -> String {
        let pin = if self.pinned { "📌 " } else { "" };
        let time_str = format_hhmm(self.timestamp);
        match self.kind {
            ClipKind::Text => {
                let single_line = collapse_text_preview(&self.payload, 86);
                format!("{pin}󰅍  {single_line}   ·   {time_str}")
            }
            ClipKind::Image => {
                let dims = if self.meta.is_empty() {
                    "Image".to_string()
                } else {
                    format!("Image ({})", self.meta)
                };
                format!("{pin}󰋩  {dims}   ·   {time_str}")
            }
        }
    }
}

/// Interactive Windows clipboard history switcher with live image previews and pinning.
pub struct ClipboardMode {
    display: String,
    items: Vec<ClipItem>,
    dry_run: bool,
    palette: Option<ActivePalette>,
    status_hint: Option<String>,
}

impl ClipboardMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "clipboard".to_string(),
            items: Vec::new(),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            status_hint: None,
        }
    }

    fn reload_items(&mut self) {
        self.items = load_history(&clipboard_cache_dir());
    }
}

impl RichMode for ClipboardMode {
    fn name(&self) -> &str {
        "clipboard"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.status_hint = None;
        if !self.dry_run {
            let _ = capture_current_clipboard();
        }
        self.reload_items();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰅍  Clipboard".to_string())
    }

    fn message(&self) -> Option<String> {
        if let Some(hint) = &self.status_hint {
            return Some(hint.clone());
        }
        if self.items.is_empty() {
            return Some(
                "Clipboard history is empty. Copy text or a screenshot and press Alt+V."
                    .to_string(),
            );
        }
        let img_count = self
            .items
            .iter()
            .filter(|i| i.kind == ClipKind::Image)
            .count();
        let pin_count = self.items.iter().filter(|i| i.pinned).count();
        let n = self.items.len();
        let item_word = if n == 1 { "item" } else { "items" };
        let img_word = if img_count == 1 { "image" } else { "images" };
        Some(format!(
            "{n} {item_word} ({img_count} {img_word}, {pin_count} pinned)  ·  Enter: Copy  ·  Shift+Enter: Pin  ·  Esc: Close"
        ))
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: true,
            new_selection: None,
            keep_filter: false,
        }
    }

    fn theme_snippets(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .palette
            .as_ref()
            .map(|p| vec![p.to_clipboard_rasi()])
            .unwrap_or_default();
        let wcfg = literofi_core::widgets_config::WidgetsConfig::load_default();
        if let Some(ov) =
            literofi_core::widgets_config::WidgetsConfig::rasi_override_snippet(&wcfg.clipboard)
        {
            out.push(ov);
        }
        out
    }

    fn entries(&self) -> Vec<ListEntry> {
        let mut out: Vec<ListEntry> = self
            .items
            .iter()
            .map(|item| {
                let icon = match item.kind {
                    ClipKind::Image => Some(item.payload.clone()),
                    ClipKind::Text => None,
                };
                let search_meta = match item.kind {
                    ClipKind::Image => format!("image screenshot bmp {}", item.meta),
                    ClipKind::Text => item.payload.chars().take(240).collect(),
                };
                ListEntry {
                    text: item.preview_label(),
                    icon,
                    meta: Some(search_meta),
                    ..Default::default()
                }
            })
            .collect();

        if !self.items.is_empty() {
            out.push(ListEntry {
                text: "󰃢  Clear Unpinned History".to_string(),
                icon: None,
                meta: Some("clear delete reset history".to_string()),
                ..Default::default()
            });
        }
        out
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::Accepted { index, .. } => {
                // Last action row: Clear Unpinned History
                if !self.items.is_empty() && *index == self.items.len() {
                    if !self.dry_run {
                        let dir = clipboard_cache_dir();
                        clear_unpinned_history(&dir);
                    }
                    self.reload_items();
                    self.status_hint = Some("Cleared unpinned clipboard history.".to_string());
                    return Ok(ModeStep::Reload);
                }

                if let Some(item) = self.items.get(*index).cloned() {
                    if !self.dry_run {
                        match item.kind {
                            ClipKind::Text => write_text_to_clipboard(&item.payload)?,
                            ClipKind::Image => write_bmp_to_clipboard(Path::new(&item.payload))?,
                        }
                        let dir = clipboard_cache_dir();
                        promote_item_to_top(&dir, item.hash);
                    }
                    return Ok(ModeStep::Exit(0));
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                if let Some(item) = self.items.get(*index).cloned() {
                    if !self.dry_run {
                        let dir = clipboard_cache_dir();
                        let now_pinned = toggle_pin_item(&dir, item.hash);
                        self.status_hint = Some(if now_pinned {
                            "Pinned item to top.".to_string()
                        } else {
                            "Unpinned item.".to_string()
                        });
                    }
                    self.reload_items();
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

pub fn clipboard_cache_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return PathBuf::from(local)
                .join("LiteRofi")
                .join("cache")
                .join("clipboard");
        }
    }
    std::env::temp_dir().join("literofi-clipboard")
}

fn history_file(dir: &Path) -> PathBuf {
    dir.join("history.tsv")
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn format_hhmm(unix_secs: u64) -> String {
    // Convert UTC unix secs to approximate local HH:MM using Win32 local bias when available
    let offset_secs = local_utc_offset_secs();
    let local = (unix_secs as i64 + offset_secs).rem_euclid(86_400);
    let hours = local / 3600;
    let mins = (local % 3600) / 60;
    format!("{hours:02}:{mins:02}")
}

#[cfg(windows)]
fn local_utc_offset_secs() -> i64 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut tzi = TIME_ZONE_INFORMATION::default();
    // SAFETY: Valid TIME_ZONE_INFORMATION pointer passed to GetTimeZoneInformation.
    let state = unsafe { GetTimeZoneInformation(&mut tzi) };
    let bias_mins = match state {
        2 => i64::from(tzi.Bias) + i64::from(tzi.DaylightBias),
        1 => i64::from(tzi.Bias) + i64::from(tzi.StandardBias),
        _ => i64::from(tzi.Bias),
    };
    -bias_mins * 60
}

#[cfg(not(windows))]
fn local_utc_offset_secs() -> i64 {
    0
}

fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn escape_tsv(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape_tsv(s: &str) -> String {
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

fn collapse_text_preview(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    let mut count = 0usize;
    for ch in s.trim().chars() {
        if count >= max_chars {
            out.push('…');
            break;
        }
        if ch == '\n' {
            if !prev_space {
                out.push_str(" ↵ ");
                count += 3;
                prev_space = true;
            }
        } else if ch == '\r' {
            continue;
        } else if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                count += 1;
                prev_space = true;
            }
        } else {
            out.push(ch);
            count += 1;
            prev_space = false;
        }
    }
    out
}

pub fn load_history(dir: &Path) -> Vec<ClipItem> {
    let Ok(content) = std::fs::read_to_string(history_file(dir)) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(6, '\t').collect();
        if parts.len() != 6 {
            continue;
        }
        let kind = match parts[0] {
            "text" => ClipKind::Text,
            "image" => ClipKind::Image,
            _ => continue,
        };
        let pinned = parts[1] == "1";
        let timestamp = parts[2].parse::<u64>().unwrap_or(0);
        let hash = u64::from_str_radix(parts[3], 16).unwrap_or(0);
        let meta = unescape_tsv(parts[4]);
        let payload = unescape_tsv(parts[5]);
        if kind == ClipKind::Image && !Path::new(&payload).is_file() {
            continue;
        }
        items.push(ClipItem {
            kind,
            pinned,
            timestamp,
            hash,
            meta,
            payload,
        });
    }
    sort_items(&mut items);
    items
}

fn sort_items(items: &mut [ClipItem]) {
    items.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
}

fn save_history(dir: &Path, items: &[ClipItem]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let mut buf = String::new();
    for it in items {
        let kind = match it.kind {
            ClipKind::Text => "text",
            ClipKind::Image => "image",
        };
        let pin = if it.pinned { "1" } else { "0" };
        buf.push_str(kind);
        buf.push('\t');
        buf.push_str(pin);
        buf.push('\t');
        buf.push_str(&it.timestamp.to_string());
        buf.push('\t');
        buf.push_str(&format!("{:016x}", it.hash));
        buf.push('\t');
        buf.push_str(&escape_tsv(&it.meta));
        buf.push('\t');
        buf.push_str(&escape_tsv(&it.payload));
        buf.push('\n');
    }
    let path = history_file(dir);
    let tmp = path.with_extension(format!("tsv.tmp.{}", std::process::id()));
    std::fs::write(&tmp, buf).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path)
            .map_err(|e| format!("failed to replace {}: {e}", path.display()))?;
    }
    Ok(())
}

fn record_item_in_dir(dir: &Path, mut new_item: ClipItem) -> Result<(), String> {
    let mut items = load_history(dir);
    if let Some(pos) = items
        .iter()
        .position(|i| i.kind == new_item.kind && i.hash == new_item.hash)
    {
        let existing = items.remove(pos);
        new_item.pinned = existing.pinned;
        // Reuse existing image file if identical hash
        if new_item.kind == ClipKind::Image && existing.payload != new_item.payload {
            let _ = std::fs::remove_file(&new_item.payload);
            new_item.payload = existing.payload;
        }
    }
    items.insert(0, new_item);
    sort_items(&mut items);

    // Prune excess unpinned entries & delete orphaned image files
    let mut unpinned_seen = 0usize;
    let mut kept = Vec::with_capacity(items.len());
    for item in items {
        if item.pinned {
            kept.push(item);
        } else if unpinned_seen < MAX_UNPINNED_ENTRIES {
            unpinned_seen += 1;
            kept.push(item);
        } else if item.kind == ClipKind::Image {
            let _ = std::fs::remove_file(&item.payload);
        }
    }
    save_history(dir, &kept)
}

fn promote_item_to_top(dir: &Path, hash: u64) {
    let mut items = load_history(dir);
    if let Some(it) = items.iter_mut().find(|i| i.hash == hash) {
        it.timestamp = now_unix_secs();
        sort_items(&mut items);
        let _ = save_history(dir, &items);
    }
}

fn toggle_pin_item(dir: &Path, hash: u64) -> bool {
    let mut items = load_history(dir);
    let mut now_pinned = false;
    if let Some(it) = items.iter_mut().find(|i| i.hash == hash) {
        it.pinned = !it.pinned;
        now_pinned = it.pinned;
        sort_items(&mut items);
        let _ = save_history(dir, &items);
    }
    now_pinned
}

fn clear_unpinned_history(dir: &Path) {
    let items = load_history(dir);
    let mut kept = Vec::new();
    for item in items {
        if item.pinned {
            kept.push(item);
        } else if item.kind == ClipKind::Image {
            let _ = std::fs::remove_file(&item.payload);
        }
    }
    let _ = save_history(dir, &kept);
}

/// Wraps raw Windows `CF_DIB` bytes with a valid 14-byte `BITMAPFILEHEADER` so WIC/D2D
/// can load it directly as a standard `.bmp` file. Returns `(bmp_bytes, width, height)`.
pub fn wrap_dib_as_bmp(dib: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if dib.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?);
    if header_size < 40 || (header_size as usize) > dib.len() {
        return None;
    }
    let w = i32::from_le_bytes(dib[4..8].try_into().ok()?).unsigned_abs();
    let h = i32::from_le_bytes(dib[8..12].try_into().ok()?).unsigned_abs();
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return None;
    }
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    let clr_used = u32::from_le_bytes(dib[32..36].try_into().ok()?);

    let color_table_bytes = if clr_used > 0 {
        clr_used.saturating_mul(4)
    } else if bit_count <= 8 {
        (1u32 << bit_count).saturating_mul(4)
    } else if compression == 3 && header_size == 40 {
        // BI_BITFIELDS adds 3 DWORD color masks after a 40-byte BITMAPINFOHEADER
        12
    } else {
        0
    };

    let pixel_offset = 14u32
        .saturating_add(header_size)
        .saturating_add(color_table_bytes);
    let file_size = 14u32.saturating_add(u32::try_from(dib.len()).ok()?);

    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&pixel_offset.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some((bmp, w, h))
}

/// Captures the current Windows clipboard contents (text or DIB image) into persistent history.
/// Safe to call from both the resident daemon (`WM_CLIPBOARDUPDATE`) and `ClipboardMode::init`.
#[cfg(windows)]
pub fn capture_current_clipboard() -> Result<bool, String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_DIB: u32 = 8;
    const CF_UNICODETEXT: u32 = 13;

    // Retry briefly if another app currently holds the clipboard lock
    let mut opened = false;
    for _ in 0..4 {
        // SAFETY: Standard Win32 OpenClipboard with NULL owner for read-only inspection.
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !opened {
        return Ok(false);
    }

    let captured = unsafe {
        let has_text = IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok();
        let has_dib = IsClipboardFormatAvailable(CF_DIB).is_ok();

        let res = if has_text {
            GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
                let hg = HGLOBAL(h.0);
                let byte_len = GlobalSize(hg);
                if byte_len == 0 || byte_len > MAX_TEXT_BYTES * 2 {
                    return None;
                }
                let ptr = GlobalLock(hg) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                let max_u16 = byte_len / 2;
                let slice = std::slice::from_raw_parts(ptr, max_u16);
                let nul = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                let text = String::from_utf16_lossy(&slice[..nul]);
                let _ = GlobalUnlock(hg);
                if text.trim().is_empty() {
                    None
                } else {
                    Some(CapturedData::Text(text))
                }
            })
        } else if has_dib {
            GetClipboardData(CF_DIB).ok().and_then(|h| {
                let hg = HGLOBAL(h.0);
                let byte_len = GlobalSize(hg);
                if !(40..=MAX_IMAGE_BYTES).contains(&byte_len) {
                    return None;
                }
                let ptr = GlobalLock(hg) as *const u8;
                if ptr.is_null() {
                    return None;
                }
                let dib = std::slice::from_raw_parts(ptr, byte_len).to_vec();
                let _ = GlobalUnlock(hg);
                Some(CapturedData::Dib(dib))
            })
        } else {
            None
        };
        let _ = CloseClipboard();
        res
    };

    let Some(data) = captured else {
        return Ok(false);
    };

    let dir = clipboard_cache_dir();
    let ts = now_unix_secs();
    match data {
        CapturedData::Text(text) => {
            let trimmed = if text.len() > MAX_TEXT_BYTES {
                text.chars().take(MAX_TEXT_BYTES / 2).collect()
            } else {
                text
            };
            let hash = fnv1a64_bytes(trimmed.as_bytes());
            let meta = format!("{} chars", trimmed.chars().count());
            record_item_in_dir(
                &dir,
                ClipItem {
                    kind: ClipKind::Text,
                    pinned: false,
                    timestamp: ts,
                    hash,
                    meta,
                    payload: trimmed,
                },
            )?;
            Ok(true)
        }
        CapturedData::Dib(dib) => {
            let Some((bmp, w, h)) = wrap_dib_as_bmp(&dib) else {
                return Ok(false);
            };
            let hash = fnv1a64_bytes(&dib);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
            let img_path = dir.join(format!("img_{ts}_{hash:016x}.bmp"));
            std::fs::write(&img_path, &bmp)
                .map_err(|e| format!("failed to write {}: {e}", img_path.display()))?;
            record_item_in_dir(
                &dir,
                ClipItem {
                    kind: ClipKind::Image,
                    pinned: false,
                    timestamp: ts,
                    hash,
                    meta: format!("{w}×{h}"),
                    payload: img_path.to_string_lossy().into_owned(),
                },
            )?;
            Ok(true)
        }
    }
}

#[cfg(not(windows))]
pub fn capture_current_clipboard() -> Result<bool, String> {
    Ok(false)
}

#[cfg(windows)]
enum CapturedData {
    Text(String),
    Dib(Vec<u8>),
}

#[cfg(windows)]
pub fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_UNICODETEXT: u32 = 13;

    let wide: Vec<u16> = OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let bytes = wide.len() * 2;

    // SAFETY: Standard Win32 clipboard write sequence transferring HGLOBAL ownership to OS.
    unsafe {
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {e}"))?;
        let res = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;
            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes)
                .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
            let dst = GlobalLock(hmem) as *mut u16;
            if dst.is_null() {
                return Err("GlobalLock returned null".to_string());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(hmem);
            SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0))
                .map_err(|e| format!("SetClipboardData(CF_UNICODETEXT) failed: {e}"))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        res
    }
}

#[cfg(not(windows))]
pub fn write_text_to_clipboard(_text: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn write_bmp_to_clipboard(bmp_path: &Path) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_DIB: u32 = 8;

    let bytes = std::fs::read(bmp_path)
        .map_err(|e| format!("failed to read {}: {e}", bmp_path.display()))?;
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err("invalid cached BMP file".to_string());
    }
    let dib = &bytes[14..];

    // SAFETY: Standard Win32 CF_DIB clipboard write sequence transferring HGLOBAL ownership to OS.
    unsafe {
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {e}"))?;
        let res = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;
            let hmem = GlobalAlloc(GMEM_MOVEABLE, dib.len())
                .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
            let dst = GlobalLock(hmem) as *mut u8;
            if dst.is_null() {
                return Err("GlobalLock returned null".to_string());
            }
            std::ptr::copy_nonoverlapping(dib.as_ptr(), dst, dib.len());
            let _ = GlobalUnlock(hmem);
            SetClipboardData(CF_DIB, HANDLE(hmem.0))
                .map_err(|e| format!("SetClipboardData(CF_DIB) failed: {e}"))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        res
    }
}

#[cfg(not(windows))]
fn write_bmp_to_clipboard(_bmp_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_escape_roundtrip_preserves_tabs_newlines_and_backslashes() {
        let orig = "line1\nline2\tcol\\path\r\nend";
        let esc = escape_tsv(orig);
        assert!(!esc.contains('\n') && !esc.contains('\t'));
        assert_eq!(unescape_tsv(&esc), orig);
    }

    #[test]
    fn wrap_dib_as_bmp_builds_valid_header_and_dimensions() {
        let mut dib = vec![0u8; 40 + 16];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes()); // biSize
        dib[4..8].copy_from_slice(&2i32.to_le_bytes()); // biWidth
        dib[8..12].copy_from_slice(&2i32.to_le_bytes()); // biHeight
        dib[12..14].copy_from_slice(&1u16.to_le_bytes()); // biPlanes
        dib[14..16].copy_from_slice(&32u16.to_le_bytes()); // biBitCount
        let (bmp, w, h) = wrap_dib_as_bmp(&dib).expect("valid DIB");
        assert_eq!((w, h), (2, 2));
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(bmp.len(), 14 + dib.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
    }

    #[test]
    fn record_and_pin_orders_pinned_items_first() {
        let tmp = std::env::temp_dir().join(format!("literofi_clip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        record_item_in_dir(
            &tmp,
            ClipItem {
                kind: ClipKind::Text,
                pinned: false,
                timestamp: 100,
                hash: 1,
                meta: "3 chars".into(),
                payload: "one".into(),
            },
        )
        .unwrap();
        record_item_in_dir(
            &tmp,
            ClipItem {
                kind: ClipKind::Text,
                pinned: false,
                timestamp: 200,
                hash: 2,
                meta: "3 chars".into(),
                payload: "two".into(),
            },
        )
        .unwrap();
        assert!(toggle_pin_item(&tmp, 1));
        let loaded = load_history(&tmp);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].payload, "one");
        assert!(loaded[0].pinned);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
