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
#[test]
fn op_subcommands_are_not_mistaken_for_restore() {
    assert!(detect_last_mutating_jj_op("jj op restore c29450c43dfe").is_none());
    assert!(detect_last_mutating_jj_op("jj op revert c29450c43dfe").is_none());
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
