use literofi_core::config::Config;
use literofi_core::mode::custom_exit_code;
use literofi_modes::combi::CombiMode;
use literofi_modes::dmenu::DmenuMode;
use literofi_modes::drun::DrunMode;
use literofi_modes::filebrowser::FileBrowserMode;
use literofi_modes::keys::KeysMode;
use literofi_modes::recursivebrowser::RecursiveBrowserMode;
use literofi_modes::registry::run_modes_self_test;
use literofi_modes::run::RunMode;
use literofi_modes::script::ScriptMode;
use literofi_modes::ssh::SshMode;
use literofi_modes::win32_apps::discover_windows_apps;
use literofi_modes::win32_windows::{enumerate_windows, DiscoveredWindow};
use literofi_modes::window::WindowMode;
use literofi_modes::{ModeStep, RichMode};
use literofi_render::state::{LauncherState, UiOutcome};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("literofi_w4_{tag}_{}_{}", std::process::id(), id));
        fs::create_dir_all(&dir).expect("create temp dir");
        Self { path: dir }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn dmenu_e2e_protocol_multiselect_format_and_exit_codes() {
    let mut cfg = Config {
        dmenu: true,
        multi_select: true,
        format: "i:s".to_string(),
        active_ranges: Some("0".to_string()),
        urgent_ranges: Some("3".to_string()),
        ..Config::default()
    };

    let stream = b"\0prompt\x1fPickItems\n\
                   \0message\x1fUseShiftEnter\n\
                   \0no-custom\x1ftrue\n\
                   alpha\0meta\x1ffirst-tag\n\
                   beta\n\
                   gamma\0nonselectable\x1ftrue\n\
                   delta\n";

    let mut dmenu = DmenuMode::from_bytes(&cfg, stream.to_vec());
    dmenu.init().expect("dmenu init");

    assert_eq!(dmenu.prompt_override().as_deref(), Some("PickItems"));
    assert_eq!(dmenu.message().as_deref(), Some("UseShiftEnter"));
    assert_eq!(dmenu.no_custom_override(), Some(true));

    let entries = dmenu.entries();
    assert_eq!(entries.len(), 4);
    assert!(entries[0].active);
    assert!(entries[2].nonselectable);
    assert!(entries[3].urgent);
    assert_eq!(entries[0].meta.as_deref(), Some("first-tag"));

    // Custom input is blocked by \0no-custom\x1ftrue
    let step_custom = dmenu
        .handle_outcome(
            &UiOutcome::CustomInput {
                text: "typed".to_string(),
                exit_code: 0,
            },
            "typed",
        )
        .expect("custom");
    assert_eq!(step_custom, ModeStep::Reload);
    assert!(dmenu.outputs().is_empty());

    // Toggle index 1 and index 3 via AcceptedAlt (-kb-accept-alt multi-select)
    let step1 = dmenu
        .handle_outcome(
            &UiOutcome::AcceptedAlt {
                index: 1,
                text: "beta".to_string(),
            },
            "",
        )
        .expect("toggle 1");
    assert_eq!(step1, ModeStep::Reload);
    assert_eq!(dmenu.overlay().as_deref(), Some("1/4"));

    let step2 = dmenu
        .handle_outcome(
            &UiOutcome::AcceptedAlt {
                index: 3,
                text: "delta".to_string(),
            },
            "",
        )
        .expect("toggle 3");
    assert_eq!(step2, ModeStep::Reload);
    assert_eq!(dmenu.overlay().as_deref(), Some("2/4"));

    // Accept with custom exit code (-kb-custom-2 -> 11) emits both selected rows
    let exit_step = dmenu
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 3,
                text: "delta".to_string(),
                exit_code: custom_exit_code(2),
            },
            "q",
        )
        .expect("accept multi");
    assert_eq!(exit_step, ModeStep::Exit(11));
    assert_eq!(
        dmenu.outputs(),
        &["1:beta".to_string(), "3:delta".to_string()]
    );

    // Cancel exits with code 1 and -only-match blocks custom input
    cfg.only_match = true;
    let mut only_match = DmenuMode::from_bytes(&cfg, b"one\ntwo\n".to_vec());
    only_match.init().expect("init");
    assert_eq!(
        only_match
            .handle_outcome(&UiOutcome::Cancelled, "")
            .expect("cancel"),
        ModeStep::Exit(1)
    );
}

#[test]
fn run_e2e_path_discovery_ignored_prefixes_history_and_delete() {
    let tmp = TempDirGuard::new("run");
    let bin_dir = tmp.path().join("bin");
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&bin_dir).expect("bin dir");

    fs::write(bin_dir.join("alpha.exe"), b"").expect("alpha.exe");
    fs::write(bin_dir.join("beta.cmd"), b"@echo off\n").expect("beta.cmd");
    fs::write(bin_dir.join("_skipme.exe"), b"").expect("_skipme.exe");
    fs::write(bin_dir.join("notes.txt"), b"not executable").expect("notes.txt");

    let cfg = Config {
        ignored_prefixes: "_".to_string(),
        terminal: "cmd.exe /C".to_string(),
        run_shell_command: "{terminal} {cmd}".to_string(),
        ..Config::default()
    };

    let mut run =
        RunMode::new(&cfg).with_test_env(vec![bin_dir.clone()], false, cache_dir.clone(), true);
    run.init().expect("run init");

    let names: Vec<String> = run.entries().into_iter().map(|e| e.text).collect();
    assert_eq!(names, vec!["alpha.exe", "beta.cmd"]);

    // Execute beta.cmd -> records history & captures dry-run argv
    let idx_beta = names
        .iter()
        .position(|n| n == "beta.cmd")
        .expect("beta pos");
    let step = run
        .handle_outcome(
            &UiOutcome::Accepted {
                index: idx_beta,
                text: "beta.cmd".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("exec beta");
    assert_eq!(step, ModeStep::Exit(0));
    assert_eq!(run.launched_commands(), &[vec!["beta.cmd".to_string()]]);

    // AcceptedAlt uses terminal wrapper template
    let idx_alpha = names
        .iter()
        .position(|n| n == "alpha.exe")
        .expect("alpha pos");
    let _ = run
        .handle_outcome(
            &UiOutcome::AcceptedAlt {
                index: idx_alpha,
                text: "alpha.exe".to_string(),
            },
            "",
        )
        .expect("exec alpha in term");
    assert_eq!(
        run.launched_commands()[1],
        vec![
            "cmd.exe".to_string(),
            "/C".to_string(),
            "alpha.exe".to_string()
        ]
    );

    // On re-init, most frequently/recently launched items float to the top with active=true
    let mut run2 = RunMode::new(&cfg).with_test_env(vec![bin_dir], false, cache_dir, true);
    run2.init().expect("run2 init");
    let entries2 = run2.entries();
    assert!(entries2[0].active && entries2[1].active);

    // DeleteEntry removes item from history
    let first_name = entries2[0].text.clone();
    let del_step = run2
        .handle_outcome(&UiOutcome::DeleteEntry { index: 0 }, "")
        .expect("delete");
    assert_eq!(del_step, ModeStep::Reload);
    assert!(run2
        .entries()
        .iter()
        .find(|e| e.text == first_name)
        .is_some_and(|e| !e.active));
}

#[test]
fn drun_e2e_desktop_parsing_format_categories_and_live_windows_discovery() {
    let tmp = TempDirGuard::new("drun");
    let apps_dir = tmp.path().join("apps");
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&apps_dir).expect("apps dir");

    fs::write(
        apps_dir.join("editor.desktop"),
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=CodePad\n\
         GenericName=Text Editor\n\
         Comment=Edit source files\n\
         Exec=codepad.exe %F\n\
         Categories=Development;TextEditor;\n\
         Keywords=code;rust;\n",
    )
    .expect("editor.desktop");

    fs::write(
        apps_dir.join("hidden.desktop"),
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=SecretTool\n\
         Exec=secret.exe\n\
         NoDisplay=true\n",
    )
    .expect("hidden.desktop");

    fs::write(
        apps_dir.join("game.desktop"),
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Arcade\n\
         Exec=arcade.exe\n\
         Categories=Game;\n",
    )
    .expect("game.desktop");

    let cfg = Config {
        drun_exclude_categories: Some("Game".to_string()),
        ..Config::default()
    };

    let mut drun = DrunMode::new(&cfg).with_desktop_dirs(
        vec![apps_dir.clone()],
        false,
        cache_dir.clone(),
        true,
    );
    drun.init().expect("drun init");

    let entries = drun.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "CodePad (Text Editor)");
    let meta = entries[0].meta.as_deref().unwrap_or("");
    assert!(meta.contains("codepad.exe") && meta.contains("Development") && meta.contains("rust"));

    // Launch updates drun history
    let step = drun
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: entries[0].text.clone(),
                exit_code: 0,
            },
            "",
        )
        .expect("drun accept");
    assert_eq!(step, ModeStep::Exit(0));
    assert_eq!(drun.launched_ids(), &["editor.desktop".to_string()]);

    // Live Windows discovery finds real installed Start Menu / AppsFolder apps cleanly
    let live_apps = discover_windows_apps();
    assert!(
        !live_apps.is_empty(),
        "expected non-empty live Windows app discovery"
    );
}

#[test]
fn window_and_windowcd_e2e_live_win32_enumeration_and_formatting() {
    // Live Win32 enumeration sanity check
    let _live = enumerate_windows(None);
    let cfg = Config::default();

    let mut live_win = WindowMode::new(&cfg, false);
    live_win.init().expect("live window init");
    let mut live_cd = WindowMode::new(&cfg, true);
    live_cd.init().expect("live windowcd init");
    assert!(live_cd.entries().len() <= live_win.entries().len());

    // Deterministic mock formatting, field matching, current-monitor filter & actions
    let mocks = vec![
        DiscoveredWindow {
            hwnd: 1001,
            title: "Project Notes".to_string(),
            class_name: "Notepad".to_string(),
            process_name: "notepad".to_string(),
            monitor_index: 0,
            is_active: true,
            is_minimized: false,
            is_on_current_monitor: true,
        },
        DiscoveredWindow {
            hwnd: 2002,
            title: "Second Monitor Browser".to_string(),
            class_name: "Chrome_WidgetWin_1".to_string(),
            process_name: "chrome".to_string(),
            monitor_index: 1,
            is_active: false,
            is_minimized: false,
            is_on_current_monitor: false,
        },
    ];

    let mut w_all = WindowMode::new(&cfg, false).with_mock_windows(mocks.clone(), true);
    w_all.init().expect("mock window init");
    assert_eq!(w_all.entries().len(), 2);
    assert!(w_all.entries()[0].active);
    assert!(w_all.entries()[0].text.contains("Project Notes"));
    assert!(w_all.entries()[0].text.contains("notepad"));

    let mut w_cd = WindowMode::new(&cfg, true).with_mock_windows(mocks, true);
    w_cd.init().expect("mock windowcd init");
    assert_eq!(w_cd.entries().len(), 1);

    let act = w_all
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 1,
                text: String::new(),
                exit_code: 0,
            },
            "",
        )
        .expect("activate");
    assert_eq!(act, ModeStep::Exit(0));
    assert_eq!(w_all.action_log(), &["activate:2002".to_string()]);

    let del = w_all
        .handle_outcome(&UiOutcome::DeleteEntry { index: 0 }, "")
        .expect("close");
    assert_eq!(del, ModeStep::Reload);
    assert_eq!(w_all.entries().len(), 1);
}

#[test]
fn script_e2e_live_child_process_protocol_lifecycle() {
    let tmp = TempDirGuard::new("script");
    let script_path = tmp.path().join("mode.ps1");

    // PowerShell script emits binary UTF-8 + \0 + \x1f records across ROFI_RETV states
    let ps_source = r#"
$retv = $env:ROFI_RETV
$arg0 = if ($args.Count -gt 0) { $args[0] } else { "" }
$out = [Console]::OpenStandardOutput()
function Emit([string]$s) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($s)
    $out.Write($bytes, 0, $bytes.Length)
}
$N = [char]0
$U = [char]0x1F
if ($retv -eq "0") {
    Emit "${N}prompt${U}ScriptTest`n"
    Emit "${N}message${U}InitMsg`n"
    Emit "${N}data${U}state42`n"
    Emit "${N}use-hot-keys${U}true`n"
    Emit "${N}active${U}0`n"
    Emit "${N}urgent${U}1`n"
    Emit "ItemOne${N}info${U}payload1`n"
    Emit "ItemTwo${N}info${U}payload2`n"
} elseif ($retv -eq "1") {
    Emit "${N}keep-selection${U}true`n"
    Emit "${N}keep-filter${U}true`n"
    Emit "Stage2:$arg0|$($env:ROFI_INFO)|$($env:ROFI_DATA)|$($env:ROFI_INPUT)`n"
} elseif ($retv -eq "10") {
    Emit "Hotkey10:$arg0`n"
} elseif ($retv -eq "2") {
    Emit "Custom2:$arg0`n"
} elseif ($retv -eq "3") {
    Emit "Deleted3:$arg0`n"
} elseif ($retv -eq "11" -or $arg0 -eq "switch") {
    Emit "${N}switch-mode${U}run`n"
} else {
    # Empty stdout -> Exit(0)
}
"#;
    fs::write(&script_path, ps_source).expect("write mode.ps1");

    let cfg = Config::default();
    let quoted_path = script_path.to_string_lossy();
    let cmd_spec = format!("powershell -NoProfile -ExecutionPolicy Bypass -File '{quoted_path}'");

    let mut script = ScriptMode::new("myscript".to_string(), cmd_spec, &cfg);
    script.init().expect("ROFI_RETV=0 init");

    assert_eq!(script.prompt_override().as_deref(), Some("ScriptTest"));
    assert_eq!(script.message().as_deref(), Some("InitMsg"));
    let e0 = script.entries();
    assert_eq!(e0.len(), 2);
    assert!(e0[0].active);
    assert!(e0[1].urgent);
    assert_eq!(e0[0].info.as_deref(), Some("payload1"));

    // ROFI_RETV=1 (AcceptEntry) receives entry, ROFI_INFO, ROFI_DATA, ROFI_INPUT & reload hints
    let s1 = script
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "ItemOne".to_string(),
                exit_code: 0,
            },
            "typed-query",
        )
        .expect("ROFI_RETV=1");
    assert_eq!(s1, ModeStep::Reload);
    assert!(script.reload_hints().keep_selection);
    assert!(script.reload_hints().keep_filter);
    assert_eq!(
        script.entries()[0].text,
        "Stage2:ItemOne|payload1|state42|typed-query"
    );

    // ROFI_RETV=10 (custom hotkey when use-hot-keys=true)
    let s10 = script
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "x".to_string(),
                exit_code: 10,
            },
            "q",
        )
        .expect("ROFI_RETV=10");
    assert_eq!(s10, ModeStep::Reload);
    assert!(script.entries()[0].text.starts_with("Hotkey10:"));

    // ROFI_RETV=2 (custom input)
    let s2 = script
        .handle_outcome(
            &UiOutcome::CustomInput {
                text: "mycustom".to_string(),
                exit_code: 0,
            },
            "mycustom",
        )
        .expect("ROFI_RETV=2");
    assert_eq!(s2, ModeStep::Reload);
    assert_eq!(script.entries()[0].text, "Custom2:mycustom");

    // ROFI_RETV=3 (delete entry)
    let s3 = script
        .handle_outcome(&UiOutcome::DeleteEntry { index: 0 }, "q")
        .expect("ROFI_RETV=3");
    assert_eq!(s3, ModeStep::Reload);
    assert_eq!(script.entries()[0].text, "Deleted3:Custom2:mycustom");

    // \0switch-mode\x1frun
    let s_sw = script
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "ignored".to_string(),
                exit_code: 11,
            },
            "q",
        )
        .expect("switch-mode");
    assert_eq!(s_sw, ModeStep::SwitchMode("run".to_string()));

    // Empty stdout terminates script session with Exit(0)
    let s_exit = script
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "done".to_string(),
                exit_code: 12,
            },
            "q",
        )
        .expect("empty exit");
    assert_eq!(s_exit, ModeStep::Exit(0));
}

#[test]
fn combi_e2e_prefix_formatting_bang_filtering_and_dispatch() {
    let tmp = TempDirGuard::new("combi");
    let bin_dir = tmp.path().join("bin");
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&bin_dir).expect("bin");
    fs::write(bin_dir.join("runtool.exe"), b"").expect("runtool");

    let cfg = Config {
        display_run: Some("RUN".to_string()),
        ..Config::default()
    };
    let run = Box::new(RunMode::new(&cfg).with_test_env(vec![bin_dir], false, cache_dir, true));
    let dm = Box::new(DmenuMode::from_bytes(
        &cfg,
        b"menu_alpha\nmenu_beta\n".to_vec(),
    ));

    let mut combi = CombiMode::from_sub_modes(&cfg, vec![run, dm]);
    combi.init().expect("combi init");

    let entries = combi.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].visible_label(), "RUN runtool.exe");
    assert_eq!(entries[1].visible_label(), "dmenu menu_alpha");
    assert_eq!(entries[2].visible_label(), "dmenu menu_beta");

    // LauncherState bang prefix `!dm beta` isolates dmenu entries and matches `beta`
    let mut state = LauncherState::new(&cfg, entries);
    state.insert_str("!dm beta");
    assert_eq!(state.filtered.len(), 1);
    assert_eq!(state.filtered[0], 2);

    // Accepting global index 2 dispatches to sub-mode `dmenu` index 1 -> Exit(0)
    let step = combi
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 2,
                text: "dmenu menu_beta".to_string(),
                exit_code: 0,
            },
            &state.query,
        )
        .expect("dispatch combi");
    assert_eq!(step, ModeStep::Exit(0));

    // Also verify run_modes_self_test initializes all 5 live Windows modes cleanly
    let report = run_modes_self_test(&Config::default()).expect("modes self-test");
    assert!(report.run_count > 0);
    assert!(report.drun_count > 0);
}

#[test]
fn filebrowser_e2e_navigation_sorting_hidden_toggle_and_open() {
    let tmp = TempDirGuard::new("fb");
    let root = tmp.path().join("browse");
    let sub = root.join("sub");
    let alt = root.join("alt");
    let cache = tmp.path().join("cache");
    fs::create_dir_all(&sub).expect("sub dir");
    fs::create_dir_all(&alt).expect("alt dir");
    fs::write(root.join("readme.md"), b"hello").expect("readme");
    fs::write(root.join(".hidden"), b"secret").expect("hidden");
    fs::write(sub.join("inside.txt"), b"inner").expect("inside");

    let cfg = Config::default();
    let mut fb = FileBrowserMode::new(&cfg).with_dir_and_cache(root.clone(), cache, true);
    fb.init().expect("fb init");

    // `..` + directories first + hidden excluded by default
    let labels: Vec<String> = fb
        .entries()
        .iter()
        .map(|e| e.visible_label().to_string())
        .collect();
    assert_eq!(labels, vec!["..", "alt/", "sub/", "readme.md"]);
    assert!(!fb.show_hidden());

    // DeleteEntry toggles show_hidden
    let tog = fb
        .handle_outcome(&UiOutcome::DeleteEntry { index: 0 }, "")
        .expect("toggle hidden");
    assert_eq!(tog, ModeStep::Reload);
    assert!(fb.show_hidden());
    let labels_hidden: Vec<String> = fb
        .entries()
        .iter()
        .map(|e| e.visible_label().to_string())
        .collect();
    assert!(labels_hidden.iter().any(|l| l == ".hidden"));

    // Accept `sub/` navigates into child directory
    let sub_idx = labels_hidden
        .iter()
        .position(|l| l == "sub/")
        .expect("sub idx");
    let nav = fb
        .handle_outcome(
            &UiOutcome::Accepted {
                index: sub_idx,
                text: "sub".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("enter sub");
    assert_eq!(nav, ModeStep::Reload);
    assert_eq!(fb.current_dir(), &sub);

    // Open `inside.txt` inside `sub/`
    let inside_idx = fb
        .entries()
        .iter()
        .position(|e| e.text == "inside.txt")
        .expect("inside idx");
    let open = fb
        .handle_outcome(
            &UiOutcome::Accepted {
                index: inside_idx,
                text: "inside.txt".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("open inside");
    assert_eq!(open, ModeStep::Exit(0));
    assert_eq!(
        fb.opened_paths(),
        &[sub.join("inside.txt").to_string_lossy().into_owned()]
    );

    // Navigate back up via `..` (`index: 0`), then CustomInput jump to `alt`
    let up = fb
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "..".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("go up");
    assert_eq!(up, ModeStep::Reload);
    assert_eq!(fb.current_dir(), &root);

    let jump = fb
        .handle_outcome(
            &UiOutcome::CustomInput {
                text: "alt".to_string(),
                exit_code: 0,
            },
            "alt",
        )
        .expect("jump alt");
    assert_eq!(jump, ModeStep::Reload);
    assert_eq!(fb.current_dir(), &alt);
}

#[test]
fn recursivebrowser_e2e_bfs_walk_and_filter_regex() {
    let tmp = TempDirGuard::new("rb");
    let root = tmp.path().join("tree");
    fs::create_dir_all(root.join("nested").join("deep")).expect("nested");
    fs::create_dir_all(root.join(".git")).expect(".git");
    fs::write(root.join("a.rs"), b"fn main(){}").expect("a.rs");
    fs::write(root.join("nested").join("b.log"), b"log").expect("b.log");
    fs::write(root.join("nested").join("deep").join("c.txt"), b"txt").expect("c.txt");
    fs::write(root.join(".git").join("config"), b"core").expect("git config");
    fs::write(root.join(".secret"), b"hidden").expect("dotfile");

    let cfg = Config {
        recursivebrowser_filter_regex: r"(^\..*|\.log$)".to_string(),
        ..Config::default()
    };
    let mut rb = RecursiveBrowserMode::new(&cfg).with_dir(root.clone(), 500, true);
    rb.init().expect("rb init");

    let names: Vec<String> = rb.entries().into_iter().map(|e| e.text).collect();
    assert_eq!(names.len(), 2);
    assert!(names.iter().any(|n| n.ends_with("a.rs")));
    assert!(names.iter().any(|n| n.ends_with("c.txt")));

    let step = rb
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: names[0].clone(),
                exit_code: 0,
            },
            "",
        )
        .expect("open recursive");
    assert_eq!(step, ModeStep::Exit(0));
    assert_eq!(rb.opened_paths().len(), 1);
}

#[test]
fn ssh_e2e_config_include_known_hosts_etc_hosts_and_history() {
    let tmp = TempDirGuard::new("ssh");
    let ssh_dir = tmp.path().join(".ssh");
    let conf_d = ssh_dir.join("conf.d");
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&conf_d).expect("conf.d");

    let extra_kh = ssh_dir.join("extra_known_hosts");
    fs::write(
        &extra_kh,
        "extra.internal ssh-ed25519 AAAA\n\
         |1|hashed|entry= ssh-ed25519 AAAA\n\
         [bastion.example.org]:2222,10.0.0.5 ssh-rsa AAAA\n",
    )
    .expect("extra_kh");

    let conf_path = ssh_dir.join("config");
    let extra_kh_str = extra_kh.to_string_lossy().replace('\\', "/");
    fs::write(
        &conf_path,
        format!(
            "Include conf.d/*.conf\n\
             UserKnownHostsFile {extra_kh_str}\n\
             Host prod-box prod # alias\n\
             Host *.wildcard.ignored\n"
        ),
    )
    .expect("ssh config");

    fs::write(conf_d.join("work.conf"), "Host build-srv\n").expect("work.conf");

    let known_hosts_path = ssh_dir.join("known_hosts");
    fs::write(&known_hosts_path, "legacy.local ssh-ed25519 AAAA\n").expect("known_hosts");

    let etc_hosts_path = tmp.path().join("hosts");
    fs::write(
        &etc_hosts_path,
        "# comment line\n127.0.0.1 localhost dev.local\n",
    )
    .expect("etc hosts");

    let cfg = Config {
        parse_hosts: true,
        parse_known_hosts: true,
        terminal: "cmd.exe /C".to_string(),
        ssh_command: "{terminal} {ssh-client} {host} [-p {port}]".to_string(),
        ..Config::default()
    };

    let mut ssh = SshMode::new(&cfg).with_test_paths(
        Some(conf_path.clone()),
        Some(known_hosts_path.clone()),
        Some(etc_hosts_path.clone()),
        cache_dir.clone(),
        true,
    );
    ssh.init().expect("ssh init");

    let entries = ssh.entries();
    let hosts: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
    assert!(hosts.contains(&"build-srv"));
    assert!(hosts.contains(&"prod-box"));
    assert!(hosts.contains(&"legacy.local"));
    assert!(hosts.contains(&"extra.internal"));
    assert!(hosts.contains(&"bastion.example.org"));
    assert!(hosts.contains(&"dev.local"));
    assert!(!hosts.iter().any(|h| h.contains('*') || h.starts_with('|')));

    // Port 2222 entry keeps `-p 2222`; port 0 entry omits `-p`
    let idx_bastion = hosts
        .iter()
        .position(|h| *h == "bastion.example.org")
        .expect("bastion idx");
    let step = ssh
        .handle_outcome(
            &UiOutcome::Accepted {
                index: idx_bastion,
                text: "bastion.example.org".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("exec bastion");
    assert_eq!(step, ModeStep::Exit(0));
    assert_eq!(
        ssh.launched_commands()[0],
        vec![
            "cmd.exe".to_string(),
            "/C".to_string(),
            "ssh".to_string(),
            "bastion.example.org".to_string(),
            "-p".to_string(),
            "2222".to_string(),
        ]
    );

    let idx_prod = hosts
        .iter()
        .position(|h| *h == "prod-box")
        .expect("prod idx");
    let _ = ssh
        .handle_outcome(
            &UiOutcome::Accepted {
                index: idx_prod,
                text: "prod-box".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("exec prod");
    assert_eq!(
        ssh.launched_commands()[1],
        vec![
            "cmd.exe".to_string(),
            "/C".to_string(),
            "ssh".to_string(),
            "prod-box".to_string(),
        ]
    );

    // On re-init, history hosts float to top with active=true, and DeleteEntry removes from history
    let mut ssh2 = SshMode::new(&cfg).with_test_paths(
        Some(conf_path),
        Some(known_hosts_path),
        Some(etc_hosts_path),
        cache_dir,
        true,
    );
    ssh2.init().expect("ssh2 init");
    let e2 = ssh2.entries();
    assert!(e2[0].active && e2[1].active);
    let removed_host = e2[0].text.clone();
    let del = ssh2
        .handle_outcome(&UiOutcome::DeleteEntry { index: 0 }, "")
        .expect("delete ssh history");
    assert_eq!(del, ModeStep::Reload);
    assert!(ssh2
        .entries()
        .iter()
        .find(|e| e.text == removed_host)
        .is_some_and(|e| !e.active));
}

#[test]
fn keys_e2e_lists_all_keybindings_and_exit_codes() {
    let cfg = Config::default();
    let mut keys = KeysMode::new(&cfg);
    keys.init().expect("keys init");

    let entries = keys.entries();
    assert_eq!(entries.len(), cfg.keybindings.entries().len());
    assert!(entries
        .iter()
        .any(|e| e.text.contains("kb-accept-entry") && e.text.contains("Return")));

    assert_eq!(
        keys.handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: entries[0].text.clone(),
                exit_code: custom_exit_code(5),
            },
            "",
        )
        .expect("custom code"),
        ModeStep::Exit(14)
    );
    assert_eq!(
        keys.handle_outcome(&UiOutcome::Cancelled, "")
            .expect("cancel"),
        ModeStep::Exit(1)
    );
}

#[test]
fn drun_populates_real_windows_icon_specs() {
    let apps = discover_windows_apps();
    assert!(!apps.is_empty(), "expected live Windows apps");
    let with_icons = apps.iter().filter(|a| a.icon.is_some()).count();
    assert!(
        with_icons > 0,
        "expected discovered Windows apps to carry icon specs"
    );

    let cfg = Config::default();
    let mut drun = DrunMode::new(&cfg);
    drun.init().expect("live drun init");
    let entries = drun.entries();
    assert!(
        entries.iter().any(|e| e.icon.is_some()),
        "expected drun ListEntry items to expose icon specs"
    );
}

#[test]
fn theme_mode_lists_themes_and_updates_config_rasi() {
    use literofi_modes::theme_picker::ThemeMode;

    let tmp = TempDirGuard::new("theme_mode");
    let themes_dir = tmp.path().join("themes");
    std::fs::create_dir_all(&themes_dir).expect("create themes");
    std::fs::write(
        themes_dir.join("KooL_style-4.rasi"),
        "window { width: 40%; }\n",
    )
    .expect("t1");
    std::fs::write(
        themes_dir.join("tokyonight.rasi"),
        "window { width: 640px; }\n",
    )
    .expect("t2");
    let cfg_path = tmp.path().join("config.rasi");
    std::fs::write(
        &cfg_path,
        "configuration {\n    show-icons: true;\n}\n@theme \"tokyonight\"\n",
    )
    .expect("write config");

    let mut mode = ThemeMode::with_paths(vec![themes_dir], cfg_path.clone());
    mode.init().expect("init theme mode");
    let entries = mode.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].text, "KooL_style-4");
    assert_eq!(entries[1].text, "tokyonight");

    let step = mode
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "KooL_style-4".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("apply theme");
    assert_eq!(step, ModeStep::Exit(0));

    let updated = std::fs::read_to_string(&cfg_path).expect("read config");
    assert!(updated.contains("@theme \"KooL_style-4\""));
    assert!(!updated.contains("@theme \"tokyonight\""));
}

#[test]
fn theme_mode_live_preview_enter_then_alt_a_saves() {
    use literofi_modes::theme_picker::ThemeMode;

    let tmp = TempDirGuard::new("theme_mode_preview");
    let themes_dir = tmp.path().join("themes");
    std::fs::create_dir_all(&themes_dir).expect("create themes");
    let t1_path = themes_dir.join("catppuccin-mocha.rasi");
    std::fs::write(&t1_path, "window { width: 50%; }\n").expect("t1");
    let cfg_path = tmp.path().join("config.rasi");
    std::fs::write(&cfg_path, "@theme \"nord\"\n").expect("write config");

    let mut mode = ThemeMode::with_paths_preview(vec![themes_dir], cfg_path.clone());
    mode.init().expect("init preview theme mode");

    // 1. Enter previews live without modifying config.rasi
    let step1 = mode
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "catppuccin-mocha".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("preview theme");
    assert_eq!(step1, ModeStep::PreviewTheme(t1_path));
    let still_old = std::fs::read_to_string(&cfg_path).expect("read config");
    assert!(still_old.contains("@theme \"nord\""));

    // 2. Alt+A / Shift+Enter (`AcceptedAlt`) persists selection and exits
    let step2 = mode
        .handle_outcome(
            &UiOutcome::AcceptedAlt {
                index: 0,
                text: "catppuccin-mocha".to_string(),
            },
            "",
        )
        .expect("save theme");
    assert_eq!(step2, ModeStep::Exit(0));
    let saved = std::fs::read_to_string(&cfg_path).expect("read saved config");
    assert!(saved.contains("@theme \"catppuccin-mocha\""));
}

#[test]
fn color_mode_previews_reverts_and_saves_wallust_palette() {
    use literofi_modes::color_picker::ColorMode;

    let tmp = TempDirGuard::new("color_mode");
    let wallust_dir = tmp.path().join("wallust");
    std::fs::create_dir_all(&wallust_dir).expect("wallust dir");
    let wallust_file = wallust_dir.join("colors-rofi.rasi");
    std::fs::write(&wallust_file, "* { background: #000000; }\n").expect("orig");

    let mut mode = ColorMode::with_root(tmp.path().to_path_buf());
    mode.init().expect("init color mode");
    assert!(mode.entries().len() >= 10);

    // Enter previews live by updating wallust/colors-rofi.rasi and returning ReloadTheme
    let step1 = mode
        .handle_outcome(
            &UiOutcome::Accepted {
                index: 0,
                text: "Catppuccin Mocha".to_string(),
                exit_code: 0,
            },
            "",
        )
        .expect("preview palette");
    assert_eq!(step1, ModeStep::ReloadTheme);
    let preview_txt = std::fs::read_to_string(&wallust_file).expect("read preview");
    assert!(preview_txt.contains("#1e1e2e"));

    // Esc reverts original file
    let step2 = mode
        .handle_outcome(&UiOutcome::Cancelled, "")
        .expect("cancel palette");
    assert_eq!(step2, ModeStep::Exit(1));
    let reverted = std::fs::read_to_string(&wallust_file).expect("read reverted");
    assert!(reverted.contains("#000000"));
}

#[test]
fn color_mode_loads_base16_rgba_palettes_from_colors_dir() {
    use literofi_modes::color_picker::ColorMode;

    let tmp = TempDirGuard::new("color_mode_base16");
    let colors_dir = tmp.path().join("colors");
    std::fs::create_dir_all(&colors_dir).expect("colors dir");
    std::fs::write(
        colors_dir.join("base16-oxocarbon-dark.rasi"),
        "* {\n  background: rgba ( 22, 22, 22, 100 % );\n  lightbg: rgba ( 38, 38, 38, 100 % );\n  foreground: rgba ( 255, 255, 255, 100 % );\n  blue: rgba ( 120, 169, 255, 100 % );\n  red: rgba ( 238, 83, 150, 100 % );\n}\n",
    )
    .expect("write base16");

    let mut mode = ColorMode::with_root(tmp.path().to_path_buf());
    mode.init().expect("init color mode");
    let entries = mode.entries();
    let idx = entries
        .iter()
        .position(|e| e.text == "oxocarbon dark")
        .expect("base16 oxocarbon dark should be discovered");

    let step = mode
        .handle_outcome(
            &UiOutcome::AcceptedAlt {
                index: idx,
                text: "oxocarbon dark".to_string(),
            },
            "",
        )
        .expect("save base16 palette");
    assert_eq!(step, ModeStep::Exit(0));
    let wallust = std::fs::read_to_string(tmp.path().join("wallust").join("colors-rofi.rasi"))
        .expect("read wallust");
    assert!(wallust.contains("#161616"));
    assert!(wallust.contains("#78a9ff"));
}
