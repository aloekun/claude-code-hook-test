//! jj operation 検証 hook (Bash PostToolUse) — ADR-045 § Operation Verification Checklist の自動化。
//!
//! 並列 workspace 運用の lost-update incident (2026-07-12/13、ADR-045 § Known operational
//! risks) では、変更系 jj コマンドの「成功出力」が見えたにもかかわらず operation が
//! op log に記録されていなかった。本 hook は Bash tool で実行されたコマンドに変更系
//! jj 操作 (`new` / `describe` / `abandon` / `rebase` / `squash` / `bookmark 変更系`) が
//! 含まれる場合、直後に `jj op log --limit 1` (snapshot を発生させない読み取り) で
//! op head を取得し、操作に対応する operation が記録されたかを additionalContext で
//! 報告する。記録が無ければ「operation not recorded」警告を出し、事故クラスを即時検出する。
//!
//! 対象外: `jj git fetch` / `jj git push` (fetch は「Nothing changed」時に op を作らない
//! 正当なケースがあり誤警告になるため。push は push pipeline の refuse 検知が担当)。
//! read-only コマンド (`jj log` / `jj st` / `jj op log` / `jj bookmark list` 等) も対象外。
//!
//! **「対象外」は検出のトリガーとしてだけである — これらも op log には operation を書く。**
//! 初版はこの区別を書いておらず、`jj op log --limit 1` の先頭が `push bookmark ...` や
//! `snapshot working copy` に占められて誤警告を出していた (順位 489、実測で誤警告率 50%)。
//! 現在は [`INCIDENTAL_OP_PREFIXES`] を読み飛ばして最初の非付随 op と照合する。
//!
//! # 本 hook が塞げない既知の限界 (2026-08-23 ユーザー指摘)
//!
//! 1. **チェーン内の最後の 1 件しか追跡しない。** [`detect_last_mutating_jj_op`] は検出結果を
//!    上書きし続けるため、`jj describe -m x && jj new` では `new` だけを検証する。途中の
//!    `describe` が黙って落ちても捕捉できない。
//! 2. **サブプロセスが作る op は予見できない。** `pnpm push` の中で `jj git push` が走る経路は、
//!    コマンド文字列をどう解析しても事前には分からない。付随 op の読み飛ばしはこの経路の
//!    **誤警告**を消すが、「サブプロセスが何を書くか」を知った上での検証にはなっていない。
//!
//! どちらも「警告が出ない場合に安心してよい範囲」を狭める向きの限界である。警告が**出た**
//! ときの観測 (op log 先頭に対応する operation が無い) は従来どおり正しい。
//!
//! 試験運用 (ADR-039 準拠): `[post_tool_use.jj_op_verify] enabled` は source default-OFF、
//! 本リポジトリの `.claude/hooks-config.toml` で opt-in。fail-open: config 読込失敗 /
//! jj 不在 / timeout はすべて無出力で正常終了する (助言層であり block しない)。

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const JJ_OP_LOG_TIMEOUT_SECS: u64 = 5;

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

#[derive(Deserialize, Default)]
struct HooksConfig {
    post_tool_use: Option<PostToolUseSection>,
}

#[derive(Deserialize, Default)]
struct PostToolUseSection {
    jj_op_verify: Option<JjOpVerifyConfig>,
}

#[derive(Deserialize, Default)]
struct JjOpVerifyConfig {
    enabled: Option<bool>,
}

/// 検出した変更系 jj 操作。`expected_op_keyword` は成功時に op log 先頭の description に
/// 含まれるはずの jj の operation 文言。
#[derive(Debug, PartialEq)]
struct MutatingJjOp {
    verb: &'static str,
    expected_op_keyword: &'static str,
}

/// コマンド文字列から最後の変更系 jj 操作を検出する (複合コマンドでは最後の操作の op が
/// op head に来るため)。読み取り系サブコマンドは検出しない。
fn detect_last_mutating_jj_op(command: &str) -> Option<MutatingJjOp> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut found = None;
    for (i, token) in tokens.iter().enumerate() {
        if *token != "jj" {
            continue;
        }
        let Some(sub) = tokens.get(i + 1) else {
            continue;
        };
        let detected = match *sub {
            "new" => Some(("new", "new empty commit")),
            "describe" => Some(("describe", "describe commit")),
            "abandon" => Some(("abandon", "abandon commit")),
            "rebase" => Some(("rebase", "rebase commit")),
            "squash" => Some(("squash", "squash")),
            "bookmark" => match tokens.get(i + 2).copied() {
                Some("create") => Some(("bookmark create", "create bookmark")),
                Some("set") => Some(("bookmark set", "point bookmark")),
                Some("delete") => Some(("bookmark delete", "delete bookmark")),
                Some("forget") => Some(("bookmark forget", "forget bookmark")),
                Some("rename") => Some(("bookmark rename", "rename bookmark")),
                _ => None,
            },
            _ => None,
        };
        if let Some((verb, keyword)) = detected {
            found = Some(MutatingJjOp {
                verb,
                expected_op_keyword: keyword,
            });
        }
    }
    found
}

/// op head の description が操作に対応するか。
fn op_matches_expectation(op_head: &str, expected_keyword: &str) -> bool {
    op_head.to_lowercase().contains(expected_keyword)
}

fn build_ok_message(op: &MutatingJjOp, op_head: &str) -> String {
    format!(
        "[jj-op-verify] OK: `jj {}` の operation を記録確認 — {}",
        op.verb,
        op_head.trim()
    )
}

fn build_not_recorded_warning(op: &MutatingJjOp, op_head: &str) -> String {
    format!(
        "[jj-op-verify] WARNING: operation not recorded — 直前の `jj {}` に対応する operation が \
         op log 先頭にありません (先頭: {})。コマンドが実際には実行されていない可能性があります \
         (ADR-045 § Known operational risks の output corruption 兆候)。`jj op log` と \
         `jj log -r @` で実状態を確認してから作業を続けてください。",
        op.verb,
        op_head.trim()
    )
}

/// **付随 op** — 変更系 jj 操作とは無関係に op log へ書かれる operation。
///
/// これらが op log 先頭を占めると、直前の変更系操作の op が押し下げられて「記録されていない」
/// と読める (順位 489)。`jj git fetch` / `jj git push` は本 hook の**検出**対象から外れているが、
/// 外れているのは「検出のトリガー」としてだけで、**op log には operation を書く**。
/// `snapshot working copy` は読み取り以外のほぼすべての jj コマンドが作る。
///
/// 実測 (2026-08-23、直近 40 op): 28 件 (70%) が付随 op だった
/// (snapshot 17 / push 5 / fetch 5 / import git refs 1)。
const INCIDENTAL_OP_PREFIXES: &[&str] = &[
    "snapshot working copy",
    "fetch from git remote",
    "push bookmark",
    "push all bookmarks",
    "import git refs",
];

/// 遡る上限。付随 op を読み飛ばすために取る op の件数。
///
/// **無制限に広げない。** 窓を広げるほど「過去の同種 op がたまたま一致して記録済みと誤判定する」
/// 偽陰性が増える。付随 op は 1 コマンドあたり数件しか出ないため、10 件あれば実測の連続数
/// (`pnpm push` 経路で snapshot + push + fetch) を十分に覆える。
const OP_LOG_WINDOW: usize = 10;

/// op log の 1 行が付随 op か (I/O なし)。
fn is_incidental_op(op_line: &str) -> bool {
    let description = op_line.split_once(' ').map_or("", |(_, rest)| rest).trim();
    INCIDENTAL_OP_PREFIXES
        .iter()
        .any(|prefix| description.starts_with(prefix))
}

/// 付随 op を読み飛ばして、照合に使う最初の op を選ぶ (I/O なし)。
///
/// **止まるのは最初の非付随 op**であって、そこから先は探さない。探し続けると「本当に記録
/// されていない操作」の代わりに過去の同種 op を拾ってしまい、警告が出るべき場面で黙る。
fn select_op_for_matching(ops: &[String]) -> Option<&String> {
    ops.iter()
        .filter(|line| !line.trim().is_empty())
        .find(|line| !is_incidental_op(line))
}

/// `jj op log` で直近の op を [`OP_LOG_WINDOW`] 件まで取得する (1 行 1 op)。
/// op log は working copy を snapshot しない読み取り操作。fail-open: 失敗は空 Vec。
fn fetch_recent_ops() -> Vec<String> {
    fetch_op_log_output()
        .map(|out| out.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn fetch_op_log_output() -> Option<String> {
    let limit = OP_LOG_WINDOW.to_string();
    let mut child = Command::new("jj")
        .args([
            "op",
            "log",
            "--limit",
            &limit,
            "--no-graph",
            "-T",
            "id.short() ++ \" \" ++ description ++ \"\\n\"",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out_pipe = child.stdout.take()?;
    let stdout_handle = lib_subprocess::drain_pipe_unlimited(out_pipe);
    let status = lib_subprocess::wait_with_timeout_basic("jj op log", &mut child, JJ_OP_LOG_TIMEOUT_SECS)
        .ok()
        .flatten();
    let output = stdout_handle.join().ok()?;
    status.filter(|s| s.success()).map(|_| output)
}

/// 設定ファイルのパス解決 (exe ディレクトリ基準 — cwd に依存しない)。
/// 他の config 読込 hook (post-tool-linter / pre-tool-validate / stop-quality) と同じ規約。
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

fn verify_enabled(config_text: &str) -> bool {
    toml::from_str::<HooksConfig>(config_text)
        .ok()
        .and_then(|c| c.post_tool_use)
        .and_then(|p| p.jj_op_verify)
        .and_then(|v| v.enabled)
        .unwrap_or(false)
}

/// `decide_context` の判定結果。いずれも additionalContext に出す message を持つが、
/// telemetry (WP-12) は `NotRecorded` の発火のみ記録するため variant で判別する。
enum Verdict {
    Recorded(String),
    NotRecorded(String),
}

impl Verdict {
    fn message(&self) -> &str {
        match self {
            Verdict::Recorded(m) | Verdict::NotRecorded(m) => m,
        }
    }
}

/// stdin の HookInput と op log から判定結果を決める (純粋部)。
/// None = 何も出力しない (対象外コマンド / 無効化 / 検証不能)。
///
/// **op log の先頭ではなく、付随 op を読み飛ばした最初の op と照合する** (順位 489)。
/// 先頭だけを見ると `jj git push` / `snapshot working copy` に押し下げられた変更系操作を
/// 「記録されていない」と読む。本セッションの実測では 4 回発火中 2 回がこの誤警告だった。
fn decide_context(command: &str, ops: &[String]) -> Option<Verdict> {
    let op = detect_last_mutating_jj_op(command)?;
    let matched = select_op_for_matching(ops)?;
    if op_matches_expectation(matched, op.expected_op_keyword) {
        Some(Verdict::Recorded(build_ok_message(&op, matched)))
    } else {
        Some(Verdict::NotRecorded(build_not_recorded_warning(&op, matched)))
    }
}

/// jj-op-verify が「operation not recorded」警告を発火したことを記録する (WP-12、fail-open)。
fn record_not_recorded_warning() {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "hooks-post-tool-jj-op-verify",
        kind: lib_telemetry::FiringKind::Hook,
        id: "jj-op-verify",
        decision: lib_telemetry::Decision::Warn,
        session_id: None,
    });
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(hook_input) = serde_json::from_str::<HookInput>(&input) else {
        return;
    };
    let Some(command) = hook_input.tool_input.and_then(|t| t.command) else {
        return;
    };

    let enabled = std::fs::read_to_string(config_path())
        .ok()
        .map(|text| verify_enabled(&text))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    if detect_last_mutating_jj_op(&command).is_none() {
        return;
    }
    let ops = fetch_recent_ops();
    let Some(verdict) = decide_context(&command, &ops) else {
        return;
    };
    if matches!(verdict, Verdict::NotRecorded(_)) {
        record_not_recorded_warning();
    }
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": verdict.message(),
        }
    });
    println!("{output}");
}

#[cfg(test)]
mod tests {
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
    fn op_match_is_case_insensitive_contains() {
        assert!(op_matches_expectation(
            "d2e4a39cd26c describe commit d856d3b5",
            "describe commit"
        ));
        assert!(!op_matches_expectation(
            "f53cbee0d008 snapshot working copy",
            "new empty commit"
        ));
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

    /// 既知の限界 (順位 283 で anchor 修正予定、feedback-reports/267.md Tier 1 #3):
    /// `split_whitespace` は quote を認識しないため、commit message 内に埋め込まれた
    /// jj keyword が実コマンドより後に走査されると検出結果を上書きしてしまう。
    /// 本 test は現行挙動を regression として固定するもので、283 着手後は新挙動
    /// (message 内 keyword を無視) を固定するよう更新すること。
    #[test]
    fn tokenization_known_limitation_jj_keyword_inside_commit_message() {
        let op = detect_last_mutating_jj_op(
            r#"jj describe -m "note: mention jj new keyword here""#,
        )
        .unwrap();
        assert_eq!(
            op.verb, "new",
            "quote 非対応により message 内の 'jj new' が実コマンド 'jj describe' を上書きする"
        );
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
}
