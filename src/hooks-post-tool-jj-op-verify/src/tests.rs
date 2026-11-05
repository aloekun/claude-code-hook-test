//! hooks-post-tool-jj-op-verify のテスト (production は main.rs)。
//! ファイル長 800 行ガイドライン (順位 147) 遵守のため、順位 282 の検出対象追加にあたって
//! test mod を切り出した。

use super::*;

#[test]
fn detects_jj_new() {
    let op = detect_last_mutating_jj_op("jj new -m 'feat: x'").unwrap();
    assert_eq!(op.verb, "new");
    assert_eq!(op.expected_op_keyword, "new empty commit");
}

#[test]
fn detects_last_op_in_compound_command() {
    let op =
        detect_last_mutating_jj_op("jj describe -m x && jj new -m y 2>&1 | head -3").unwrap();
    assert_eq!(op.verb, "new", "複合コマンドでは最後の変更系操作を検証する");
}

#[test]
fn detects_bookmark_create_but_not_list() {
    assert!(detect_last_mutating_jj_op("jj bookmark create feat/x -r @").is_some());
    assert!(detect_last_mutating_jj_op("jj bookmark list").is_none());
}

#[test]
fn ignores_read_only_and_boundary_commands() {
    assert!(detect_last_mutating_jj_op("jj log -r @ --no-graph").is_none());
    assert!(detect_last_mutating_jj_op("jj op log --limit 1").is_none());
    assert!(detect_last_mutating_jj_op("jj st").is_none());
    assert!(
        detect_last_mutating_jj_op("jj git fetch").is_none(),
        "fetch は Nothing changed で op を作らない正当ケースがあるため対象外"
    );
    assert!(detect_last_mutating_jj_op("jj git push -b feat/x").is_none());
    assert!(detect_last_mutating_jj_op("cargo test && pnpm lint").is_none());
}

#[test]
fn op_match_is_case_insensitive_prefix_of_the_description() {
    assert!(op_matches_expectation(
        "d2e4a39cd26c describe commit d856d3b5",
        "describe commit"
    ));
    assert!(op_matches_expectation(
        "d2e4a39cd26c Describe Commit d856d3b5",
        "describe commit"
    ));
    assert!(!op_matches_expectation(
        "f53cbee0d008 snapshot working copy",
        "new empty commit"
    ));
}

/// **untrack の op は track の記録として数えない** (順位 282)。部分一致だと
/// `untrack remote bookmark` が `track remote bookmark` を含んで一致してしまう。
#[test]
fn an_untrack_op_is_not_mistaken_for_a_track_op() {
    assert!(!op_matches_expectation(
        "f385d3a533b4 untrack remote bookmark rb@origin",
        "track remote bookmark"
    ));
    assert!(op_matches_expectation(
        "c972ca44014b track remote bookmark rb@origin",
        "track remote bookmark"
    ));
}

/// 順位 282 で加えた操作。op の行は jj 0.42 の実機で記録したもの (2026-10-07)。
/// 検出 → キーワード → 実機の op 行との一致、を 1 本の表で押さえる。
#[test]
fn added_verbs_match_the_op_descriptions_recorded_on_jj_0_42() {
    let cases = [
        ("jj restore g.txt", "restore", "c9b447cb4c02 restore into commit 4957cfbce13d"),
        ("jj split g.txt -m part", "split", "40c091b98ffb split commit 04184a9abaa7"),
        (
            "jj undo",
            "undo",
            "10315aacc828 undo: restore to operation c29450c43dfe",
        ),
        (
            "jj bookmark move bm --to @",
            "bookmark move",
            "486427ae0ded point bookmark bm to commit 04184a9abaa7",
        ),
        (
            "jj bookmark track rb@origin",
            "bookmark track",
            "c972ca44014b track remote bookmark rb@origin",
        ),
        (
            "jj bookmark untrack rb@origin",
            "bookmark untrack",
            "f385d3a533b4 untrack remote bookmark rb@origin",
        ),
    ];
    for (command, verb, op_line) in cases {
        let op = detect_last_mutating_jj_op(command)
            .unwrap_or_else(|| panic!("{command} が検出されない"));
        assert_eq!(op.verb, verb, "{command}");
        assert!(
            op_matches_expectation(op_line, op.expected_op_keyword),
            "{command}: キーワード {:?} が実機の op 行 {op_line:?} に一致しない",
            op.expected_op_keyword
        );
    }
}

/// 変更の無い `jj restore` / track 済みの `jj bookmark track` は `Nothing changed.` を出し、
/// op を作らない (jj 0.42 実測)。この出力を見たら照合しない。
#[test]
fn nothing_changed_in_either_stream_is_reported_as_no_change() {
    let in_stderr = serde_json::json!({"stdout": "", "stderr": "Nothing changed.\n"});
    let in_stdout = serde_json::json!({"stdout": "Nothing changed.", "stderr": ""});
    assert!(reported_no_change(Some(&in_stderr)));
    assert!(reported_no_change(Some(&in_stdout)));
}

/// 照合を省くのは jj のその 1 行が出たときだけ。無い・形が違うときは従来どおり照合する。
#[test]
fn other_or_missing_output_still_gets_verified() {
    let changed = serde_json::json!({
        "stdout": "",
        "stderr": "Started tracking 1 remote bookmarks.",
    });
    let mentioned_in_prose = serde_json::json!({
        "stdout": "echo: Nothing changed. was expected",
        "stderr": "",
    });
    let unexpected_shape = serde_json::json!("Nothing changed.");
    assert!(!reported_no_change(Some(&changed)));
    assert!(!reported_no_change(Some(&mentioned_in_prose)));
    assert!(!reported_no_change(Some(&unexpected_shape)));
    assert!(!reported_no_change(None));
}

/// `jj op restore` / `jj op revert` は `op` サブコマンドなので、`jj restore` として検出しない。
/// 順位 597 からは `op` の操作として、実測した自分の文言で検出する。
#[test]
fn op_subcommands_are_not_mistaken_for_restore() {
    let restore = detect_last_mutating_jj_op("jj op restore c29450c43dfe").unwrap();
    assert_eq!(restore.verb, "op restore");
    assert_eq!(restore.expected_op_keyword, "restore to operation");
    let revert = detect_last_mutating_jj_op("jj op revert c29450c43dfe").unwrap();
    assert_eq!(revert.verb, "op revert");
    assert_eq!(revert.expected_op_keyword, "revert operation");
}

fn ops(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| (*l).to_string()).collect()
}

/// 受け入れ基準: 操作に対応する op が無い場合に「operation not recorded」警告を出す。
///
/// **付随 op ではない別の操作**が先頭に来ている状態 = 本物の未記録。
#[test]
fn decide_context_warns_when_operation_not_recorded() {
    let verdict = decide_context(
        "jj new -m 'x'",
        &ops(&["f53cbee0d008 describe commit 75b52ec7"]),
    )
    .unwrap();
    assert!(matches!(verdict, Verdict::NotRecorded(_)));
    assert!(verdict.message().contains("WARNING: operation not recorded"));
    assert!(verdict.message().contains("jj op log"));
}

#[test]
fn decide_context_confirms_recorded_operation() {
    let verdict =
        decide_context("jj new -m 'x'", &ops(&["02911d7f8d4b new empty commit"])).unwrap();
    assert!(matches!(verdict, Verdict::Recorded(_)));
    assert!(verdict.message().starts_with("[jj-op-verify] OK"));
}

#[test]
fn decide_context_none_for_non_mutating_command() {
    assert!(decide_context("cargo test", &ops(&["abc op"])).is_none());
}

#[test]
fn decide_context_none_when_op_log_unavailable() {
    assert!(
        decide_context("jj new -m 'x'", &[]).is_none(),
        "fail-open: jj 不在 / timeout では警告を出さない (助言層)"
    );
}


/// **実観測の再現 1** (`jj describe -m ... && pnpm push`)。
/// push の op が先頭を占めても、その下の `describe commit` と照合して OK を出す。
#[test]
fn a_push_op_at_the_head_does_not_hide_the_recorded_describe() {
    let verdict = decide_context(
        "jj describe -m 'msg' && pnpm push",
        &ops(&[
            "c8d83850 push bookmark fix/foo to git remote origin",
            "02edb0ec describe commit 75b52ec7",
        ]),
    )
    .unwrap();
    assert!(
        matches!(verdict, Verdict::Recorded(_)),
        "{}",
        verdict.message()
    );
}

/// **実観測の再現 2** (`jj bookmark forget ... && jj git fetch`)。
/// fetch と snapshot が連続しても、その下まで読み飛ばす。
#[test]
fn consecutive_incidental_ops_are_skipped() {
    let verdict = decide_context(
        "jj bookmark forget old && jj git fetch",
        &ops(&[
            "aaaaaaaa snapshot working copy",
            "bbbbbbbb fetch from git remote(s) origin",
            "cccccccc snapshot working copy",
            "dddddddd forget bookmark old",
        ]),
    )
    .unwrap();
    assert!(
        matches!(verdict, Verdict::Recorded(_)),
        "{}",
        verdict.message()
    );
}

/// **偽陰性を作らない側**: 最初の非付随 op で止まる。その先に一致する op があっても
/// 探しに行かない (探すと「本当に落ちた操作」を過去の同種 op で隠す)。
#[test]
fn the_search_stops_at_the_first_non_incidental_op() {
    let verdict = decide_context(
        "jj new",
        &ops(&[
            "aaaaaaaa snapshot working copy",
            "bbbbbbbb describe commit 75b52ec7",
            "cccccccc new empty commit",
        ]),
    )
    .unwrap();
    assert!(
        matches!(verdict, Verdict::NotRecorded(_)),
        "2 件目の describe で止まるべき: {}",
        verdict.message()
    );
}

/// 付随 op しか無ければ照合できない = 無出力 (fail-open)。
#[test]
fn only_incidental_ops_yields_no_verdict() {
    assert!(decide_context(
        "jj new",
        &ops(&["aaaaaaaa snapshot working copy", "bbbbbbbb import git refs"])
    )
    .is_none());
}

#[test]
fn incidental_prefixes_are_matched_on_the_description_not_the_id() {
    assert!(is_incidental_op("aaaaaaaa snapshot working copy"));
    assert!(is_incidental_op("bbbbbbbb fetch from git remote(s) origin"));
    assert!(is_incidental_op("cccccccc push bookmark foo to git remote origin"));
    assert!(is_incidental_op("dddddddd push all bookmarks to git remote origin"));
    assert!(is_incidental_op("eeeeeeee import git refs"));
}

/// 変更系 op は付随 op に数えない (数えると照合対象が消える)。
#[test]
fn mutating_ops_are_not_incidental() {
    assert!(!is_incidental_op("aaaaaaaa new empty commit"));
    assert!(!is_incidental_op("bbbbbbbb describe commit 75b52ec7"));
    assert!(!is_incidental_op("cccccccc create bookmark foo pointing to commit 1"));
}

/// **id に付随 op の文言が含まれても誤判定しない** — 照合は description 側で行う。
#[test]
fn an_id_shaped_line_without_a_description_is_not_incidental() {
    assert!(!is_incidental_op("snapshot"));
}

#[test]
fn blank_lines_in_the_op_log_are_ignored() {
    let log = ops(&["", "   ", "aaaaaaaa new empty commit"]);
    let selected = select_op_for_matching(&log);
    assert_eq!(selected.map(String::as_str), Some("aaaaaaaa new empty commit"));
}

#[test]
fn verify_enabled_defaults_off_and_reads_config() {
    assert!(!verify_enabled(""), "section 不在は OFF (ADR-039 § 1)");
    assert!(!verify_enabled("[post_tool_use.jj_op_verify]\n"));
    assert!(!verify_enabled(
        "[post_tool_use.jj_op_verify]\nenabled = false\n"
    ));
    assert!(verify_enabled(
        "[post_tool_use.jj_op_verify]\nenabled = true\n"
    ));
    assert!(!verify_enabled("not toml ["), "パース失敗は OFF (fail-open)");
}

/// 順位 476 の回帰テスト・方向 1 (誤検知しない): commit message 本文に別の jj サブコマンド名
/// が書かれていても、quote 内は 1 トークンに畳まれるため実コマンドを上書きしない。
#[test]
fn quoted_commit_message_containing_jj_keyword_is_not_misdetected() {
    let op = detect_last_mutating_jj_op(
        r#"jj describe -m "note: mention jj new keyword here""#,
    )
    .unwrap();
    assert_eq!(
        op.verb, "describe",
        "quote 内の 'jj new' は無視され、実コマンド 'jj describe' を検出する"
    );
}

/// 順位 476 の回帰テスト・方向 2 (正しく検出する): quote を含まない直接コマンドは
/// 従来どおり検出される。
#[test]
fn direct_command_without_quotes_is_still_detected() {
    let op = detect_last_mutating_jj_op("jj abandon -r x").unwrap();
    assert_eq!(op.verb, "abandon");
}

/// 順位 476 の回帰テスト・方向 3 (複合コマンドで最後の操作を採る): quote 内の message を
/// 含む先行コマンドがあっても、複合コマンドの最後の変更系操作を正しく検出する。
#[test]
fn compound_command_with_quoted_message_still_detects_last_op() {
    let op =
        detect_last_mutating_jj_op(r#"jj describe -m "msg" && jj abandon -r x"#).unwrap();
    assert_eq!(op.verb, "abandon");
}

/// 順位 476 の回帰テスト・方向 4 (PR #494 CodeRabbit 指摘): 二重引用符の中の `\"` は
/// 終端ではない。escape を解釈しないと message 本文の `jj new` が独立コマンドとして
/// 検出され、実際の `describe` に対して「operation not recorded」の誤警告が出る。
#[test]
fn escaped_quote_inside_a_double_quoted_message_does_not_end_the_quote() {
    let op = detect_last_mutating_jj_op(r#"jj describe -m "note: \" jj new \" here""#)
        .expect("describe が検出されること");
    assert_eq!(
        op.verb, "describe",
        "escape された quote は終端ではなく、message 内の 'jj new' は無視される"
    );
}

/// 同じ入力を token 単位でも固定する。`jj new` が独立 token として現れないこと。
#[test]
fn escaped_quote_keeps_the_message_in_a_single_token() {
    let tokens = tokenize_respecting_quotes(r#"jj describe -m "a \" jj new \" b""#);
    assert_eq!(
        tokens,
        vec!["jj", "describe", "-m", r#"a " jj new " b"#],
        "message 全体が 1 token に畳まれること"
    );
}

/// **単一引用符では backslash を escape にしない** (POSIX)。ここで解釈すると
/// 閉じ quote を読み飛ばし、二重引用符の修正が逆向きの壊れ方を生む。
#[test]
fn backslash_is_literal_inside_single_quotes() {
    let tokens = tokenize_respecting_quotes(r#"jj describe -m 'a \' && jj new"#);
    assert_eq!(
        tokens,
        vec!["jj", "describe", "-m", r#"a \"#, "&&", "jj", "new"],
        "単一引用符は backslash の直後でも閉じる"
    );
}

/// 閉じない二重引用符は終端まで 1 token (fail-open)。escape 追加後も維持する。
#[test]
fn unterminated_double_quote_still_folds_to_the_end() {
    let tokens = tokenize_respecting_quotes(r#"jj describe -m "a \" jj new"#);
    assert_eq!(tokens, vec!["jj", "describe", "-m", r#"a " jj new"#]);
}

#[test]
fn tokenization_requires_exact_jj_token_no_substring_match() {
    assert!(
        detect_last_mutating_jj_op("jjnew -m 'x'").is_none(),
        "'jjnew' は単一 token であり 'jj' と完全一致しないため検出されない"
    );
}

#[test]
fn tokenization_requires_exact_jj_token_trailing_punctuation_breaks_match() {
    assert!(
        detect_last_mutating_jj_op("see jj, new commit").is_none(),
        "'jj,' はカンマ付きのため 'jj' と完全一致せず検出されない"
    );
}

#[test]
fn hook_input_parses_bash_payload() {
    let input: HookInput = serde_json::from_str(
        r#"{"tool_name":"Bash","tool_input":{"command":"jj new -m 'x'"}}"#,
    )
    .unwrap();
    assert_eq!(input.tool_input.unwrap().command.unwrap(), "jj new -m 'x'");
}

/// 順位 536 (#494 の採用分): quote 内の文言と破壊的な bookmark 操作の組み合わせ。
/// message 本文の `jj new` は無視し、最後の `bookmark delete` を検出する (既存の abandon 版と同じ形)。
#[test]
fn compound_command_with_quoted_message_detects_a_trailing_bookmark_delete() {
    let op = detect_last_mutating_jj_op(r#"jj describe -m "then jj new" && jj bookmark delete old"#)
        .unwrap();
    assert_eq!(op.verb, "bookmark delete");
    assert_eq!(op.expected_op_keyword, "delete bookmark");
}

/// 順位 536: `cd` で作業ディレクトリを変えたコマンドは、hook の cwd とは別の op log を
/// 操作したかもしれないので照合しない。区切りの書き方 (`;` の付着 / `&&` / サブシェル) を問わない。
#[test]
fn a_command_that_changes_directory_is_not_verified() {
    for command in [
        "cd /c/work/other; jj new -m x",
        "cd /c/work/other && jj describe -m x",
        "echo start && cd ../other && jj new -m x",
        "(cd repo && jj abandon -r x)",
        "pushd repo && jj new -m x",
        "cd repo\njj new -m x",
    ] {
        assert!(skips_verification(command, false), "照合しないこと: {command}");
    }
}

/// 順位 536: `jj -R` / `--repository` で別のリポジトリを操作したコマンドも照合しない。
#[test]
fn a_jj_command_with_an_explicit_repository_is_not_verified() {
    for command in [
        "jj -R ../other new -m x",
        "jj new -m x -R ../other",
        "jj --repository ../other describe -m x",
        "jj --repository=../other describe -m x",
        "jj -R../other bookmark delete old",
    ] {
        assert!(skips_verification(command, false), "照合しないこと: {command}");
    }
}

/// 順位 536: background で起動したコマンドは、hook が呼ばれた時点でまだ終わっていない。
/// 2026-10-20 に観測した誤警告の入力 (`jj describe --stdin < msg && pnpm push`) をそのまま使う。
#[test]
fn a_command_run_in_background_is_not_verified() {
    let input: HookInput = serde_json::from_str(
        r#"{"tool_name":"Bash","tool_input":{"command":"jj describe --stdin < msg.txt && pnpm push","run_in_background":true},"tool_response":{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"bswnpmo0d"}}"#,
    )
    .unwrap();
    let tool_input = input.tool_input.unwrap();
    assert_eq!(tool_input.run_in_background, Some(true));
    let command = tool_input.command.unwrap();
    assert!(skips_verification(&command, true));
    assert!(!skips_verification(&command, false), "同じコマンドでも foreground なら照合する");
}

/// 順位 536 の逆向き: 従来の照合対象は従来どおり照合する。`-R` は jj の外 (`cp -Rf`) や
/// quote の中では効かず、`cd` という語を含むだけの message でも照合をやめない。
#[test]
fn ordinary_commands_are_still_verified() {
    for command in [
        "jj new -m x",
        "jj describe -m x && jj new -m y 2>&1 | head -3",
        "cp -Rf a b && jj new -m x",
        "grep -R foo src; jj describe -m x",
        r#"jj describe -m "cd into the repo first""#,
        r#"jj describe -m "use -R carefully""#,
        "jj abandon -r x",
    ] {
        assert!(!skips_verification(command, false), "照合すること: {command}");
    }
    let input: HookInput =
        serde_json::from_str(r#"{"tool_input":{"command":"jj new -m x"}}"#).unwrap();
    assert_eq!(input.tool_input.unwrap().run_in_background, None, "フラグが無い入力は foreground");
}

/// jj は PowerShell tool からも実行されるので、matcher が Bash だけだと検証が漏れる
/// (2026-10-20、hooks-pre-tool-validate の PowerShell 配線漏れと同時に発見)。
/// PowerShell の `tool_response` も `stdout` / `stderr` を持つため、本 hook はそのまま扱える。
#[test]
fn the_settings_matcher_passes_both_shell_tools() {
    for rel in [".claude/settings.json", ".claude/settings.local.json.template"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} を読めない: {e}", path.display()));
        let settings: serde_json::Value = serde_json::from_str(&text).expect("settings は JSON");
        let matchers: Vec<&str> = settings["hooks"]["PostToolUse"]
            .as_array()
            .expect("PostToolUse がある")
            .iter()
            .filter(|entry| {
                entry["hooks"].as_array().is_some_and(|hooks| {
                    hooks.iter().any(|h| {
                        h["command"]
                            .as_str()
                            .is_some_and(|c| c.contains("hooks-post-tool-jj-op-verify"))
                    })
                })
            })
            .flat_map(|entry| entry["matcher"].as_str().unwrap_or_default().split('|'))
            .collect();
        for tool in ["Bash", "PowerShell"] {
            assert!(matchers.contains(&tool), "{rel} の jj-op-verify matcher に {tool} が無い");
        }
    }
}

/// 順位 597 の観測: `jj rebase ... && jj edit X` の後、先頭の `edit commit` を `rebase` と照合して
/// 誤警告した (4 回)。`edit` を変更系として検出し、記録済みと判定する。
#[test]
fn rebase_followed_by_edit_is_verified_against_the_edit() {
    let command = "jj rebase -s xyz -d master && jj edit xyz";
    let op = detect_last_mutating_jj_op(command).unwrap();
    assert_eq!(op.verb, "edit");
    let verdict =
        decide_context(command, &ops(&["0b1c2d3e4f5a edit commit 2d38d0a69bd5"])).unwrap();
    assert!(matches!(verdict, Verdict::Recorded(_)), "{}", verdict.message());
}

/// 順位 597: jj 0.42 で実測した op の文言 (2026-11-05) と、検出した操作の文言が前方一致する。
#[test]
fn measured_op_descriptions_match_the_detected_keywords() {
    for (command, op_line) in [
        ("jj edit @-", "aaaaaaaaaaaa edit commit dc30283288caee21"),
        ("jj commit -m c1", "aaaaaaaaaaaa commit f4cf53d4efcd8e23"),
        ("jj duplicate @", "aaaaaaaaaaaa duplicate 1 commit(s)"),
        ("jj absorb", "aaaaaaaaaaaa absorb changes into 1 commits"),
        ("jj revert -r @- -d @", "aaaaaaaaaaaa revert commit 6571c43a190e41ae"),
        ("jj metaedit -m x", "aaaaaaaaaaaa edit commit metadata for commit 3bff603e604e"),
        ("jj parallelize @-::@", "aaaaaaaaaaaa parallelize 2 commits"),
        ("jj next --edit", "aaaaaaaaaaaa next: 751df5e123f6 -> editing 316e975f86c2"),
        ("jj prev --edit", "aaaaaaaaaaaa prev: dc30283288ca -> editing fca347514d73"),
        ("jj file chmod x a.txt", "aaaaaaaaaaaa make paths executable in commit e76e72c0c779"),
        ("jj op restore @-", "aaaaaaaaaaaa restore to operation 8e27918e5fcf"),
        ("jj op revert", "aaaaaaaaaaaa revert operation 9c2e02feb97e"),
    ] {
        let verdict =
            decide_context(command, &ops(&[op_line])).unwrap_or_else(|| panic!("{command}"));
        assert!(matches!(verdict, Verdict::Recorded(_)), "{command}: {}", verdict.message());
    }
}

/// 順位 597: 文言の分からないサブコマンドが後ろに続いたら、前の操作と照合しない
/// (その op が先頭に来るので、照合すると必ず外れる)。
#[test]
fn an_unknown_subcommand_after_a_detected_op_skips_verification() {
    for command in [
        "jj new && jj resolve a.txt",
        "jj describe -m x && jj bookmark advance",
        "jj new && jj workspace add ../ws",
        "jj new && jj op abandon ..@-",
        "jj new && jj file track a.txt",
        "jj new && jj sign -r @",
    ] {
        assert!(detect_last_mutating_jj_op(command).is_none(), "{command}");
    }
}

/// 順位 597: 読み取り系や付随 op になる呼び出しは、前に検出した操作を残す。
#[test]
fn a_neutral_invocation_after_a_detected_op_keeps_it() {
    let push = ["jj", "git", "push", "-b", "feat/x"].join(" ");
    for command in [
        "jj new && jj log -r @".to_string(),
        "jj new && jj st".to_string(),
        "jj new && jj op log --limit 1".to_string(),
        "jj new && jj bookmark list".to_string(),
        "jj new && jj b l".to_string(),
        format!("jj new && {push}"),
        "jj new && jj evolution-log".to_string(),
        "jj new && jj --ignore-working-copy log".to_string(),
    ] {
        let op = detect_last_mutating_jj_op(&command).unwrap_or_else(|| panic!("{command}"));
        assert_eq!(op.verb, "new", "{command}");
    }
}

/// #619 の CodeRabbit 指摘: 照合をやめる判定は、コマンドの読み違いで効いてはならない。
/// 引数の中の `jj` / 語に付いた `;` / サブコマンドの無い `bookmark` の後の `&&` を
/// サブコマンドと読むと、前に検出した `new` を黙って捨てる (この PR 以前は照合していた)。
#[test]
fn only_a_jj_at_the_head_of_a_command_is_classified() {
    for command in [
        "jj new && grep -rn jj docs",
        "jj new && echo jj resolve",
        "jj new && jj log; echo done",
        "jj new && jj bookmark && echo done",
        "jj new; jj st;",
    ] {
        let op = detect_last_mutating_jj_op(command).unwrap_or_else(|| panic!("{command}"));
        assert_eq!(op.verb, "new", "{command}");
    }
}

/// 環境変数の代入が前にあっても、`jj` のコマンドとして読む。
#[test]
fn an_env_assignment_before_jj_is_skipped() {
    let op = detect_last_mutating_jj_op("JJ_CONFIG=x jj describe -m y").unwrap();
    assert_eq!(op.verb, "describe");
}

/// #619 の CodeRabbit 指摘: グローバルオプションをサブコマンドとして扱うと、`edit` を
/// 見落として前の `rebase` と照合し、誤警告を出す。値を取るオプションは値ごと読み飛ばす。
#[test]
fn global_options_before_the_subcommand_are_skipped() {
    for (command, verb) in [
        ("jj rebase -s x -d master && jj --ignore-working-copy edit x", "edit"),
        ("jj new && jj --at-op @- edit x", "edit"),
        ("jj new && jj --config ui.color=never --no-pager describe -m y", "describe"),
        ("jj new && jj --color never bookmark set feat/x -r @", "bookmark set"),
    ] {
        assert_eq!(detect_last_mutating_jj_op(command).map(|o| o.verb), Some(verb), "{command}");
    }
    assert!(
        detect_last_mutating_jj_op("jj --at-op @- log").is_none(),
        "オプションの値 (@-) をサブコマンドと読まない"
    );
    let op = detect_last_mutating_jj_op("jj new && jj --at-op @- log").unwrap();
    assert_eq!(op.verb, "new", "読み取り系は前の検出を残す");
}

/// #619 の CodeRabbit 指摘: `edit` の文言 `edit commit` は、`metaedit` の文言
/// `edit commit metadata ...` の先頭と重なる。`jj edit` が失敗して直前の op が metaedit の
/// ままでも、記録済みと判定しない。
#[test]
fn an_edit_is_not_matched_against_a_metaedit_op() {
    let metaedit = ops(&["aaaaaaaaaaaa edit commit metadata for commit 3bff603e604e"]);
    let verdict = decide_context("jj edit x", &metaedit).unwrap();
    assert!(!matches!(verdict, Verdict::Recorded(_)), "{}", verdict.message());
    let verdict = decide_context("jj metaedit -m x", &metaedit).unwrap();
    assert!(matches!(verdict, Verdict::Recorded(_)), "metaedit 自身は一致する: {}", verdict.message());
}

/// jj 0.42 の bookmark の別名 (`jj bookmark --help` の aliases)。
#[test]
fn bookmark_aliases_are_detected() {
    for (command, verb) in [
        ("jj b c feat/x -r @", "bookmark create"),
        ("jj b s feat/x -r @", "bookmark set"),
        ("jj b m feat/x --to @", "bookmark move"),
        ("jj b d feat/x", "bookmark delete"),
        ("jj bookmark t feat/x@origin", "bookmark track"),
    ] {
        assert_eq!(detect_last_mutating_jj_op(command).map(|o| o.verb), Some(verb), "{command}");
    }
}
