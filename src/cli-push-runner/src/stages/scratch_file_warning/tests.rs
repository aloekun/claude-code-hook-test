use super::*;

#[test]
fn parse_file_list_basic() {
    let raw = "src/main.rs\nsrc/lib.rs\n";
    assert_eq!(
        parse_file_list_output(raw),
        vec!["src/main.rs", "src/lib.rs"]
    );
}

#[test]
fn parse_file_list_skips_empty_lines() {
    let raw = "src/main.rs\n\n\nsrc/lib.rs\n";
    assert_eq!(
        parse_file_list_output(raw),
        vec!["src/main.rs", "src/lib.rs"]
    );
}

#[test]
fn parse_file_list_trims_whitespace() {
    let raw = "  src/main.rs  \n\tsrc/lib.rs\t\n";
    assert_eq!(
        parse_file_list_output(raw),
        vec!["src/main.rs", "src/lib.rs"]
    );
}

#[test]
fn parse_file_list_empty_returns_empty() {
    assert_eq!(parse_file_list_output(""), Vec::<String>::new());
}

#[test]
fn extract_basename_forward_slash() {
    assert_eq!(extract_basename("src/foo/bar.rs"), "bar.rs");
}

#[test]
fn extract_basename_backslash() {
    assert_eq!(extract_basename(r"src\foo\bar.rs"), "bar.rs");
}

#[test]
fn extract_basename_no_separator() {
    assert_eq!(extract_basename("foo.rs"), "foo.rs");
}

#[test]
fn extract_basename_mixed_separators() {
    assert_eq!(extract_basename(r"src/foo\bar.rs"), "bar.rs");
    assert_eq!(extract_basename(r"src\foo/bar.rs"), "bar.rs");
}

#[test]
fn extract_basename_trailing_separator_returns_empty() {
    assert_eq!(extract_basename("src/foo/"), "");
}

#[test]
fn matches_glob_prefix_wildcard() {
    assert!(matches_glob("__foo", "__*"));
    assert!(matches_glob("__", "__*"));
    assert!(!matches_glob("foo__", "__*"));
    assert!(!matches_glob("_foo", "__*"));
}

#[test]
fn matches_glob_suffix_wildcard() {
    assert!(matches_glob("foo.tmp", "*.tmp"));
    assert!(matches_glob(".tmp", "*.tmp"));
    assert!(!matches_glob("foo.tmpx", "*.tmp"));
}

#[test]
fn matches_glob_prefix_and_suffix_wildcards() {
    assert!(matches_glob("_tmp_file.ps1", "_tmp_*"));
    assert!(matches_glob("__file.py", "__*.py"));
    assert!(!matches_glob("__file.ps1", "__*.py"));
}

#[test]
fn matches_glob_single_middle_wildcard() {
    assert!(matches_glob("foobazbar", "foo*bar"));
    assert!(matches_glob("foobar", "foo*bar"));
    assert!(!matches_glob("fooXY", "foo*bar"));
}

#[test]
fn matches_glob_three_part_pattern() {
    assert!(matches_glob("mytest_x.ps1", "*test*.ps1"));
    assert!(matches_glob("test.ps1", "*test*.ps1"));
    assert!(!matches_glob("foo.ps1", "*test*.ps1"));
}

#[test]
fn matches_glob_no_wildcard_exact() {
    assert!(matches_glob("foo", "foo"));
    assert!(!matches_glob("foo.bar", "foo"));
    assert!(!matches_glob("foo", "bar"));
}

#[test]
fn matches_glob_only_wildcard_matches_anything() {
    assert!(matches_glob("anything", "*"));
    assert!(matches_glob("", "*"));
}

#[test]
fn matches_glob_empty_pattern_exact() {
    assert!(matches_glob("", ""));
    assert!(!matches_glob("foo", ""));
}

#[test]
fn find_violations_detects_default_pattern() {
    let files = vec![
        "src/main.rs".to_string(),
        "__test.ps1".to_string(),
        "docs/__draft.md".to_string(),
        "src/__scratch.rs".to_string(),
    ];
    let patterns = vec!["__*".to_string()];
    let violations = find_violations(&files, &patterns);
    assert_eq!(
        violations,
        vec![
            "__test.ps1".to_string(),
            "docs/__draft.md".to_string(),
            "src/__scratch.rs".to_string()
        ]
    );
}

#[test]
fn find_violations_empty_when_no_match() {
    let files = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
    let patterns = vec!["__*".to_string()];
    assert!(find_violations(&files, &patterns).is_empty());
}

#[test]
fn find_violations_multiple_patterns() {
    let files = vec![
        "__test.ps1".to_string(),
        "_tmp_log.txt".to_string(),
        "src/main.rs".to_string(),
    ];
    let patterns = vec!["__*".to_string(), "_tmp_*".to_string()];
    let violations = find_violations(&files, &patterns);
    assert_eq!(violations.len(), 2);
    assert!(violations.contains(&"__test.ps1".to_string()));
    assert!(violations.contains(&"_tmp_log.txt".to_string()));
}

#[test]
fn find_violations_reports_file_only_once_when_matching_multiple_patterns() {
    let files = vec!["__test.tmp".to_string()];
    let patterns = vec!["__*".to_string(), "*.tmp".to_string()];
    let violations = find_violations(&files, &patterns);
    assert_eq!(violations.len(), 1);
}

#[test]
fn find_violations_matches_basename_in_any_subdirectory() {
    let files = vec![
        "subdir/__hidden.txt".to_string(),
        r"win\path\__hidden.txt".to_string(),
        "__top.txt".to_string(),
    ];
    let patterns = vec!["__*".to_string()];
    assert_eq!(find_violations(&files, &patterns).len(), 3);
}

#[test]
fn find_violations_ignores_dirname_prefix_match_when_basename_does_not_match() {
    let files = vec!["__src/main.rs".to_string()];
    let patterns = vec!["__*".to_string()];
    assert!(find_violations(&files, &patterns).is_empty());
}

#[test]
fn find_violations_detects_tmp_prefix_pattern() {
    let files = vec![
        "_tmp_dump.txt".to_string(),
        "_tmp_log.ps1".to_string(),
        "_tmp_script.py".to_string(),
        "src/main.rs".to_string(),
    ];
    let patterns = vec!["_tmp_*".to_string()];
    let violations = find_violations(&files, &patterns);
    assert_eq!(violations.len(), 3);
    assert!(violations.contains(&"_tmp_dump.txt".to_string()));
    assert!(violations.contains(&"_tmp_log.ps1".to_string()));
    assert!(violations.contains(&"_tmp_script.py".to_string()));
}

#[test]
fn find_violations_with_dunder_and_tmp_patterns_combined() {
    let files = vec![
        "__scratch.ps1".to_string(),
        "_tmp_dump.txt".to_string(),
        "src/main.rs".to_string(),
        "Cargo.toml".to_string(),
    ];
    let patterns = vec!["__*".to_string(), "_tmp_*".to_string()];
    let violations = find_violations(&files, &patterns);
    assert_eq!(violations.len(), 2);
    assert!(violations.contains(&"__scratch.ps1".to_string()));
    assert!(violations.contains(&"_tmp_dump.txt".to_string()));
}

#[test]
fn find_violations_tmp_pattern_does_not_match_underscore_only() {
    let files = vec!["_underscore_var.txt".to_string(), "_tmp.txt".to_string()];
    let patterns = vec!["_tmp_*".to_string()];
    let violations = find_violations(&files, &patterns);
    assert!(violations.is_empty());
}

#[test]
fn parse_override_env_truthy() {
    for v in [
        "1", "true", "TRUE", "yes", "YES", "on", "On", " true ", "\tyes\n",
    ] {
        assert!(parse_override_env(Some(v)), "'{}' should be truthy", v);
    }
}

#[test]
fn parse_override_env_falsy() {
    for v in ["0", "false", "no", "off", "", "   ", "maybe", "enable"] {
        assert!(!parse_override_env(Some(v)), "'{}' should be falsy", v);
    }
}

#[test]
fn parse_override_env_none_is_false() {
    assert!(!parse_override_env(None));
}

#[test]
fn effective_patterns_default_when_none() {
    let p = effective_patterns(None);
    assert_eq!(p, vec!["__*".to_string()]);
}

#[test]
fn effective_patterns_default_when_no_patterns_field() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: None,
        root_script_allowlist: None,
    };
    assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
}

#[test]
fn effective_patterns_default_when_empty_list() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec![]),
        root_script_allowlist: None,
    };
    assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
}

#[test]
fn effective_patterns_uses_config_when_provided() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec!["__*".to_string(), "_tmp_*".to_string()]),
        root_script_allowlist: None,
    };
    assert_eq!(
        effective_patterns(Some(&config)),
        vec!["__*".to_string(), "_tmp_*".to_string()]
    );
}

#[test]
fn effective_patterns_all_blank_falls_back_to_default() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec!["".to_string(), "  ".to_string(), "\t".to_string()]),
        root_script_allowlist: None,
    };
    assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
}

#[test]
fn effective_patterns_mixed_blank_and_valid_keeps_only_valid() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec![
            "".to_string(),
            "__*".to_string(),
            "   ".to_string(),
            "_tmp_*".to_string(),
        ]),
        root_script_allowlist: None,
    };
    assert_eq!(
        effective_patterns(Some(&config)),
        vec!["__*".to_string(), "_tmp_*".to_string()]
    );
}

#[test]
fn effective_patterns_whitespace_padded_is_trimmed() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec!["  __*  ".to_string()]),
        root_script_allowlist: None,
    };
    assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
}

#[test]
fn effective_patterns_mixed_filter_to_empty_falls_back_to_default() {
    let config = ScratchFileWarningConfig {
        enabled: Some(true),
        patterns: Some(vec!["  ".to_string(), "\t".to_string()]),
        root_script_allowlist: None,
    };
    assert_eq!(effective_patterns(Some(&config)), vec!["__*".to_string()]);
}

/// 順位 322 の incident 再現 (ADR-049 流儀): post-merge-feedback の takt run が
/// repo root に残した `analyze_transcript.py` を、**pattern 列挙に依存せず**検出する。
#[test]
fn root_script_detects_the_analyze_transcript_incident_file() {
    let files = vec![
        "analyze_transcript.py".to_string(),
        "src/main.rs".to_string(),
        "Cargo.toml".to_string(),
    ];
    let violations = root_script_violations(&files, &[]);
    assert_eq!(
        violations,
        vec!["analyze_transcript.py".to_string()],
        "deny-list に無い名前でも root 直下の .py なら検出すること (順位 322)"
    );
}

/// 既存 pattern では取りこぼしていたことを対比で固定する
/// (この 2 テストが揃って初めて「配置ベースにした意味」が担保される)。
#[test]
fn existing_patterns_miss_the_incident_file() {
    let files = vec!["analyze_transcript.py".to_string()];
    let patterns = vec!["__*".to_string(), "_tmp_*".to_string()];
    assert!(
        find_violations(&files, &patterns).is_empty(),
        "pattern 列挙では捕まらない (これが順位 322 の near-miss)"
    );
}

/// `scripts/` 配下など **root 直下でない**スクリプトは対象外 (誤検知しないこと)。
#[test]
fn root_script_ignores_non_root_scripts() {
    let files = vec![
        "scripts/deploy.sh".to_string(),
        "scripts/lint-workflows.mjs".to_string(),
        "src/tools/gen.py".to_string(),
        r"scripts\win.ps1".to_string(),
    ];
    assert!(
        root_script_violations(&files, &[]).is_empty(),
        "root 直下だけを見る (scripts/ 配下は正当な置き場所)"
    );
}

/// root 直下でも Rust / TS / 設定ファイル等は対象外。
#[test]
fn root_script_ignores_non_script_extensions() {
    let files = vec![
        "Cargo.toml".to_string(),
        "package.json".to_string(),
        "README.md".to_string(),
        "build.rs".to_string(),
        "noext".to_string(),
    ];
    assert!(root_script_violations(&files, &[]).is_empty());
}

/// allow-list に載せた root スクリプトは許可する (正当な追加への逃げ道)。
#[test]
fn root_script_allowlist_permits_listed_names() {
    let files = vec!["setup.py".to_string(), "leftover.py".to_string()];
    let allowlist = vec!["setup.py".to_string()];
    assert_eq!(
        root_script_violations(&files, &allowlist),
        vec!["leftover.py".to_string()]
    );
}

/// 拡張子の大文字小文字は問わない。
#[test]
fn root_script_extension_match_is_case_insensitive() {
    let files = vec!["Dump.PY".to_string(), "Run.Ps1".to_string()];
    assert_eq!(root_script_violations(&files, &[]).len(), 2);
}

/// 二層の合成: pattern 層と配置ベース層の両方が効き、重複は 1 件に畳まれる。
#[test]
fn all_violations_combines_both_layers_without_duplicates() {
    let files = vec![
        "analyze_transcript.py".to_string(),
        "_tmp_dump.txt".to_string(),
        "src/__scratch.rs".to_string(),
        "src/main.rs".to_string(),
    ];
    let patterns = vec!["__*".to_string(), "_tmp_*".to_string()];
    let violations = all_violations(&files, &patterns, &[]);
    assert_eq!(violations.len(), 3, "{:?}", violations);
    assert!(violations.contains(&"analyze_transcript.py".to_string()));
    assert!(violations.contains(&"_tmp_dump.txt".to_string()));
    assert!(violations.contains(&"src/__scratch.rs".to_string()));
}

#[test]
fn all_violations_reports_root_script_once_when_pattern_also_matches() {
    let files = vec!["_tmp_dump.py".to_string()];
    let patterns = vec!["_tmp_*".to_string()];
    assert_eq!(all_violations(&files, &patterns, &[]).len(), 1);
}
