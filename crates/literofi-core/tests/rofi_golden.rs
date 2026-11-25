use literofi_core::cli::{
    parse_argv, parse_char, parse_command_template, string_replace_if_exists, Argv,
};
use literofi_core::config::{MatchingMethod, SortingMethod, WindowLocation};
use literofi_core::history::{history_get_list, history_remove, history_set, HistoryConfig};
use literofi_core::keyb::parse_chord_list;
use literofi_core::matcher::{
    fuzzy_score_fzf_v2, fuzzy_score_rofi, fzf_v2_sort_key, levenshtein, parse_case_sensitivity,
    rofi_force_utf8, rofi_latin_to_utf8, token_match, token_match_ranges, tokenize, MatchOptions,
    FZF_V2_MIN_SCORE, ROFI_FUZZY_MIN_SCORE,
};
use literofi_core::mode::custom_exit_code;
use literofi_core::protocol::{
    format_output_line, parse_dmenu_script_stream, parse_ranges, DmenuScriptEntry,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_history_path(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "literofi_hist_{tag}_{}_{nanos}.txt",
        std::process::id()
    ))
}

#[test]
fn golden_tokenize_normal_glob_fuzzy_regex_and_prefix() {
    // helper-tokenize.c: empty input
    let empty = tokenize("", &MatchOptions::default());
    assert!(empty.is_empty());
    assert!(token_match(&empty, "anything", false));

    // Normal single CI
    let ci_normal = MatchOptions {
        method: MatchingMethod::Normal,
        case_sensitive: false,
        ..Default::default()
    };
    let tok = tokenize("noot", &ci_normal);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(!token_match(&tok, "nooaap mies", false));
    assert!(token_match(&tok, "nootap mies", false));
    assert!(token_match(&tok, "aap Noot mies", false));
    assert!(!token_match(&tok, "Nooaap mies", false));
    assert!(token_match(&tok, "noOTap mies", false));

    // Normal single CS
    let cs_normal = MatchOptions {
        case_sensitive: true,
        ..ci_normal.clone()
    };
    let tok = tokenize("noot", &cs_normal);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(!token_match(&tok, "nooaap mies", false));
    assert!(token_match(&tok, "nootap mies", false));
    assert!(!token_match(&tok, "aap Noot mies", false));
    assert!(!token_match(&tok, "Nooaap mies", false));
    assert!(!token_match(&tok, "noOTap mies", false));

    // Normal multiple CI & negate
    let tok = tokenize("no ot", &ci_normal);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(!token_match(&tok, "nooaap mies", false));
    assert!(token_match(&tok, "nootap mies", false));
    assert!(token_match(&tok, "noap miesot", false));

    let tok = tokenize("-noot", &ci_normal);
    assert!(!token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "nooaap mies", false));
    assert!(!token_match(&tok, "nootap mies", false));
    assert!(token_match(&tok, "noap miesot", false));

    let tok = tokenize("-noot aap", &ci_normal);
    assert!(!token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "nooaap mies", false));
    assert!(!token_match(&tok, "nootap mies", false));
    assert!(!token_match(&tok, "noap miesot", false));

    // Glob CI / CS / multi / ? / *
    let ci_glob = MatchOptions {
        method: MatchingMethod::Glob,
        case_sensitive: false,
        ..Default::default()
    };
    let cs_glob = MatchOptions {
        case_sensitive: true,
        ..ci_glob.clone()
    };
    let tok = tokenize("noot", &ci_glob);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "aap Noot mies", false));

    let tok = tokenize("noot", &cs_glob);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap Noot mies", false));

    let tok = tokenize("no ot", &ci_glob);
    assert!(token_match(&tok, "noap miesot", false));

    let tok = tokenize("n?ot", &ci_glob);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "nootap mies", false));
    assert!(!token_match(&tok, "noap miesot", false));

    let tok = tokenize("n*ot", &ci_glob);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "nootap mies", false));
    assert!(token_match(&tok, "noap miesot", false));

    let tok = tokenize("n* ot", &ci_glob);
    assert!(token_match(&tok, "ot nap mies", false));

    // Fuzzy CI / CS / split / multi
    let ci_fuzzy = MatchOptions {
        method: MatchingMethod::Fuzzy,
        case_sensitive: false,
        ..Default::default()
    };
    let cs_fuzzy = MatchOptions {
        case_sensitive: true,
        ..ci_fuzzy.clone()
    };
    let tok = tokenize("noot", &ci_fuzzy);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "nooaap mies", false));
    assert!(token_match(&tok, "noOTap mies", false));

    let tok = tokenize("noot", &cs_fuzzy);
    assert!(!token_match(&tok, "aap Noot mies", false));

    let tok = tokenize("ont", &ci_fuzzy);
    assert!(!token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "nootap nmiest", false));

    let tok = tokenize("o n t", &ci_fuzzy);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "ot nap mies", false));

    // Regex
    let ci_regex = MatchOptions {
        method: MatchingMethod::Regex,
        case_sensitive: false,
        ..Default::default()
    };
    let tok = tokenize("n.?ot", &ci_regex);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(!token_match(&tok, "noap miesot", false));

    let tok = tokenize("n[oa]{2}t", &ci_regex);
    assert!(token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "noat miesot", false));
    assert!(!token_match(&tok, "noaat miesot", false));

    let tok = tokenize(r"^(aap|noap)\sMie.*", &ci_regex);
    assert!(!token_match(&tok, "aap noot mies", false));
    assert!(token_match(&tok, "aap mies", false));
    assert!(token_match(&tok, "noap miesot", false));
    assert!(!token_match(&tok, "ot nap mies", false));

    // Invalid regex falls back safely to literal match
    let tok = tokenize("[unclosed", &ci_regex);
    assert!(token_match(&tok, "foo [unclosed bar", false));
    assert!(!token_match(&tok, "foo unclosed bar", false));

    // Prefix matching
    let ci_prefix = MatchOptions {
        method: MatchingMethod::Prefix,
        case_sensitive: false,
        ..Default::default()
    };
    let tok = tokenize("fire", &ci_prefix);
    assert!(token_match(&tok, "Firefox Browser", false));
    assert!(!token_match(&tok, "backfire tool", false));

    // Normalized UTF-8 matching + highlight ranges
    let norm_opts = MatchOptions {
        method: MatchingMethod::Normal,
        normalize_match: true,
        ..Default::default()
    };
    let tok = tokenize("sifre cafe", &norm_opts);
    assert!(token_match(&tok, "Şifre Yöneticisi Café", true));
    let spans = token_match_ranges(&tokenize("no ot", &ci_normal), "aap noot mies", false);
    assert_eq!(spans, vec![(4, 6), (6, 8)]);
}

#[test]
fn golden_helper_test_chars_levenshtein_utf8_and_scorers() {
    // helper_parse_char
    assert_eq!(parse_char("\\n"), b'\n');
    assert_eq!(parse_char("\\a"), 0x07);
    assert_eq!(parse_char("\\b"), 0x08);
    assert_eq!(parse_char("\\t"), b'\t');
    assert_eq!(parse_char("\\v"), 0x0b);
    assert_eq!(parse_char("\\f"), 0x0c);
    assert_eq!(parse_char("\\r"), b'\r');
    assert_eq!(parse_char("\\\\"), b'\\');
    assert_eq!(parse_char("\\0"), 0);
    assert_eq!(parse_char("\\x77"), b'w');
    assert_eq!(parse_char("\\x0A"), b'\n');

    // levenshtein
    assert_eq!(levenshtein("aap", "aap", false), 0);
    assert_eq!(levenshtein("aap", "aap ", false), 1);
    assert_eq!(levenshtein("aap ", "aap", false), 1);
    assert_eq!(levenshtein("aap", "aap noot", false), 5);
    assert_eq!(levenshtein("aap", "noot aap", false), 5);
    assert_eq!(levenshtein("aap", "noot aap mies", false), 10);
    assert_eq!(levenshtein("noot aap mies", "aap", false), 10);
    assert_eq!(levenshtein("otp", "noot aap", false), 5);

    // latin1 & force_utf8
    assert_eq!(rofi_latin_to_utf8(&[0xA1, 0xB5]), "¡µ");
    assert_eq!(rofi_force_utf8(b"Valid utf8"), "Valid utf8");
    let invalid = b"Valid utf8 until \xc3\x28 we continue here";
    assert_eq!(
        rofi_force_utf8(invalid),
        "Valid utf8 until \u{FFFD}( we continue here"
    );

    // rofi_scorer_fuzzy_evaluate (helper-test.c passed length 12 => "aap noot mie")
    assert_eq!(
        fuzzy_score_rofi("aap noot mie", "aap noot mie", false),
        -605
    );
    assert_eq!(fuzzy_score_rofi("anm", "aap noot mie", false), -155);
    assert_eq!(
        fuzzy_score_rofi("blu", "aap noot mie", false),
        1_073_741_824
    );
    assert_eq!(fuzzy_score_rofi("Anm", "aap noot mie", true), 1_073_741_754);
    assert_eq!(fuzzy_score_rofi("Anm", "aap noot mie", false), -155);
    assert_eq!(
        fuzzy_score_rofi("aap noot mie", "Anm", false),
        1_073_741_824
    );
    assert_eq!(
        fuzzy_score_rofi("a", &"x".repeat(300), false),
        -ROFI_FUZZY_MIN_SCORE
    );

    // fzf_v2 exact scores
    assert_eq!(
        fuzzy_score_fzf_v2("aap noot mies", "aap noot mies", false, false),
        316
    );
    assert_eq!(fuzzy_score_fzf_v2("anm", "aap noot mies", false, false), 58);
    assert_eq!(
        fuzzy_score_fzf_v2("blu", "aap noot mies", false, false),
        FZF_V2_MIN_SCORE
    );
    assert_eq!(
        fuzzy_score_fzf_v2("Anm", "aap noot mies", true, false),
        FZF_V2_MIN_SCORE
    );
    assert_eq!(fuzzy_score_fzf_v2("Anm", "aap noot mies", false, false), 58);
    assert!(
        fuzzy_score_fzf_v2("wip", "hourglass not done; wip; in progress", false, false)
            > fuzzy_score_fzf_v2("wip", "locked with pen", false, false)
    );
    assert_eq!(
        fuzzy_score_fzf_v2("noot mies", "aap noot mies", false, false),
        228
    );
    assert_eq!(
        fuzzy_score_fzf_v2("aap mies", "aap noot mies", false, false),
        202
    );
    assert_eq!(
        fuzzy_score_fzf_v2("aap xyz", "aap noot mies", false, false),
        FZF_V2_MIN_SCORE
    );
    assert_eq!(
        fuzzy_score_fzf_v2("  aap   mies  ", "aap noot mies", false, false),
        202
    );

    // algo_test.go cases ported in helper-test.c:270-294
    assert_eq!(fuzzy_score_fzf_v2("oBZ", "fooBarbaz1", false, false), 49);
    assert_eq!(
        fuzzy_score_fzf_v2("rdoc", "/AutomatorDocument.icns", false, false),
        79
    );
    assert_eq!(
        fuzzy_score_fzf_v2("zshc", "/man1/zshcompctl.1", false, false),
        109
    );
    assert_eq!(
        fuzzy_score_fzf_v2("zshc", "/.oh-my-zsh/cache", false, false),
        102
    );
    assert_eq!(fuzzy_score_fzf_v2(".vimrc", "a.vimrc", false, false), 152);
    assert_eq!(fuzzy_score_fzf_v2("12356", "abc123 456", false, false), 108);
    assert_eq!(fuzzy_score_fzf_v2("fbb", "foo bar baz", false, false), 78);
    assert_eq!(fuzzy_score_fzf_v2("fbb", "foo/bar/baz", false, false), 76);
    assert_eq!(fuzzy_score_fzf_v2("fbb", "fooBarBaz", false, false), 74);
    assert_eq!(
        fuzzy_score_fzf_v2("foo-b", "xFoo-Bar Baz", false, false),
        124
    );
    assert_eq!(fuzzy_score_fzf_v2("FBB", "FooBarBaz", true, false), 74);
    assert_eq!(fuzzy_score_fzf_v2("oBz", "fooBarbaz", true, false), 49);
    assert_eq!(
        fuzzy_score_fzf_v2("oBZ", "fooBarbaz", true, false),
        FZF_V2_MIN_SCORE
    );

    // fzf_v2 sort_key invariants & 5-line ranking
    let key = |pat: &str, s: &str| fzf_v2_sort_key(fuzzy_score_fzf_v2(pat, s, false, false), s);
    assert!(key("apple", "apple") < key("apple", "baked apple pie"));
    assert!(key("fbb", "foo bar baz") < key("fbb", "fooBarBaz"));
    assert_eq!(key("ab", "ab cd"), key("ab", "ab ef"));
    assert_eq!(key("ab", "  ab  "), key("ab", "ab"));
    assert_eq!(key("ab", "ab你"), key("ab", "abx"));
    assert!(key("zz", "aa") == key("zz", "bb"));
    assert!(key("zz", "aa") < key("zz", "aaa"));
    assert!(key("ab", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxab") < key("ab", "a"));
    assert_eq!(key("zz", ""), i32::from(u16::MAX) << 15);

    let lines = [
        "baked apple pie",
        "green apple tart",
        "candy apple cake",
        "pine-apple juice",
        "apple",
    ];
    let expected_scores = [140, 140, 140, 128, 140];
    let mut keys = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let sc = fuzzy_score_fzf_v2("apple", line, false, false);
        assert_eq!(sc, expected_scores[i]);
        keys.push(fzf_v2_sort_key(sc, line));
    }
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&i| (keys[i], i));
    assert_eq!(order, vec![4, 0, 1, 2, 3]);

    // parse_case_sensitivity
    assert!(!parse_case_sensitivity("all lower case 你好", false, false));
    assert!(!parse_case_sensitivity(
        "not All lowEr Case 你好",
        false,
        false
    ));
    assert!(parse_case_sensitivity("all lower case 你好", true, false));
    assert!(parse_case_sensitivity(
        "not All lowEr Case 你好",
        true,
        false
    ));
    assert!(!parse_case_sensitivity("all lower case", true, true));
    assert!(parse_case_sensitivity("AAAAAAAAAAAA", true, true));
    assert!(!parse_case_sensitivity("all lower case 你好", false, true));
    assert!(parse_case_sensitivity(
        "not All lowEr Case 你好",
        false,
        true
    ));

    // helper_string_replace_if_exists
    assert_eq!(
        string_replace_if_exists(
            "{terminal} [-t {title} blub ]-e {cmd}",
            &[
                ("{cmd}", Some("aap")),
                ("{title}", Some("some title")),
                ("{terminal}", Some("rofi-sensible-terminal")),
            ]
        ),
        "rofi-sensible-terminal -t some title blub -e aap"
    );
    assert_eq!(
        string_replace_if_exists(
            "{terminal} [-t {title} blub ]-e {cmd}",
            &[
                ("{cmd}", Some("aap")),
                ("{terminal}", Some("rofi-sensible-terminal")),
            ]
        ),
        "rofi-sensible-terminal -e aap"
    );
    assert_eq!(
        string_replace_if_exists(
            "{name} [<span weight='light' size='small'><i>({category})</i></span>]",
            &[
                ("{name}", Some("Librecad")),
                ("{category}", Some("Desktop app"))
            ]
        ),
        "Librecad <span weight='light' size='small'><i>(Desktop app)</i></span>"
    );
    assert_eq!(
        string_replace_if_exists(
            "{name}[ <span weight='light' size='small'><i>({category})</i></span>]",
            &[("{name}", Some("Librecad"))]
        ),
        "Librecad"
    );
    assert_eq!(
        string_replace_if_exists(
            "{terminal} [{title} blub ]-e {cmd}",
            &[
                ("{cmd}", Some("aap")),
                ("{title}", Some("some title")),
                ("{terminal}", Some("rofi-sensible-terminal")),
            ]
        ),
        "rofi-sensible-terminal some title blub -e aap"
    );
    assert_eq!(
        string_replace_if_exists(
            "{terminal} [{title} blub ]-e {cmd}",
            &[
                ("{cmd}", Some("aap")),
                ("{title}", None),
                ("{terminal}", Some("rofi-sensible-terminal")),
            ]
        ),
        "rofi-sensible-terminal -e aap"
    );
}

#[test]
fn golden_cmdline_parser_and_argv_helpers() {
    let list = parse_command_template(
        "{host} {terminal} -e bash -c \"{ssh-client} {host}; echo '{terminal} {host}'\" -i -3 -u 4",
        "x-terminal-emulator",
        "ssh",
        &[("{host}", Some("chuck"))],
    )
    .expect("template should parse");

    assert_eq!(
        list,
        vec![
            "chuck",
            "x-terminal-emulator",
            "-e",
            "bash",
            "-c",
            "ssh chuck; echo 'x-terminal-emulator chuck'",
            "-i",
            "-3",
            "-u",
            "4",
        ]
    );

    let argv = Argv::new(&list);
    assert_eq!(argv.find_arg("-e"), Some(2));
    assert_eq!(argv.find_arg("-x"), None);
    assert_eq!(argv.find_arg_str("-e"), Some("bash"));
    assert_eq!(argv.find_arg_str("-x"), None);
    assert_eq!(argv.find_arg_uint("-u"), Some(4));
    assert_eq!(argv.find_arg_uint("-i"), Some(4_294_967_293));
    assert_eq!(argv.find_arg_int("-i"), Some(-3));
    assert_eq!(argv.find_arg_int("-x"), None);
}

#[test]
fn golden_history_roundtrip_and_lru_decay() {
    let path = temp_history_path("golden");
    let _ = fs::remove_file(&path);
    let cfg = HistoryConfig::default();

    assert_eq!(
        history_get_list(&path, false).expect("empty ok"),
        Vec::<String>::new()
    );

    history_set(&path, "aap", &cfg).expect("write aap");
    assert_eq!(history_get_list(&path, false).expect("read"), vec!["aap"]);

    history_remove(&path, "aap", false, cfg.max_size).expect("remove");
    assert_eq!(
        history_get_list(&path, false).expect("empty after remove"),
        Vec::<String>::new()
    );

    history_set(&path, "aap", &cfg).expect("write 1");
    history_set(&path, "aap", &cfg).expect("write 2");
    assert_eq!(
        history_get_list(&path, false).expect("deduped"),
        vec!["aap"]
    );

    for i in 2..26u32 {
        let item = format!("aap{i}");
        history_set(&path, &item, &cfg).expect("append");
        let items = history_get_list(&path, false).expect("list");
        assert_eq!(items.len(), i as usize);
    }

    history_set(&path, "blaat", &cfg).expect("26th caps at 25");
    let final_items = history_get_list(&path, false).expect("final");
    assert_eq!(final_items.len(), 25);
    for (idx, item) in final_items.iter().take(24).enumerate() {
        assert_eq!(item, &format!("aap{}", idx + 2));
    }

    // Ignored prefix does not record
    let cfg_ignored = HistoryConfig {
        ignored_prefixes: "secret; temp-".to_string(),
        ..Default::default()
    };
    history_set(&path, "temp-cmd", &cfg_ignored).expect("ignored ok");
    assert!(!history_get_list(&path, false)
        .expect("read")
        .contains(&"temp-cmd".to_string()));

    let _ = fs::remove_file(&path);
}

#[test]
fn protocol_dmenu_script_metadata_headers_and_output_format() {
    let raw = b"\x00prompt\x1fPick App\n\
\x00message\x1fChoose wisely\n\
\x00markup-rows\x1ftrue\n\
\x00urgent\x1f0,2-3\n\
\x00data\x1fctx-42\n\
\x00theme\x1felement { padding: 4px; }\n\
First Row\x00icon\x1ffirefox,web\x1fdisplay\x1fFirefox\x1fmeta\x1fbrowser\x1finfo\x1fid-1\x1furgent\x1ftrue\x1factive\x1ftrue\n\
Static Row\x00nonselectable\x1ftrue\x1fpermanent\x1ftrue\n";

    let (headers, entries) = parse_dmenu_script_stream(raw, b'\n');
    assert_eq!(headers.prompt.as_deref(), Some("Pick App"));
    assert_eq!(headers.message.as_deref(), Some("Choose wisely"));
    assert_eq!(headers.markup_rows, Some(true));
    assert_eq!(headers.urgent_ranges, vec![(0, 0), (2, 3)]);
    assert_eq!(headers.data.as_deref(), Some("ctx-42"));
    assert_eq!(headers.theme_snippets, vec!["element { padding: 4px; }"]);

    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0],
        DmenuScriptEntry {
            entry: "First Row".to_string(),
            display: Some("Firefox".to_string()),
            icons: vec!["firefox".to_string(), "web".to_string()],
            meta: Some("browser".to_string()),
            info: Some("id-1".to_string()),
            nonselectable: false,
            permanent: false,
            urgent: true,
            active: true,
        }
    );
    assert!(entries[1].nonselectable);
    assert!(entries[1].permanent);
    assert_eq!(parse_ranges("5-2, 7"), vec![(2, 5), (7, 7)]);

    // Format line test: i, d, s, p, q, f, F
    assert_eq!(format_output_line("i:d:s", "Alpha", 2, None), "2:3:Alpha");
    assert_eq!(
        format_output_line("p", "<b>A &amp; B</b>", 0, None),
        "A & B"
    );
    assert_eq!(
        format_output_line("q|f|F", "it's ok", 0, Some("a b")),
        "'it'\\''s ok'|a b|'a b'"
    );
}

#[test]
fn cli_flags_keybindings_dump_config_and_exit_codes() {
    let args: Vec<String> = [
        "literofi",
        "-show",
        "drun",
        "-no-sort",
        "-sorting-method",
        "fzf_v2",
        "-matching",
        "fuzzy",
        "-case-smart",
        "-theme-str",
        "window { width: 40%; }",
        "-theme-str",
        "listview { lines: 8; }",
        "-kb-row-up",
        "Up,k",
        "-location",
        "1",
        "-dump-config",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    let cfg = parse_argv(&args).expect("argv parses");
    assert_eq!(cfg.show.as_deref(), Some("drun"));
    assert!(!cfg.sort);
    assert_eq!(cfg.sorting_method, SortingMethod::FzfV2);
    assert_eq!(cfg.matching, MatchingMethod::Fuzzy);
    assert!(cfg.case_smart);
    assert_eq!(cfg.location, WindowLocation::North);
    assert_eq!(
        cfg.theme_str,
        vec![
            "window { width: 40%; }".to_string(),
            "listview { lines: 8; }".to_string()
        ]
    );
    assert_eq!(cfg.keybindings.get_by_name("kb-row-up"), Some("Up,k"));
    assert!(cfg.dump_config);

    let dumped = cfg.dump_config_rasi();
    assert!(dumped.starts_with("configuration {\n"));
    assert!(dumped.contains("\tmatching: \"fuzzy\";\n"));
    assert!(dumped.contains("\tsorting-method: \"fzf_v2\";\n"));
    assert!(dumped.contains("\tkb-row-up: \"Up,k\";\n"));

    let chords = parse_chord_list("Control+Alt+h,Return");
    assert_eq!(chords.len(), 2);
    assert!(chords[0].modifiers.control && chords[0].modifiers.alt);
    assert_eq!(chords[0].key, "h");
    assert_eq!(chords[1].key, "Return");

    assert_eq!(custom_exit_code(1), 10);
    assert_eq!(custom_exit_code(19), 28);
}
