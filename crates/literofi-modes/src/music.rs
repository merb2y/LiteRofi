//! Built-in `-show music` media controller mode powered by Windows SMTC
//! (`GlobalSystemMediaTransportControlsSessionManager`) with live track metadata
//! and album artwork extraction, styled after `niraj998/Rofi-Scripts` music widget.

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
}

impl Default for MediaSnapshot {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            state: PlaybackState::NoSession,
            cover_path: None,
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
        if let Ok(mut snap) = rx.recv_timeout(std::time::Duration::from_millis(1400)) {
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
        let Some(cover) = &self.snapshot.cover_path else {
            return Vec::new();
        };
        if !cover.is_file() {
            return Vec::new();
        }
        let escaped = cover.to_string_lossy().replace('\\', "/");
        vec![format!(
            "inputbar {{ background-image: url(\"{escaped}\", width); }}"
        )]
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
        }
    }
    snap
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
