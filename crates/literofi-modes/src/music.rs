//! Built-in `-show music` media controller mode powered by Windows SMTC
//! (`GlobalSystemMediaTransportControlsSessionManager`) with live track metadata
//! and album artwork extraction, styled after `niraj998/Rofi-Scripts` music widget.

use crate::active_palette::ActivePalette;
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    NoSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub state: PlaybackState,
    pub cover_path: Option<PathBuf>,
    pub position_secs: u64,
    pub duration_secs: u64,
}

impl Default for MediaSnapshot {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            state: PlaybackState::NoSession,
            cover_path: None,
            position_secs: 0,
            duration_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    Previous,
    TogglePlayPause,
    Next,
    Stop,
    Refresh,
}

/// Interactive media control mode (`-show music` / `-show media`).
pub struct MusicMode {
    display: String,
    cache_dir: PathBuf,
    snapshot: MediaSnapshot,
    selected_idx: usize,
    palette: Option<ActivePalette>,
    dry_run: bool,
    last_action: Option<MediaAction>,
}

impl MusicMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "music".to_string(),
            cache_dir: resolve_cache_dir(cfg),
            snapshot: MediaSnapshot::default(),
            selected_idx: 1, // Default focus on Play/Pause
            palette: None,
            dry_run: false,
            last_action: None,
        }
    }

    #[cfg(test)]
    pub fn with_mock(snapshot: MediaSnapshot) -> Self {
        Self {
            display: "music".to_string(),
            cache_dir: std::env::temp_dir().join("literofi_music_test"),
            snapshot,
            selected_idx: 1,
            palette: None,
            dry_run: true,
            last_action: None,
        }
    }

    fn refresh(&mut self, allow_empty: bool) {
        if self.dry_run {
            return;
        }
        // Run WinRT SMTC on a dedicated MTA worker thread so the Win32 STA UI thread
        // can never deadlock or freeze waiting on `IAsyncOperation::get()`.
        let cache = self.cache_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(query_system_media(&cache));
        });
        if let Ok(mut snap) = rx.recv_timeout(std::time::Duration::from_millis(2200)) {
            let had_track = !self.snapshot.title.is_empty() || !self.snapshot.artist.is_empty();
            let got_track = !snap.title.is_empty() || !snap.artist.is_empty();
            if !got_track && had_track && !allow_empty {
                // Keep existing track metadata/cover across transient SMTC gaps (e.g. play/pause
                // or track transitions where Spotify/Chrome briefly reports empty properties).
                if snap.state != PlaybackState::NoSession {
                    self.snapshot.state = snap.state;
                }
                return;
            }
            if snap.cover_path.is_none()
                && snap.title == self.snapshot.title
                && snap.artist == self.snapshot.artist
                && self
                    .snapshot
                    .cover_path
                    .as_ref()
                    .is_some_and(|p| p.is_file())
            {
                snap.cover_path = self.snapshot.cover_path.clone();
            }
            self.snapshot = snap;
        }
    }

    fn action_for_index(idx: usize) -> Option<MediaAction> {
        match idx {
            0 => Some(MediaAction::Previous),
            1 => Some(MediaAction::TogglePlayPause),
            2 => Some(MediaAction::Next),
            3 => Some(MediaAction::Stop),
            4 => Some(MediaAction::Refresh),
            _ => None,
        }
    }
}

impl RichMode for MusicMode {
    fn name(&self) -> &str {
        "music"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        if !self.dry_run {
            self.palette = ActivePalette::load_from_user_config();
        }
        self.refresh(true);
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        // Matches `niraj998/Rofi-Scripts` music widget: song title + artist inside the
        // prompt pill overlaid at the bottom-left of the album artwork.
        let icon = match self.snapshot.state {
            PlaybackState::Playing => "\u{f001}", // Nerd Font music note
            PlaybackState::Paused => "\u{f04c}",  // Nerd Font pause
            PlaybackState::Stopped => "\u{f04d}",
            PlaybackState::NoSession => "\u{f001}",
        };
        if self.snapshot.title.is_empty() && self.snapshot.artist.is_empty() {
            return Some(format!("{icon}  Play Something"));
        }
        let mut text = String::new();
        if !self.snapshot.title.is_empty() {
            text.push_str(&self.snapshot.title);
        }
        if !self.snapshot.artist.is_empty() {
            if !text.is_empty() {
                text.push_str("  -  ");
            }
            text.push_str(&self.snapshot.artist);
        }
        let truncated: String = text.chars().take(42).collect();
        Some(format!("{icon}  {truncated}"))
    }

    fn message(&self) -> Option<String> {
        None
    }

    fn theme_snippets(&self) -> Vec<String> {
        let mut snippets = Vec::new();
        // Cover-Art Auto-Accent (pocket-dots style): derive live palette directly from the
        // playing track's album art when available, falling back to the user's active theme.
        let mut cover_accent_applied = false;
        if let Some(cover) = &self.snapshot.cover_path {
            if cover.is_file() {
                if let Some(cp) = crate::wallpaper::extract_wallpaper_palette(cover, "cover") {
                    snippets.push(format!(
                        "* {{ bg: {bg}; fg: {fg}; button: {alt}; background-color: {bg}; text-color: {fg}; }}\n\
                         window {{ background-color: {bg}; border-color: {acc}; }}\n\
                         mainbox {{ background-color: {bg}; }}\n\
                         inputbar {{ background-color: {alt}; }}\n\
                         prompt {{ background-color: {bg_rgba}; text-color: {fg}; }}\n\
                         listview {{ background-color: {bg}; }}\n\
                         element {{ background-color: {alt}; text-color: {fg}; }}\n\
                         element selected.normal {{ background-color: {acc}; text-color: {bg}; }}",
                        bg = cp.bg,
                        alt = cp.bg_alt,
                        fg = cp.fg,
                        acc = cp.selected,
                        bg_rgba = cp.bg_rgba,
                    ));
                    cover_accent_applied = true;
                }
            }
        }
        if !cover_accent_applied {
            if let Some(pal) = &self.palette {
                snippets.push(pal.to_music_rasi());
            }
        }
        if let Some(cover) = &self.snapshot.cover_path {
            if cover.is_file() {
                let escaped = cover.to_string_lossy().replace('\\', "/");
                snippets.push(format!(
                    "inputbar {{ background-image: url(\"{escaped}\", width); }}"
                ));
            }
        }
        snippets
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(true)
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: false,
            new_selection: Some(self.selected_idx),
            keep_filter: false,
        }
    }

    fn entries(&self) -> Vec<ListEntry> {
        // Classic FontAwesome / Nerd Font PUA glyphs present in every Nerd Font:
        // Prev (f048), Play (f04b) / Pause (f04c), Next (f051), Stop (f04d), Refresh (f021).
        let toggle_glyph = match self.snapshot.state {
            PlaybackState::Playing => "\u{f04c}",
            _ => "\u{f04b}",
        };
        ["\u{f048}", toggle_glyph, "\u{f051}", "\u{f04d}", "\u{f021}"]
            .into_iter()
            .map(|glyph| ListEntry {
                text: glyph.to_string(),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::Accepted { index, .. } => {
                let idx = *index;
                self.selected_idx = idx;
                if let Some(action) = Self::action_for_index(idx) {
                    self.last_action = Some(action);
                    if !self.dry_run {
                        let prev_title = self.snapshot.title.clone();
                        let prev_artist = self.snapshot.artist.clone();
                        let prev_state = self.snapshot.state;
                        if action != MediaAction::Refresh {
                            let (tx, rx) = std::sync::mpsc::channel();
                            std::thread::spawn(move || {
                                let _ = tx.send(execute_media_action(action));
                            });
                            let _ = rx.recv_timeout(std::time::Duration::from_millis(800));
                        }
                        // Poll SMTC briefly until the player publishes the updated track/state
                        // (Spotify/YouTube Music often take 300-700ms after Next/Prev).
                        let attempts = match action {
                            MediaAction::Next | MediaAction::Previous => 10,
                            MediaAction::TogglePlayPause => 5,
                            MediaAction::Stop | MediaAction::Refresh => 1,
                        };
                        for attempt in 0..attempts {
                            if action != MediaAction::Refresh {
                                let delay_ms = if attempt == 0 { 140 } else { 110 };
                                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                            }
                            self.refresh(action == MediaAction::Stop);
                            let changed = match action {
                                MediaAction::Next | MediaAction::Previous => {
                                    (self.snapshot.title != prev_title
                                        || self.snapshot.artist != prev_artist)
                                        && (!self.snapshot.title.is_empty()
                                            || !self.snapshot.artist.is_empty())
                                }
                                MediaAction::TogglePlayPause => self.snapshot.state != prev_state,
                                _ => true,
                            };
                            if changed {
                                break;
                            }
                        }
                    } else if action == MediaAction::TogglePlayPause {
                        self.snapshot.state = match self.snapshot.state {
                            PlaybackState::Playing => PlaybackState::Paused,
                            _ => PlaybackState::Playing,
                        };
                    }
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

fn detect_image_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else {
        "png"
    }
}

fn write_cover_bytes(cache_dir: &Path, bytes: &[u8]) -> Option<PathBuf> {
    if bytes.len() < 64 {
        return None;
    }
    let _ = std::fs::create_dir_all(cache_dir);
    // Content-hashed filename ensures Direct2D's path-keyed image cache refreshes immediately
    // whenever album artwork changes, while reusing the same bitmap when unchanged.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let ext = detect_image_ext(bytes);
    let file_name = format!("music-cover-{hash:016x}.{ext}");
    let path = cache_dir.join(&file_name);
    if !path.is_file() {
        std::fs::write(&path, bytes).ok()?;
        if let Ok(rd) = std::fs::read_dir(cache_dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
                    if n.starts_with("music-cover-") && n != file_name {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
        }
    }
    Some(path)
}

#[cfg(windows)]
fn read_session_snapshot(
    session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    cache_dir: &Path,
) -> MediaSnapshot {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;
    use windows::Storage::Streams::DataReader;

    let state = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(|s| match s {
            Status::Playing => PlaybackState::Playing,
            Status::Paused => PlaybackState::Paused,
            Status::Stopped => PlaybackState::Stopped,
            _ => PlaybackState::NoSession,
        })
        .unwrap_or(PlaybackState::NoSession);

    let mut snap = MediaSnapshot {
        state,
        ..MediaSnapshot::default()
    };

    if let Ok(props_op) = session.TryGetMediaPropertiesAsync() {
        if let Ok(props) = props_op.get() {
            snap.title = props.Title().map(|s| s.to_string()).unwrap_or_default();
            snap.artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
            snap.album = props
                .AlbumTitle()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if let Ok(thumb_ref) = props.Thumbnail() {
                if let Ok(stream_op) = thumb_ref.OpenReadAsync() {
                    if let Ok(stream) = stream_op.get() {
                        let size = stream.Size().unwrap_or(0).min(8 * 1024 * 1024) as u32;
                        if size > 0 {
                            if let Ok(reader) = DataReader::CreateDataReader(&stream) {
                                if let Ok(load_op) = reader.LoadAsync(size) {
                                    if let Ok(loaded) = load_op.get() {
                                        let mut buf = vec![0u8; loaded as usize];
                                        if reader.ReadBytes(&mut buf).is_ok() {
                                            snap.cover_path = write_cover_bytes(cache_dir, &buf);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !snap.title.is_empty() {
                if let Some(hd) = cached_hd_album_art(cache_dir, &snap.title, &snap.artist) {
                    snap.cover_path = Some(hd);
                } else {
                    // Never block track switching on network iTunes lookup: warm HD art in background
                    let dir = cache_dir.to_path_buf();
                    let t = snap.title.clone();
                    let a = snap.artist.clone();
                    std::thread::spawn(move || {
                        let _ = fetch_hd_album_art(&dir, &t, &a);
                    });
                }
            }
        }
    }
    if let Ok(tl) = session.GetTimelineProperties() {
        let mut pos_ticks = tl.Position().map(|p| p.Duration).unwrap_or(0).max(0);
        let end_ticks = tl.EndTime().map(|e| e.Duration).unwrap_or(0).max(0);

        // Windows SMTC `Position` is frozen at `LastUpdatedTime`; extrapolate elapsed time while Playing
        // so periodic refreshes never snap the progress bar back to 0:00.
        if snap.state == PlaybackState::Playing {
            if let Ok(last_upd) = tl.LastUpdatedTime() {
                if last_upd.UniversalTime > 0 {
                    let now_ft = current_windows_filetime_100ns();
                    if now_ft > last_upd.UniversalTime {
                        let delta = now_ft - last_upd.UniversalTime;
                        // Guard against bogus timestamps (> 6 hours)
                        if delta < 6 * 3600 * 10_000_000 {
                            pos_ticks = pos_ticks.saturating_add(delta);
                        }
                    }
                }
            }
        }

        snap.position_secs = (pos_ticks / 10_000_000) as u64;
        snap.duration_secs = (end_ticks / 10_000_000) as u64;
        if snap.duration_secs > 0 && snap.position_secs > snap.duration_secs {
            snap.position_secs = snap.duration_secs;
        }
    }
    snap
}

#[cfg(windows)]
fn current_windows_filetime_100ns() -> i64 {
    use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    // SAFETY: Standard Win32 query returning current UTC FILETIME (100ns ticks since 1601).
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((i64::from(ft.dwHighDateTime as u32)) << 32) | i64::from(ft.dwLowDateTime)
}

/// Public entry point for the native Direct2D Music Player widget to query SMTC + HD cover art.
/// Runs inside a dedicated MTA (`COINIT_MULTITHREADED`) thread with a hard 1500ms timeout so
/// WinRT `IAsyncOperation::get()` can never deadlock on a UI/STA thread.
#[cfg(windows)]
pub fn query_system_media_snapshot(cache_dir: &Path) -> MediaSnapshot {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    let dir = cache_dir.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let snap = query_system_media(&dir);
        if !snap.title.is_empty() || !snap.artist.is_empty() {
            save_last_media_snapshot(&dir, &snap);
        }
        if com_ok {
            unsafe { CoUninitialize() };
        }
        let _ = tx.send(snap);
    });
    rx.recv_timeout(std::time::Duration::from_millis(1500))
        .unwrap_or_default()
}

#[cfg(not(windows))]
pub fn query_system_media_snapshot(_cache_dir: &Path) -> MediaSnapshot {
    MediaSnapshot::default()
}

/// Loads the last known non-empty media snapshot from disk in <1ms so `Alt+M` opens instantly.
pub fn load_last_media_snapshot(cache_dir: &Path) -> MediaSnapshot {
    let path = cache_dir.join("music-last.tsv");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return MediaSnapshot::default();
    };
    let parts: Vec<&str> = raw.splitn(7, '\t').collect();
    if parts.len() < 7 {
        return MediaSnapshot::default();
    }
    let state = match parts[3] {
        "playing" => PlaybackState::Playing,
        "paused" => PlaybackState::Paused,
        "stopped" => PlaybackState::Stopped,
        _ => PlaybackState::NoSession,
    };
    let cover_path = if parts[4].is_empty() {
        None
    } else {
        let p = PathBuf::from(parts[4]);
        p.is_file().then_some(p)
    };
    MediaSnapshot {
        title: parts[0].to_string(),
        artist: parts[1].to_string(),
        album: parts[2].to_string(),
        state,
        cover_path,
        position_secs: parts[5].parse().unwrap_or(0),
        duration_secs: parts[6].trim().parse().unwrap_or(0),
    }
}

fn save_last_media_snapshot(cache_dir: &Path, snap: &MediaSnapshot) {
    let _ = std::fs::create_dir_all(cache_dir);
    let state_s = match snap.state {
        PlaybackState::Playing => "playing",
        PlaybackState::Paused => "paused",
        PlaybackState::Stopped => "stopped",
        PlaybackState::NoSession => "none",
    };
    let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    let cover = snap
        .cover_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        clean(&snap.title),
        clean(&snap.artist),
        clean(&snap.album),
        state_s,
        clean(&cover),
        snap.position_secs,
        snap.duration_secs
    );
    let _ = std::fs::write(cache_dir.join("music-last.tsv"), line);
}

/// Public entry point for the native Direct2D Music Player widget to dispatch Play/Pause/Prev/Next/Stop.
#[cfg(windows)]
pub fn dispatch_media_action(action: MediaAction) {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let _ = execute_media_action(action);
    if com_ok {
        unsafe { CoUninitialize() };
    }
}

#[cfg(not(windows))]
pub fn dispatch_media_action(action: MediaAction) {
    let _ = execute_media_action(action);
}

/// Seeks the active Windows SMTC media session to `position_secs`.
#[cfg(windows)]
pub fn seek_system_media(position_secs: u64) -> bool {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let ok = (|| -> bool {
        let Ok(mgr_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() else {
            return false;
        };
        let Ok(mgr) = mgr_op.get() else {
            return false;
        };
        let Ok(session) = mgr.GetCurrentSession() else {
            return false;
        };
        let ticks = (position_secs as i64).saturating_mul(10_000_000);
        session
            .TryChangePlaybackPositionAsync(ticks)
            .and_then(|op| op.get())
            .unwrap_or(false)
    })();
    if com_ok {
        unsafe { CoUninitialize() };
    }
    ok
}

#[cfg(not(windows))]
pub fn seek_system_media(_position_secs: u64) -> bool {
    false
}

/// Returns the disk-cached 600x600 HD album art in <1ms if already downloaded.
#[cfg(windows)]
fn cached_hd_album_art(cache_dir: &Path, title: &str, artist: &str) -> Option<PathBuf> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let q_str = if artist.is_empty() {
        title.trim().to_string()
    } else {
        format!("{} {}", artist.trim(), title.trim())
    };
    if q_str.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    q_str.to_ascii_lowercase().hash(&mut hasher);
    let key = hasher.finish();
    let cached = cache_dir.join(format!("music-hd-{key:016x}.jpg"));
    if cached.is_file() && std::fs::metadata(&cached).is_ok_and(|m| m.len() > 2048) {
        Some(cached)
    } else {
        None
    }
}

/// Queries Windows Core Audio (WASAPI) default render endpoint master volume (`0.0..=1.0`, muted).
#[cfg(windows)]
pub fn get_master_volume() -> (f32, bool) {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let res = (|| -> Option<(f32, bool)> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let level = vol.GetMasterVolumeLevelScalar().ok()?.clamp(0.0, 1.0);
            let muted = vol.GetMute().ok()?.as_bool();
            Some((level, muted))
        })();
        if com_ok {
            CoUninitialize();
        }
        res.unwrap_or((0.5, false))
    }
}

#[cfg(not(windows))]
pub fn get_master_volume() -> (f32, bool) {
    (0.5, false)
}

/// Sets Windows Core Audio (WASAPI) default render endpoint master volume (`0.0..=1.0`).
#[cfg(windows)]
pub fn set_master_volume(level: f32) -> (f32, bool) {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    let clamped = level.clamp(0.0, 1.0);
    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let res = (|| -> Option<(f32, bool)> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let _ = vol.SetMute(false, std::ptr::null());
            let _ = vol.SetMasterVolumeLevelScalar(clamped, std::ptr::null());
            let out_lvl = vol.GetMasterVolumeLevelScalar().unwrap_or(clamped);
            let out_mute = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            Some((out_lvl, out_mute))
        })();
        if com_ok {
            CoUninitialize();
        }
        res.unwrap_or((clamped, false))
    }
}

#[cfg(not(windows))]
pub fn set_master_volume(level: f32) -> (f32, bool) {
    (level.clamp(0.0, 1.0), false)
}

/// Toggles Windows Core Audio (WASAPI) default render endpoint mute state.
#[cfg(windows)]
pub fn toggle_master_mute() -> (f32, bool) {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let res = (|| -> Option<(f32, bool)> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let cur_mute = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            let _ = vol.SetMute(!cur_mute, std::ptr::null());
            let out_lvl = vol.GetMasterVolumeLevelScalar().unwrap_or(0.5);
            Some((out_lvl, !cur_mute))
        })();
        if com_ok {
            CoUninitialize();
        }
        res.unwrap_or((0.5, false))
    }
}

#[cfg(not(windows))]
pub fn toggle_master_mute() -> (f32, bool) {
    (0.5, false)
}

#[cfg(windows)]
fn fetch_hd_album_art(cache_dir: &Path, title: &str, artist: &str) -> Option<PathBuf> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let query = format!("{} {}", artist.trim(), title.trim());
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    query.to_ascii_lowercase().hash(&mut hasher);
    let key = hasher.finish();
    let cached = cache_dir.join(format!("music-hd-{key:016x}.jpg"));
    if cached.is_file() && std::fs::metadata(&cached).is_ok_and(|m| m.len() > 2048) {
        return Some(cached);
    }
    let miss_marker = cache_dir.join(format!("music-hd-{key:016x}.miss"));
    if miss_marker.is_file() {
        return None;
    }
    let _ = std::fs::create_dir_all(cache_dir);

    let encoded = url_encode_query(query);
    let api_url = format!("https://itunes.apple.com/search?term={encoded}&media=music&limit=1");
    let out = std::process::Command::new("curl.exe")
        .args(["-sL", "--max-time", "2", &api_url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        let _ = std::fs::write(&miss_marker, b"");
        return None;
    }
    let body = String::from_utf8_lossy(&out.stdout);
    let needle = "\"artworkUrl100\":\"";
    let Some(pos) = body.find(needle) else {
        let _ = std::fs::write(&miss_marker, b"");
        return None;
    };
    let start = pos + needle.len();
    let Some(end) = body[start..].find('"') else {
        let _ = std::fs::write(&miss_marker, b"");
        return None;
    };
    let thumb_url = &body[start..start + end];
    let hd_url = thumb_url
        .replace("100x100bb", "600x600bb")
        .replace("100x100-", "600x600-");

    let img_out = std::process::Command::new("curl.exe")
        .args(["-sL", "--max-time", "2", &hd_url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !img_out.status.success() || img_out.stdout.len() < 4096 {
        let _ = std::fs::write(&miss_marker, b"");
        return None;
    }
    let tmp = cache_dir.join(format!("music-hd-{key:016x}.tmp"));
    std::fs::write(&tmp, &img_out.stdout).ok()?;
    let _ = std::fs::rename(&tmp, &cached);
    cached.is_file().then_some(cached)
}

#[cfg(windows)]
fn url_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(windows)]
fn query_system_media(cache_dir: &Path) -> MediaSnapshot {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let Ok(mgr_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() else {
        return MediaSnapshot::default();
    };
    let Ok(mgr) = mgr_op.get() else {
        return MediaSnapshot::default();
    };

    // Prefer current session if it has metadata; otherwise scan all active SMTC sessions
    // (fixes cases where an idle browser tab steals CurrentSession from Spotify/YouTube Music).
    let mut best = MediaSnapshot::default();
    if let Ok(cur) = mgr.GetCurrentSession() {
        let snap = read_session_snapshot(&cur, cache_dir);
        if !snap.title.is_empty() && snap.state == PlaybackState::Playing {
            return snap;
        }
        best = snap;
    }

    if let Ok(sessions) = mgr.GetSessions() {
        let count = sessions.Size().unwrap_or(0);
        for i in 0..count {
            if let Ok(sess) = sessions.GetAt(i) {
                let snap = read_session_snapshot(&sess, cache_dir);
                if snap.title.is_empty() && snap.artist.is_empty() {
                    continue;
                }
                if snap.state == PlaybackState::Playing {
                    return snap;
                }
                if best.title.is_empty() && best.artist.is_empty() {
                    best = snap;
                }
            }
        }
    }
    best
}

#[cfg(not(windows))]
fn query_system_media(_cache_dir: &Path) -> MediaSnapshot {
    MediaSnapshot::default()
}

#[cfg(windows)]
fn execute_media_action(action: MediaAction) -> bool {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYEVENTF_KEYUP, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP,
    };

    if let Ok(mgr_op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
        if let Ok(mgr) = mgr_op.get() {
            let mut candidates = Vec::new();
            if let Ok(cur) = mgr.GetCurrentSession() {
                candidates.push(cur);
            }
            if let Ok(sessions) = mgr.GetSessions() {
                let count = sessions.Size().unwrap_or(0);
                for i in 0..count {
                    if let Ok(sess) = sessions.GetAt(i) {
                        candidates.push(sess);
                    }
                }
            }
            for session in candidates {
                let res = match action {
                    MediaAction::Previous => session.TrySkipPreviousAsync().and_then(|op| op.get()),
                    MediaAction::TogglePlayPause => {
                        session.TryTogglePlayPauseAsync().and_then(|op| op.get())
                    }
                    MediaAction::Next => session.TrySkipNextAsync().and_then(|op| op.get()),
                    MediaAction::Stop => session.TryStopAsync().and_then(|op| op.get()),
                    MediaAction::Refresh => Ok(true),
                };
                if matches!(res, Ok(true)) {
                    return true;
                }
            }
        }
    }

    let vk = match action {
        MediaAction::Previous => VK_MEDIA_PREV_TRACK.0 as u8,
        MediaAction::TogglePlayPause => VK_MEDIA_PLAY_PAUSE.0 as u8,
        MediaAction::Next => VK_MEDIA_NEXT_TRACK.0 as u8,
        MediaAction::Stop => VK_MEDIA_STOP.0 as u8,
        MediaAction::Refresh => return true,
    };
    unsafe {
        keybd_event(vk, 0, Default::default(), 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
    true
}

#[cfg(not(windows))]
fn execute_media_action(_action: MediaAction) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_mode_formats_metadata_and_toggles_play_pause() {
        let snap = MediaSnapshot {
            title: "Starboy".to_string(),
            artist: "The Weeknd".to_string(),
            album: "Starboy".to_string(),
            state: PlaybackState::Playing,
            cover_path: None,
            ..MediaSnapshot::default()
        };
        let mut mode = MusicMode::with_mock(snap);
        mode.init().unwrap();
        assert_eq!(
            mode.prompt_override().as_deref(),
            Some("\u{f001}  Starboy  -  The Weeknd")
        );
        assert_eq!(mode.message(), None);
        let entries = mode.entries();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[1].text, "\u{f04c}");

        let step = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 1,
                    text: entries[1].text.clone(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step, ModeStep::Reload);
        assert_eq!(mode.entries()[1].text, "\u{f04b}");
    }

    #[test]
    fn detect_image_ext_handles_png_and_jpeg() {
        assert_eq!(detect_image_ext(b"\x89PNG\r\n\x1a\nxxxx"), "png");
        assert_eq!(detect_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), "jpg");
    }
}
