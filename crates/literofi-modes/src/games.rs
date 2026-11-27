//! Built-in `-show games` Game Launcher mode inspired by `ntcarlson/dotfiles`
//! (`games.rasi` + `game-splash-menu.rasi`).
//! Step 1: Horizontal Steam/Epic poster grid with live search.
//! Step 2: Selecting a game opens its full-width hero banner splash menu
//!         (`Play`, `Open in library`, `Achievements`, `News`, `Back`).

use crate::active_palette::ActivePalette;
use crate::win32_apps::{launch_windows_app, AppLaunchTarget};
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredGame {
    pub name: String,
    pub appid: Option<String>,
    pub launch_uri: String,
    pub cover_path: Option<PathBuf>,
    pub hero_path: Option<PathBuf>,
    pub logo_path: Option<PathBuf>,
    pub source: &'static str,
}

/// Interactive Windows Game Launcher mode (`-show games` / `-show game`).
pub struct GamesMode {
    display: String,
    games: Vec<DiscoveredGame>,
    selected_game: Option<usize>,
    palette: Option<ActivePalette>,
    override_games: Option<Vec<DiscoveredGame>>,
    dry_run: bool,
    launched: Option<String>,
}

impl GamesMode {
    pub fn new(_cfg: &Config) -> Self {
        Self {
            display: "games".to_string(),
            games: Vec::new(),
            selected_game: None,
            palette: None,
            override_games: None,
            dry_run: false,
            launched: None,
        }
    }

    #[cfg(test)]
    pub fn with_mock(games: Vec<DiscoveredGame>) -> Self {
        Self {
            display: "games".to_string(),
            games: Vec::new(),
            selected_game: None,
            palette: None,
            override_games: Some(games),
            dry_run: true,
            launched: None,
        }
    }

    fn active_splash_game(&self) -> Option<&DiscoveredGame> {
        self.selected_game.and_then(|i| self.games.get(i))
    }

    fn splash_actions(game: &DiscoveredGame) -> Vec<(&'static str, Option<String>)> {
        if let Some(appid) = &game.appid {
            vec![
                ("\u{f04b}  Play", Some(format!("steam://rungameid/{appid}"))),
                (
                    "\u{f1b6}  Open in library",
                    Some(format!("steam://nav/games/details/{appid}")),
                ),
                (
                    "\u{f091}  Achievements",
                    Some(format!("steam://url/SteamIDAchievementsPage/{appid}")),
                ),
                ("\u{f1ea}  News", Some(format!("steam://appnews/{appid}"))),
                ("\u{f060}  Back", None),
            ]
        } else {
            vec![
                ("\u{f04b}  Play", Some(game.launch_uri.clone())),
                ("\u{f060}  Back", None),
            ]
        }
    }
}

impl RichMode for GamesMode {
    fn name(&self) -> &str {
        "games"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.selected_game = None;
        if !self.dry_run {
            self.palette = ActivePalette::load_from_user_config();
        }
        if let Some(mock) = &self.override_games {
            self.games = mock.clone();
            return Ok(());
        }
        self.games = discover_installed_games();
        Ok(())
    }

    fn message(&self) -> Option<String> {
        self.active_splash_game()
            .filter(|g| g.hero_path.as_ref().is_none_or(|p| !p.is_file()))
            .map(|g| format!("{}  ·  {}", g.name, g.source))
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(self.selected_game.is_some())
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: false,
            new_selection: Some(0),
            keep_filter: false,
        }
    }

    fn theme_snippets(&self) -> Vec<String> {
        let Some(game) = self.active_splash_game() else {
            return self
                .palette
                .as_ref()
                .map(|p| vec![p.to_games_rasi()])
                .unwrap_or_default();
        };

        let bg = self
            .palette
            .as_ref()
            .map(ActivePalette::bg_hex)
            .unwrap_or_else(|| "#1E222A".to_string());
        let bg_alt = self
            .palette
            .as_ref()
            .map(ActivePalette::bg_alt_hex)
            .unwrap_or_else(|| "#2E3440".to_string());
        let fg = self
            .palette
            .as_ref()
            .map(ActivePalette::fg_hex)
            .unwrap_or_else(|| "#ECEFF4".to_string());
        let accent = self
            .palette
            .as_ref()
            .map(ActivePalette::accent_hex)
            .unwrap_or_else(|| "#B48EAD".to_string());
        let accent_fg = self
            .palette
            .as_ref()
            .map(ActivePalette::accent_fg_hex)
            .unwrap_or_else(|| "#2E3440".to_string());

        let has_wide_hero = game.hero_path.as_ref().is_some_and(|p| p.is_file());
        let hero_css = if let Some(hero) = game.hero_path.as_ref().filter(|p| p.is_file()) {
            let esc = hero.to_string_lossy().replace('\\', "/");
            format!("background-image: url(\"{esc}\", width);")
        } else {
            // Never stretch a tiny square `.ico` across 2560px; use a sleek active-theme gradient.
            format!(
                "background-color: {bg}; background-image: linear-gradient(to right, {bg}, {bg_alt}, {bg});"
            )
        };

        let (splash_left_children, logo_block) = if let Some(logo) =
            game.logo_path.as_ref().filter(|p| p.is_file())
        {
            let esc = logo.to_string_lossy().replace('\\', "/");
            (
                "[icon-title, dummy]",
                format!(
                    "icon-title {{ expand: false; filename: \"{esc}\"; vertical-align: 0.5; horizontal-align: 0.0; size: 380px; background-color: transparent; }}"
                ),
            )
        } else if !has_wide_hero {
            if let Some(cover) = game.cover_path.as_ref().filter(|p| p.is_file()) {
                let esc = cover.to_string_lossy().replace('\\', "/");
                (
                    "[icon-title, dummy]",
                    format!(
                        "icon-title {{ expand: false; filename: \"{esc}\"; vertical-align: 0.5; horizontal-align: 0.0; size: 150px; background-color: transparent; }}"
                    ),
                )
            } else {
                ("[dummy]", String::new())
            }
        } else {
            ("[dummy]", String::new())
        };

        let action_count = Self::splash_actions(game).len().clamp(2, 5);
        let lv_top_pad = if action_count <= 2 { 64 } else { 22 };
        let panel_bg = if has_wide_hero {
            "rgba(12, 15, 22, 0.44)"
        } else {
            "rgba(12, 15, 22, 0.28)"
        };

        vec![format!(
            "configuration {{ show-icons: false; }}\n\
             window {{ height: 360px; width: 100%; location: south; anchor: south; border-color: {accent}; {hero_css} }}\n\
             mainbox {{ children: [splash-left, vertical-box]; orientation: horizontal; padding: 0px; spacing: 0px; }}\n\
             splash-left {{ expand: true; orientation: horizontal; padding: 0px 24px 0px 56px; background-color: transparent; children: {splash_left_children}; }}\n\
             {logo_block}\n\
             vertical-box {{ expand: false; width: 250px; orientation: vertical; background-color: {panel_bg}; border: 0px 0px 0px 1px; border-color: rgba(255, 255, 255, 0.18); children: [message, listview]; }}\n\
             message {{ padding: 20px 18px 0px 18px; background-color: transparent; }}\n\
             textbox {{ font: \"Segoe UI Bold 12\"; text-color: {fg}; horizontal-align: 0.5; }}\n\
             listview {{ layout: vertical; flow: vertical; columns: 1; lines: {action_count}; spacing: 9px; padding: {lv_top_pad}px 20px 16px 20px; fixed-height: true; scrollbar: false; }}\n\
             element {{ children: [element-text]; padding: 9px 14px; margin: 0px; orientation: horizontal; border: 1px solid; border-color: rgba(255, 255, 255, 0.22); background-color: rgba(255, 255, 255, 0.13); border-radius: 10px; text-color: #FFFFFF; }}\n\
             element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; border-radius: 10px; }}\n\
             element-text {{ horizontal-align: 0.5; vertical-align: 0.5; font: \"Segoe UI Semibold 11\"; text-color: inherit; }}\n\
             element-text selected.normal {{ horizontal-align: 0.5; text-color: {accent_fg}; }}"
        )]
    }

    fn entries(&self) -> Vec<ListEntry> {
        if let Some(game) = self.active_splash_game() {
            return Self::splash_actions(game)
                .into_iter()
                .map(|(label, _)| ListEntry {
                    text: label.to_string(),
                    ..Default::default()
                })
                .collect();
        }
        self.games
            .iter()
            .map(|g| ListEntry {
                text: g.name.clone(),
                icon: g
                    .cover_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                meta: Some(g.source.to_string()),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => {
                if self.selected_game.is_some() {
                    self.selected_game = None;
                    Ok(ModeStep::Reload)
                } else {
                    Ok(ModeStep::Exit(1))
                }
            }
            UiOutcome::Accepted { index, .. } => {
                if let Some(game) = self.active_splash_game().cloned() {
                    let actions = Self::splash_actions(&game);
                    if let Some((_, Some(uri))) = actions.get(*index) {
                        self.launched = Some(uri.clone());
                        if !self.dry_run {
                            launch_windows_app(&AppLaunchTarget::ShellPath(PathBuf::from(uri)))?;
                        }
                        self.selected_game = None;
                        return Ok(ModeStep::Exit(0));
                    }
                    // "Back" selected -> return to poster grid
                    self.selected_game = None;
                    return Ok(ModeStep::Reload);
                }

                if let Some(game) = self.games.get(*index).cloned() {
                    if game.appid.is_some() {
                        // Steam game -> open 2-step Hero Banner + 5-button splash menu
                        self.selected_game = Some(*index);
                        return Ok(ModeStep::Reload);
                    }
                    // Non-Steam game (Riot, Epic/Heroic, Xbox) -> launch immediately (0ms)
                    self.launched = Some(game.launch_uri.clone());
                    if !self.dry_run {
                        launch_windows_app(&AppLaunchTarget::ShellPath(PathBuf::from(
                            game.launch_uri,
                        )))?;
                    }
                    return Ok(ModeStep::Exit(0));
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::ModeSwitch(idx) => {
                self.selected_game = None;
                Ok(ModeStep::SwitchModeIndex(*idx))
            }
            _ => Ok(ModeStep::Reload),
        }
    }
}

fn is_non_game_steam_entry(appid: &str, name: &str) -> bool {
    if appid == "228980" {
        return true; // Steamworks Common Redistributables
    }
    let low = name.to_ascii_lowercase();
    low.starts_with("steam linux runtime")
        || low.starts_with("proton ")
        || low == "proton"
        || low.contains("steamworks common redistributables")
        || low.contains("dedicated server")
        || low.contains("soundtrack")
}

fn extract_vdf_value(content: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    for line in content.lines() {
        let trimmed = line.trim();
        let rest = if let Some(r) = trimmed.strip_prefix(&needle) {
            r
        } else if trimmed
            .to_ascii_lowercase()
            .starts_with(&needle.to_ascii_lowercase())
        {
            &trimmed[needle.len()..]
        } else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(after_q) = rest.strip_prefix('"') {
            if let Some(end_q) = after_q.find('"') {
                return Some(after_q[..end_q].replace("\\\\", "\\"));
            }
        }
    }
    None
}

fn parse_steam_library_paths(steam_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(val) = extract_vdf_value(trimmed, "path") {
                let p = PathBuf::from(val);
                if p.is_dir() && !roots.iter().any(|r| r.eq_ignore_ascii_case_path(&p)) {
                    roots.push(p);
                }
            }
        }
    }
    roots
}

trait PathEqIgnoreCase {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool;
}

impl PathEqIgnoreCase for PathBuf {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool {
        self.to_string_lossy()
            .eq_ignore_ascii_case(&other.to_string_lossy())
    }
}

struct SteamArtwork {
    cover: Option<PathBuf>,
    hero: Option<PathBuf>,
    logo: Option<PathBuf>,
}

fn find_steam_artwork(librarycache: &Path, appid: &str) -> SteamArtwork {
    let mut files = Vec::new();
    let app_dir = librarycache.join(appid);
    if app_dir.is_dir() {
        collect_images_shallow(&app_dir, 2, &mut files);
    }

    let find_in_files = |wanted: &[&str]| -> Option<PathBuf> {
        for w in wanted {
            if let Some(found) = files.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(w))
            }) {
                return Some(found.clone());
            }
        }
        None
    };

    let find_flat = |suffixes: &[&str]| -> Option<PathBuf> {
        for s in suffixes {
            let cand = librarycache.join(format!("{appid}{s}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    };

    let cover = find_flat(&[
        "_library_600x900.jpg",
        "_library_600x900.png",
        "_header.jpg",
    ])
    .or_else(|| {
        find_in_files(&[
            "library_capsule.jpg",
            "library_600x900.jpg",
            "library_capsule.png",
            "library_header.jpg",
            "header.jpg",
        ])
    })
    .or_else(|| files.first().cloned());

    let hero = find_flat(&["_library_hero.jpg", "_library_hero.png"]).or_else(|| {
        find_in_files(&[
            "library_hero.jpg",
            "library_hero.png",
            "library_hero_blur.jpg",
            "library_header.jpg",
            "header.jpg",
        ])
    });

    let logo = find_flat(&["_logo.png"]).or_else(|| find_in_files(&["logo.png"]));

    SteamArtwork { cover, hero, logo }
}

fn collect_images_shallow(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png") {
                    out.push(p);
                }
            }
        } else if depth > 0 && p.is_dir() {
            collect_images_shallow(&p, depth - 1, out);
        }
    }
}

fn find_steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(p) = read_steam_path_from_registry() {
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let candidates = [
        r"C:\Program Files (x86)\Steam",
        r"C:\Program Files\Steam",
        r"D:\Steam",
        r"E:\Steam",
    ];
    candidates
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
}

#[cfg(windows)]
fn read_steam_path_from_registry() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
        REG_VALUE_TYPE,
    };

    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_err()
        {
            return None;
        }
        let mut buf = [0u16; 512];
        let mut cb = (buf.len() * 2) as u32;
        let mut rtype = REG_VALUE_TYPE::default();
        let res = RegQueryValueExW(
            hkey,
            w!("SteamPath"),
            None,
            Some(&mut rtype),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut cb),
        );
        let _ = RegCloseKey(hkey);
        if res.is_ok() && rtype == REG_SZ {
            let len = (cb as usize / 2).saturating_sub(1);
            let s = String::from_utf16_lossy(&buf[..len]);
            let trimmed = s.trim_matches('\0').trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
        None
    }
}

fn discover_steam_games(by_key: &mut BTreeMap<String, DiscoveredGame>) {
    let Some(steam_root) = find_steam_root() else {
        return;
    };
    let librarycache = steam_root.join("appcache").join("librarycache");
    for lib in parse_steam_library_paths(&steam_root) {
        let steamapps = lib.join("steamapps");
        let Ok(rd) = std::fs::read_dir(&steamapps) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !fname.starts_with("appmanifest_") || !fname.ends_with(".acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(appid) = extract_vdf_value(&text, "appid") else {
                continue;
            };
            let Some(name) = extract_vdf_value(&text, "name") else {
                continue;
            };
            if is_non_game_steam_entry(&appid, &name) {
                continue;
            }
            let art = find_steam_artwork(&librarycache, &appid);
            let key = name.to_ascii_lowercase();
            by_key.entry(key).or_insert(DiscoveredGame {
                name,
                launch_uri: format!("steam://rungameid/{appid}"),
                appid: Some(appid),
                cover_path: art.cover,
                hero_path: art.hero,
                logo_path: art.logo,
                source: "Steam",
            });
        }
    }
}

fn extract_json_str(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let pos = text.find(&needle)?;
    let rest = &text[pos + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let after_q = after.strip_prefix('"')?;
    let end_q = after_q.find('"')?;
    Some(after_q[..end_q].to_string())
}

fn discover_epic_games(by_key: &mut BTreeMap<String, DiscoveredGame>) {
    let manifests = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(rd) = std::fs::read_dir(&manifests) else {
        return;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(name) = extract_json_str(&text, "DisplayName") else {
            continue;
        };
        let Some(app_name) = extract_json_str(&text, "AppName") else {
            continue;
        };
        let exe_icon = extract_json_str(&text, "InstallLocation").and_then(|loc| {
            let loc_p = PathBuf::from(loc.replace("\\\\", "\\"));
            extract_json_str(&text, "LaunchExecutable")
                .map(|ex| loc_p.join(ex.replace("\\\\", "\\")))
                .filter(|p| p.is_file())
        });
        let key = name.to_ascii_lowercase();
        by_key.entry(key).or_insert(DiscoveredGame {
            name,
            appid: None,
            launch_uri: format!(
                "com.epicgames.launcher://apps/{app_name}?action=launch&silent=true"
            ),
            cover_path: exe_icon,
            hero_path: None,
            logo_path: None,
            source: "Epic",
        });
    }
}

fn riot_metadata_ico(metadata_dir: &Path, product: &str, patchline: &str) -> Option<PathBuf> {
    let folder = format!("{product}.{patchline}");
    let ico = metadata_dir.join(&folder).join(format!("{folder}.ico"));
    ico.is_file().then_some(ico)
}

fn parse_riot_associated_paths(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(start) = json.find("\"associated_client\"") else {
        return out;
    };
    let rest = &json[start..];
    let Some(open) = rest.find('{') else {
        return out;
    };
    let body = &rest[open + 1..];
    let Some(close) = body.find('}') else {
        return out;
    };
    for line in body[..close].lines() {
        let trimmed = line.trim();
        if let Some(after_q) = trimmed.strip_prefix('"') {
            if let Some(end_q) = after_q.find('"') {
                out.push(after_q[..end_q].replace("\\\\", "\\"));
            }
        }
    }
    out
}

fn discover_riot_games(by_key: &mut BTreeMap<String, DiscoveredGame>) {
    let installs_path = PathBuf::from(r"C:\ProgramData\Riot Games\RiotClientInstalls.json");
    let metadata_dir = PathBuf::from(r"C:\ProgramData\Riot Games\Metadata");
    let start_menu_dir =
        PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Riot Games");

    let Ok(json) = std::fs::read_to_string(&installs_path) else {
        return;
    };
    for install_path in parse_riot_associated_paths(&json) {
        let low = install_path.to_ascii_lowercase();
        let mapped: Option<(&str, &str, &str, &str)> = if low.contains("valorant") {
            Some(("VALORANT", "valorant", "live", "VALORANT.lnk"))
        } else if low.contains("league of legends") && low.contains("pbe") {
            Some((
                "League of Legends PBE",
                "league_of_legends",
                "pbe",
                "League of Legends PBE.lnk",
            ))
        } else if low.contains("league of legends") {
            Some((
                "League of Legends",
                "league_of_legends",
                "live",
                "League of Legends.lnk",
            ))
        } else if low.contains("teamfight tactics") && low.contains("pbe") {
            Some((
                "Teamfight Tactics PBE",
                "teamfighttactics",
                "pbe",
                "Teamfight Tactics PBE.lnk",
            ))
        } else if low.contains("teamfight tactics") {
            Some((
                "Teamfight Tactics",
                "teamfighttactics",
                "live",
                "Teamfight Tactics.lnk",
            ))
        } else if low.contains("lor") || low.contains("runeterra") {
            Some((
                "Legends of Runeterra",
                "bacon",
                "live",
                "Legends of Runeterra.lnk",
            ))
        } else if low.contains("2xko") || low.contains("lion") {
            Some(("2XKO", "lion", "live", "2XKO.lnk"))
        } else {
            None
        };
        let Some((name, product, patchline, lnk_name)) = mapped else {
            continue;
        };
        let lnk = start_menu_dir.join(lnk_name);
        let launch_uri = if lnk.is_file() {
            lnk.to_string_lossy().into_owned()
        } else {
            continue;
        };
        let cover_path =
            riot_metadata_ico(&metadata_dir, product, patchline).or_else(|| Some(lnk.clone()));
        let key = name.to_ascii_lowercase();
        by_key.entry(key).or_insert(DiscoveredGame {
            name: name.to_string(),
            appid: None,
            launch_uri,
            cover_path,
            hero_path: None,
            logo_path: None,
            source: "Riot Games",
        });
    }
}

fn extract_xml_attr(xml: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let pos = xml.find(&needle)?;
    let rest = &xml[pos + needle.len()..];
    let end = rest.find('"')?;
    let val = rest[..end].trim();
    (!val.is_empty()).then(|| val.to_string())
}

fn parse_xbox_game_config(xml: &str, content_dir: &Path) -> Option<DiscoveredGame> {
    let name = extract_xml_attr(xml, "DefaultDisplayName")?;
    let exe_rel = extract_xml_attr(xml, "Name").and_then(|_| {
        // Find `<Executable Name="..."` specifically (skip `<Identity Name="..."`)
        let exec_pos = xml.find("<Executable ")?;
        extract_xml_attr(&xml[exec_pos..], "Name")
    })?;
    let exe_path = content_dir.join(&exe_rel);
    let launch_uri = exe_path.to_string_lossy().into_owned();
    let cover_path = extract_xml_attr(xml, "Square150x150Logo")
        .or_else(|| extract_xml_attr(xml, "StoreLogo"))
        .map(|rel| content_dir.join(rel))
        .filter(|p| p.is_file())
        .or_else(|| exe_path.is_file().then_some(exe_path.clone()));
    let hero_path = extract_xml_attr(xml, "SplashScreenImage")
        .map(|rel| content_dir.join(rel))
        .filter(|p| p.is_file());
    Some(DiscoveredGame {
        name,
        appid: None,
        launch_uri,
        cover_path,
        hero_path,
        logo_path: None,
        source: "Xbox",
    })
}

fn discover_xbox_games(by_key: &mut BTreeMap<String, DiscoveredGame>) {
    for root in [
        r"C:\XboxGames",
        r"D:\XboxGames",
        r"E:\XboxGames",
        r"F:\XboxGames",
    ] {
        let root_p = Path::new(root);
        let Ok(rd) = std::fs::read_dir(root_p) else {
            continue;
        };
        for ent in rd.flatten() {
            let dir = ent.path();
            if !dir.is_dir() {
                continue;
            }
            if dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("GameSave"))
            {
                continue;
            }
            let content_dir = dir.join("Content");
            let cfg_file = [
                content_dir.join("MicrosoftGame.config"),
                content_dir.join("MicrosoftGame.Config"),
                dir.join("MicrosoftGame.config"),
                dir.join("MicrosoftGame.Config"),
            ]
            .into_iter()
            .find(|p| p.is_file());
            let Some(cfg_path) = cfg_file else {
                continue;
            };
            let Ok(xml) = std::fs::read_to_string(&cfg_path) else {
                continue;
            };
            let base = cfg_path.parent().unwrap_or(&content_dir);
            if let Some(game) = parse_xbox_game_config(&xml, base) {
                let key = game.name.to_ascii_lowercase();
                by_key.entry(key).or_insert(game);
            }
        }
    }
}

fn discover_heroic_and_epic_shortcuts(by_key: &mut BTreeMap<String, DiscoveredGame>) {
    // 1. Heroic Games Launcher (Legendary / Epic) `installed.json`
    if let Ok(appdata) = std::env::var("APPDATA") {
        let installed = PathBuf::from(&appdata)
            .join("heroic")
            .join("legendaryConfig")
            .join("legendary")
            .join("installed.json");
        if let Ok(text) = std::fs::read_to_string(&installed) {
            for game in parse_heroic_installed_json(&text) {
                let key = clean_game_title(&game.name).to_ascii_lowercase();
                by_key.entry(key).or_insert(game);
            }
        }
    }

    // 2. Fallback: Start Menu `.lnk` shortcuts pointing to `heroic://launch/` or `com.epicgames.launcher://`
    let mut dirs = vec![PathBuf::from(
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs",
    )];
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(
            PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            if !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Some(uri) = extract_launcher_uri_from_lnk_bytes(&bytes) else {
                continue;
            };
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let clean = clean_game_title(stem);
            if clean.is_empty() {
                continue;
            }
            let source = if uri.starts_with("heroic://") {
                "Epic (Heroic)"
            } else {
                "Epic"
            };
            let key = clean.to_ascii_lowercase();
            by_key.entry(key).or_insert(DiscoveredGame {
                name: clean,
                appid: None,
                launch_uri: path.to_string_lossy().into_owned(),
                cover_path: Some(path.clone()),
                hero_path: None,
                logo_path: None,
                source,
            });
        }
    }
}

fn clean_game_title(raw: &str) -> String {
    raw.replace("\\u00ae", "")
        .replace("\\u00AE", "")
        .replace("\\u2122", "")
        .replace("\\u2122", "")
        .replace(['®', '™'], "")
        .trim()
        .to_string()
}

fn parse_heroic_installed_json(json: &str) -> Vec<DiscoveredGame> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = json[pos..].find("\"app_name\"") {
        let start = pos + rel;
        let block_end = json[start + 10..]
            .find("\"app_name\"")
            .map(|n| start + 10 + n)
            .unwrap_or(json.len());
        let block = &json[start..block_end];
        if let (Some(app_name), Some(title_raw)) = (
            extract_json_str(block, "app_name"),
            extract_json_str(block, "title"),
        ) {
            let title = clean_game_title(&title_raw);
            let exe_icon = extract_json_str(block, "install_path").and_then(|loc| {
                let loc_p = PathBuf::from(loc.replace("\\\\", "\\"));
                extract_json_str(block, "executable")
                    .map(|ex| loc_p.join(ex.replace("\\\\", "\\")))
                    .filter(|p| p.is_file())
            });
            out.push(DiscoveredGame {
                name: title,
                appid: None,
                launch_uri: format!("heroic://launch/?appName={app_name}&runner=legendary"),
                cover_path: exe_icon,
                hero_path: None,
                logo_path: None,
                source: "Epic (Heroic)",
            });
        }
        pos = block_end;
    }
    out
}

fn extract_launcher_uri_from_lnk_bytes(bytes: &[u8]) -> Option<String> {
    let utf16: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    let wide = String::from_utf16_lossy(&utf16);
    for prefix in ["heroic://launch/", "com.epicgames.launcher://apps/"] {
        if let Some(i) = wide.find(prefix) {
            let tail = &wide[i..];
            let end = tail
                .find(|c: char| c == '\0' || c.is_control() || c.is_whitespace())
                .unwrap_or(tail.len());
            return Some(tail[..end].to_string());
        }
    }
    let ascii = String::from_utf8_lossy(bytes);
    for prefix in ["heroic://launch/", "com.epicgames.launcher://apps/"] {
        if let Some(i) = ascii.find(prefix) {
            let tail = &ascii[i..];
            let end = tail
                .find(|c: char| c == '\0' || c.is_control() || c.is_whitespace())
                .unwrap_or(tail.len());
            return Some(tail[..end].to_string());
        }
    }
    None
}

pub fn discover_installed_games() -> Vec<DiscoveredGame> {
    let mut by_key = BTreeMap::new();
    discover_steam_games(&mut by_key);
    discover_riot_games(&mut by_key);
    discover_xbox_games(&mut by_key);
    discover_epic_games(&mut by_key);
    discover_heroic_and_epic_shortcuts(&mut by_key);
    by_key.into_values().collect()
}

/// Launch recency & frequency record (`rofi-games` style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameAccess {
    pub last_played_epoch: u64,
    pub launch_count: u32,
}

/// Stable identifier key for persisting a game's access history.
pub fn game_access_key(game: &DiscoveredGame) -> String {
    if let Some(appid) = &game.appid {
        format!("steam:{appid}")
    } else {
        format!("title:{}", game.name.to_ascii_lowercase())
    }
}

fn game_access_store_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::user_home_dir().join("AppData").join("Local"));
    base.join("LiteRofi").join("cache").join("games-access.tsv")
}

fn games_toml_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::user_home_dir().join("AppData").join("Roaming"));
    base.join("LiteRofi").join("games.toml")
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load_game_access_map_from(path: &Path) -> BTreeMap<String, GameAccess> {
    let mut map = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        let mut parts = line.split('\t');
        let Some(key) = parts.next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(epoch) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let count = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        map.insert(
            key.to_string(),
            GameAccess {
                last_played_epoch: epoch,
                launch_count: count,
            },
        );
    }
    map
}

pub fn save_game_access_map_to(path: &Path, map: &BTreeMap<String, GameAccess>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = String::new();
    for (k, v) in map {
        out.push_str(&format!(
            "{k}\t{}\t{}\n",
            v.last_played_epoch, v.launch_count
        ));
    }
    let _ = std::fs::write(path, out);
}

pub fn load_game_access_map() -> BTreeMap<String, GameAccess> {
    load_game_access_map_from(&game_access_store_path())
}

pub fn record_game_launch(key: &str) {
    let path = game_access_store_path();
    let mut map = load_game_access_map_from(&path);
    let entry = map.entry(key.to_string()).or_default();
    entry.last_played_epoch = now_unix_secs();
    entry.launch_count = entry.launch_count.saturating_add(1);
    save_game_access_map_to(&path, &map);
}

pub fn clear_game_access(key: &str) {
    let path = game_access_store_path();
    let mut map = load_game_access_map_from(&path);
    if map.remove(key).is_some() {
        save_game_access_map_to(&path, &map);
    }
}

/// Formats a real launch timestamp into a concise relative string (returns `None` if never launched).
pub fn format_last_played(epoch: u64) -> Option<String> {
    if epoch == 0 {
        return None;
    }
    let now = now_unix_secs();
    let diff = now.saturating_sub(epoch);
    if diff < 120 {
        Some("Played just now".to_string())
    } else if diff < 3600 {
        Some(format!("Played {}m ago", diff / 60))
    } else if diff < 86_400 {
        Some(format!("Played {}h ago", diff / 3600))
    } else if diff < 86_400 * 30 {
        Some(format!("Played {}d ago", diff / 86_400))
    } else {
        Some(format!("Played {}mo ago", (diff / (86_400 * 30)).max(1)))
    }
}

/// Applies optional `%APPDATA%\LiteRofi\games.toml` hide rules and custom `[[entries]]`
/// (`rofi-games` compatible keys: `hide`, `box_art_dir`, `title`, `launch_command`, `path_box_art`, `path_hero`).
pub fn apply_games_toml_overrides(games: &mut Vec<DiscoveredGame>, toml_path: &Path) {
    let Ok(text) = std::fs::read_to_string(toml_path) else {
        return;
    };
    let mut hidden: Vec<String> = Vec::new();
    let mut box_art_dir: Option<PathBuf> = None;
    let mut in_entry = false;
    let mut cur_title = String::new();
    let mut cur_cmd = String::new();
    let mut cur_art: Option<String> = None;
    let mut cur_hero: Option<String> = None;
    let mut custom_entries: Vec<DiscoveredGame> = Vec::new();

    let flush_entry = |title: &mut String,
                       cmd: &mut String,
                       art: &mut Option<String>,
                       hero: &mut Option<String>,
                       box_dir: &Option<PathBuf>,
                       out: &mut Vec<DiscoveredGame>| {
        if !title.is_empty() && !cmd.is_empty() {
            let resolve_img = |raw: &Option<String>| -> Option<PathBuf> {
                let s = raw.as_deref()?.trim();
                if s.is_empty() {
                    return None;
                }
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p.is_file().then_some(p)
                } else {
                    box_dir
                        .as_ref()
                        .map(|d| d.join(&p))
                        .filter(|cand| cand.is_file())
                        .or_else(|| p.is_file().then_some(p))
                }
            };
            out.push(DiscoveredGame {
                name: std::mem::take(title),
                appid: None,
                launch_uri: std::mem::take(cmd),
                cover_path: resolve_img(art),
                hero_path: resolve_img(hero),
                logo_path: None,
                source: "Custom",
            });
        }
        title.clear();
        cmd.clear();
        *art = None;
        *hero = None;
    };

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[entries]]" {
            if in_entry {
                flush_entry(
                    &mut cur_title,
                    &mut cur_cmd,
                    &mut cur_art,
                    &mut cur_hero,
                    &box_art_dir,
                    &mut custom_entries,
                );
            }
            in_entry = true;
            continue;
        }
        if line.starts_with('[') {
            if in_entry {
                flush_entry(
                    &mut cur_title,
                    &mut cur_cmd,
                    &mut cur_art,
                    &mut cur_hero,
                    &box_art_dir,
                    &mut custom_entries,
                );
                in_entry = false;
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        let unquote = |s: &str| s.trim().trim_matches('"').trim_matches('\'').to_string();

        if !in_entry {
            if key == "box_art_dir" {
                let p = unquote(val);
                if !p.is_empty() {
                    box_art_dir = Some(PathBuf::from(p));
                }
            } else if key == "hide" {
                let inner = val.trim_start_matches('[').trim_end_matches(']');
                for item in inner.split(',') {
                    let s = unquote(item);
                    if !s.is_empty() {
                        hidden.push(s.to_ascii_lowercase());
                    }
                }
            }
        } else {
            match key {
                "title" | "name" => cur_title = unquote(val),
                "launch_command" | "launch_uri" | "command" => cur_cmd = unquote(val),
                "path_box_art" | "box_art" | "cover" => cur_art = Some(unquote(val)),
                "path_hero" | "hero" | "banner" => cur_hero = Some(unquote(val)),
                _ => {}
            }
        }
    }
    if in_entry {
        flush_entry(
            &mut cur_title,
            &mut cur_cmd,
            &mut cur_art,
            &mut cur_hero,
            &box_art_dir,
            &mut custom_entries,
        );
    }

    if !hidden.is_empty() {
        games.retain(|g| {
            let low_name = g.name.to_ascii_lowercase();
            let low_src = g.source.to_ascii_lowercase();
            !hidden.iter().any(|h| h == &low_name || h == &low_src)
        });
    }
    games.extend(custom_entries);
}

/// Discovers all installed games, applies `games.toml` overrides, and sorts by
/// launch recency (most recently played first), then launch count, then name.
pub fn discover_games_sorted() -> Vec<(DiscoveredGame, Option<GameAccess>)> {
    let mut games = discover_installed_games();
    apply_games_toml_overrides(&mut games, &games_toml_path());
    let access = load_game_access_map();
    let mut out: Vec<(DiscoveredGame, Option<GameAccess>)> = games
        .into_iter()
        .map(|g| {
            let key = game_access_key(&g);
            let acc = access.get(&key).copied();
            (g, acc)
        })
        .collect();
    sort_games_with_access(&mut out);
    out
}

pub fn sort_games_with_access(items: &mut [(DiscoveredGame, Option<GameAccess>)]) {
    items.sort_by(|(ga, aa), (gb, ab)| {
        let ta = aa.map_or(0, |x| x.last_played_epoch);
        let tb = ab.map_or(0, |x| x.last_played_epoch);
        tb.cmp(&ta)
            .then_with(|| {
                let ca = aa.map_or(0, |x| x.launch_count);
                let cb = ab.map_or(0, |x| x.launch_count);
                cb.cmp(&ca)
            })
            .then_with(|| {
                ga.name
                    .to_ascii_lowercase()
                    .cmp(&gb.name.to_ascii_lowercase())
            })
    });
}

/// Resolves the installation directory of a game (for `Shift+Enter` / `Folder` action).
pub fn resolve_game_folder(game: &DiscoveredGame) -> Option<PathBuf> {
    if let Some(appid) = &game.appid {
        if let Some(steam_root) = find_steam_root() {
            let acf_name = format!("appmanifest_{appid}.acf");
            for lib in parse_steam_library_paths(&steam_root) {
                let steamapps = lib.join("steamapps");
                let acf = steamapps.join(&acf_name);
                if let Ok(text) = std::fs::read_to_string(&acf) {
                    if let Some(dir_name) = extract_vdf_value(&text, "installdir") {
                        let candidate = steamapps.join("common").join(dir_name);
                        if candidate.is_dir() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }
    let p = Path::new(&game.launch_uri);
    if p.is_file() {
        return p.parent().map(Path::to_path_buf);
    }
    if p.is_dir() {
        return Some(p.to_path_buf());
    }
    None
}

/// Launches a game URI or executable path cleanly.
pub fn launch_game_target(uri_or_path: &str) -> Result<(), String> {
    launch_windows_app(&AppLaunchTarget::ShellPath(PathBuf::from(uri_or_path)))
}

/// Pre-resolved game catalog record persisted in `%LOCALAPPDATA%\LiteRofi\cache\games-catalog.tsv`
/// so `Alt+G` opens instantaneously without rescanning Steam/Epic/Riot/Xbox directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedGameRecord {
    pub game: DiscoveredGame,
    pub folder_path: Option<PathBuf>,
}

fn games_catalog_cache_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::user_home_dir().join("AppData").join("Local"));
    base.join("LiteRofi")
        .join("cache")
        .join("games-catalog.tsv")
}

fn static_source(s: &str) -> &'static str {
    match s {
        "Steam" => "Steam",
        "Epic Games" => "Epic Games",
        "Riot Games" => "Riot Games",
        "Xbox" => "Xbox",
        "Heroic" => "Heroic",
        "Custom" => "Custom",
        _ => "Game",
    }
}

fn build_steam_folder_map() -> BTreeMap<String, PathBuf> {
    let mut map = BTreeMap::new();
    let Some(steam_root) = find_steam_root() else {
        return map;
    };
    for lib in parse_steam_library_paths(&steam_root) {
        let steamapps = lib.join("steamapps");
        let Ok(rd) = std::fs::read_dir(&steamapps) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Some(appid) = extract_vdf_value(&text, "appid") else {
                continue;
            };
            let Some(dir_name) = extract_vdf_value(&text, "installdir") else {
                continue;
            };
            let cand = steamapps.join("common").join(dir_name);
            if cand.is_dir() {
                map.insert(appid, cand);
            }
        }
    }
    map
}

/// Performs a full multi-launcher discovery + single-pass folder resolution, saves the
/// catalog cache, and returns records sorted by launch recency.
pub fn discover_games_catalog_fresh() -> Vec<(CachedGameRecord, Option<GameAccess>)> {
    let mut games = discover_installed_games();
    apply_games_toml_overrides(&mut games, &games_toml_path());
    let steam_folders = build_steam_folder_map();
    let records: Vec<CachedGameRecord> = games
        .into_iter()
        .map(|g| {
            let folder_path = g
                .appid
                .as_ref()
                .and_then(|id| steam_folders.get(id).cloned())
                .or_else(|| {
                    let p = Path::new(&g.launch_uri);
                    if p.is_file() {
                        p.parent().map(Path::to_path_buf)
                    } else if p.is_dir() {
                        Some(p.to_path_buf())
                    } else {
                        None
                    }
                });
            CachedGameRecord {
                game: g,
                folder_path,
            }
        })
        .collect();
    save_games_catalog_cache(&records);
    attach_access_and_sort(records)
}

/// Loads the cached game catalog in <1ms and sorts by latest launch recency.
pub fn load_games_catalog_cached() -> Option<Vec<(CachedGameRecord, Option<GameAccess>)>> {
    let text = std::fs::read_to_string(games_catalog_cache_path()).ok()?;
    let mut records = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 8 {
            continue;
        }
        let opt_path = |s: &str| (!s.is_empty()).then(|| PathBuf::from(s));
        let name = cols[0].to_string();
        if name.is_empty() {
            continue;
        }
        records.push(CachedGameRecord {
            game: DiscoveredGame {
                name,
                appid: (!cols[1].is_empty()).then(|| cols[1].to_string()),
                source: static_source(cols[2]),
                launch_uri: cols[3].to_string(),
                cover_path: opt_path(cols[5]),
                hero_path: opt_path(cols[6]),
                logo_path: opt_path(cols[7]),
            },
            folder_path: opt_path(cols[4]),
        });
    }
    if records.is_empty() {
        return None;
    }
    Some(attach_access_and_sort(records))
}

fn save_games_catalog_cache(records: &[CachedGameRecord]) {
    let path = games_catalog_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    let opt_p = |p: &Option<PathBuf>| {
        p.as_ref()
            .map(|x| clean(&x.to_string_lossy()))
            .unwrap_or_default()
    };
    let mut out = String::with_capacity(records.len() * 180);
    for r in records {
        let g = &r.game;
        out.push_str(&clean(&g.name));
        out.push('\t');
        out.push_str(g.appid.as_deref().unwrap_or(""));
        out.push('\t');
        out.push_str(g.source);
        out.push('\t');
        out.push_str(&clean(&g.launch_uri));
        out.push('\t');
        out.push_str(&opt_p(&r.folder_path));
        out.push('\t');
        out.push_str(&opt_p(&g.cover_path));
        out.push('\t');
        out.push_str(&opt_p(&g.hero_path));
        out.push('\t');
        out.push_str(&opt_p(&g.logo_path));
        out.push('\n');
    }
    let _ = std::fs::write(path, out);
}

fn attach_access_and_sort(
    records: Vec<CachedGameRecord>,
) -> Vec<(CachedGameRecord, Option<GameAccess>)> {
    let access = load_game_access_map();
    let mut out: Vec<(CachedGameRecord, Option<GameAccess>)> = records
        .into_iter()
        .map(|r| {
            let key = game_access_key(&r.game);
            let acc = access.get(&key).copied();
            (r, acc)
        })
        .collect();
    out.sort_by(|(ra, aa), (rb, ab)| {
        let ta = aa.map_or(0, |x| x.last_played_epoch);
        let tb = ab.map_or(0, |x| x.last_played_epoch);
        tb.cmp(&ta)
            .then_with(|| {
                let ca = aa.map_or(0, |x| x.launch_count);
                let cb = ab.map_or(0, |x| x.launch_count);
                cb.cmp(&ca)
            })
            .then_with(|| {
                ra.game
                    .name
                    .to_ascii_lowercase()
                    .cmp(&rb.game.name.to_ascii_lowercase())
            })
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_vdf_value_parses_steam_manifest_fields() {
        let sample =
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1172470\"\n\t\"name\"\t\t\"Apex Legends\"\n}\n";
        assert_eq!(
            extract_vdf_value(sample, "appid").as_deref(),
            Some("1172470")
        );
        assert_eq!(
            extract_vdf_value(sample, "name").as_deref(),
            Some("Apex Legends")
        );
        assert!(!is_non_game_steam_entry("1172470", "Apex Legends"));
        assert!(is_non_game_steam_entry(
            "1070560",
            "Steam Linux Runtime 1.0"
        ));
    }

    #[test]
    fn games_mode_opens_splash_menu_and_supports_back_and_play() {
        let mut mode = GamesMode::with_mock(vec![DiscoveredGame {
            name: "Half-Life 2".to_string(),
            appid: Some("220".to_string()),
            launch_uri: "steam://rungameid/220".to_string(),
            cover_path: None,
            hero_path: None,
            logo_path: None,
            source: "Steam",
        }]);
        mode.init().unwrap();
        assert_eq!(mode.entries().len(), 1);
        assert_eq!(mode.entries()[0].text, "Half-Life 2");

        // Selecting a game opens its 5-item splash menu
        let step_splash = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 0,
                    text: "Half-Life 2".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step_splash, ModeStep::Reload);
        assert_eq!(mode.entries().len(), 5);
        assert!(mode.entries()[0].text.contains("Play"));
        assert!(mode.entries()[4].text.contains("Back"));

        // Selecting Back returns to poster grid
        let step_back = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 4,
                    text: "\u{f060}  Back".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step_back, ModeStep::Reload);
        assert_eq!(mode.entries().len(), 1);

        // Re-open splash and click Achievements (index 2)
        let _ = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 0,
                    text: "Half-Life 2".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        let step_ach = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 2,
                    text: "\u{f091}  Achievements".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step_ach, ModeStep::Exit(0));
        assert_eq!(
            mode.launched.as_deref(),
            Some("steam://url/SteamIDAchievementsPage/220")
        );
    }

    #[test]
    fn parses_riot_and_xbox_manifests() {
        let riot_json = r#"{ "associated_client": { "D:/Riot Games/VALORANT/live/": "D:/Riot Games/Riot Client/RiotClientServices.exe" } }"#;
        let paths = parse_riot_associated_paths(riot_json);
        assert_eq!(paths, vec!["D:/Riot Games/VALORANT/live/"]);

        let xbox_xml = r#"<Game><Identity Name="Microsoft.Test" /><ExecutableList><Executable Name="Minecraft.exe" /></ExecutableList><ShellVisuals DefaultDisplayName="Minecraft Launcher" Square150x150Logo="GraphicsLogo.png" SplashScreenImage="SplashScreen.png" /></Game>"#;
        let g = parse_xbox_game_config(xbox_xml, Path::new(r"C:\XboxGames\Minecraft\Content"))
            .expect("xbox game");
        assert_eq!(g.name, "Minecraft Launcher");
        assert_eq!(g.source, "Xbox");
        assert!(g.launch_uri.ends_with("Minecraft.exe"));
    }

    #[test]
    fn sorts_games_by_recency_and_applies_games_toml() {
        let base = std::env::temp_dir().join(format!("literofi_games_toml_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        let toml = base.join("games.toml");
        std::fs::write(
            &toml,
            "hide = [\"Hidden Game\"]\n\n[[entries]]\ntitle = \"Celeste\"\nlaunch_command = \"C:/Games/Celeste.exe\"\n",
        )
        .unwrap();

        let mut list = vec![
            DiscoveredGame {
                name: "Hidden Game".into(),
                appid: Some("1".into()),
                launch_uri: "steam://rungameid/1".into(),
                cover_path: None,
                hero_path: None,
                logo_path: None,
                source: "Steam",
            },
            DiscoveredGame {
                name: "Alpha".into(),
                appid: Some("2".into()),
                launch_uri: "steam://rungameid/2".into(),
                cover_path: None,
                hero_path: None,
                logo_path: None,
                source: "Steam",
            },
            DiscoveredGame {
                name: "Zelda".into(),
                appid: Some("3".into()),
                launch_uri: "steam://rungameid/3".into(),
                cover_path: None,
                hero_path: None,
                logo_path: None,
                source: "Steam",
            },
        ];
        apply_games_toml_overrides(&mut list, &toml);
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|g| g.name != "Hidden Game"));
        assert!(list
            .iter()
            .any(|g| g.name == "Celeste" && g.source == "Custom"));

        let mut paired: Vec<(DiscoveredGame, Option<GameAccess>)> = list
            .into_iter()
            .map(|g| {
                let acc = if g.name == "Zelda" {
                    Some(GameAccess {
                        last_played_epoch: 5000,
                        launch_count: 1,
                    })
                } else if g.name == "Celeste" {
                    Some(GameAccess {
                        last_played_epoch: 9000,
                        launch_count: 2,
                    })
                } else {
                    None
                };
                (g, acc)
            })
            .collect();
        sort_games_with_access(&mut paired);
        assert_eq!(paired[0].0.name, "Celeste");
        assert_eq!(paired[1].0.name, "Zelda");
        assert_eq!(paired[2].0.name, "Alpha");
        assert_eq!(format_last_played(0), None);

        let _ = std::fs::remove_dir_all(&base);
    }
}
