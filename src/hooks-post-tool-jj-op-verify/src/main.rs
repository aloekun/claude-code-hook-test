//! jj operation 検証 hook (Bash PostToolUse) — ADR-045 § Operation Verification Checklist の自動化。
//!
//! 並列 workspace 運用の lost-update incident (2026-07-12/13、ADR-045 § Known operational
//! risks) では、変更系 jj コマンドの「成功出力」が見えたにもかかわらず operation が
//! op log に記録されていなかった。本 hook は Bash tool で実行されたコマンドに変更系
//! jj 操作 (`new` / `describe` / `abandon` / `rebase` / `squash` / `restore` / `split` / `undo` /
//! `bookmark 変更系`) が
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
//! **jj が `Nothing changed.` を出したときは照合しない** (順位 282)。変更の無い `jj restore` や
//! 既に track 済みの `jj bookmark track` は、成功しても op を作らない (jj 0.42 実測)。fetch を
//! 検出対象から外したのと同じ理由で、照合すると誤警告になる。判定はコマンドの出力
//! (`tool_response`) で行う — [`reported_no_change`]。
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
//! 3. **複合コマンドでは、どれか 1 つが `Nothing changed.` を出すと全体を照合しない。** 出力は
//!    コマンド全体で 1 つなので、どの jj 操作が出した行かを区別できない。
//!
//! どれも「警告が出ない場合に安心してよい範囲」を狭める向きの限界である。警告が**出た**
//! ときの観測 (op log 先頭に対応する operation が無い) は従来どおり正しい。
//!
//! 試験運用 (ADR-039 準拠): `[post_tool_use.jj_op_verify] enabled` は source default-OFF、
//! 本リポジトリの `.claude/hooks-config.toml` で opt-in。fail-open: config 読込失敗 /
//! jj 不在 / timeout はすべて無出力で正常終了する (助言層であり block しない)。

use serde::Deserialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const JJ_OP_LOG_TIMEOUT_SECS: u64 = 5;

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
    /// Bash の結果 (`stdout` / `stderr` / `interrupted`)。形が想定と違っても hook 全体の
    /// パースを失敗させないよう、`Value` のまま受けて [`reported_no_change`] で読む。
    tool_response: Option<serde_json::Value>,
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

/// 開いた quote の中身を閉じ quote まで読み進め、`current` へ積む。
///
/// **二重引用符の中だけ backslash を escape として解釈する** (POSIX の quoting 規則)。
/// これが無いと `jj describe -m "note: \" jj new \" here"` の `\"` を終端と誤読し、
/// message 本文の `jj new` が独立したコマンドとして検出される (PR #494 CodeRabbit 指摘)。
/// **単一引用符では解釈しない** — POSIX では `'...'` 内の backslash は普通の文字であり、
/// ここで escape 扱いすると `'a \' b'` の閉じ quote を読み飛ばして逆向きに壊れる。
///
/// 閉じ quote が無ければ文字列終端まで読む (fail-open: パースエラーで panic しない)。
fn consume_quoted(chars: &mut std::str::Chars<'_>, quote: char, current: &mut String) {
    let honors_escape = quote == '"';
    let mut escaped = false;
    for quoted in chars.by_ref() {
        if escaped {
            current.push(quoted);
            escaped = false;
        } else if honors_escape && quoted == '\\' {
            escaped = true;
        } else if quoted == quote {
            return;
        } else {
            current.push(quoted);
        }
    }
}

/// コマンド文字列を、quote (`"..."` / `'...'`) で囲まれた範囲を 1 トークンとして扱いつつ
/// 空白で分割する。`split_whitespace` と異なり、quote 内の文言 (例: commit message 本文) が
/// 別トークンへ分割されて jj サブコマンド名と誤認されることがない (順位 476)。
/// quote 内の読み進めと escape の扱いは [`consume_quoted`] が持つ。
fn tokenize_respecting_quotes(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                in_token = true;
                consume_quoted(&mut chars, c, &mut current);
            }
            c if c.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                in_token = true;
                current.push(c);
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

/// コマンド文字列から最後の変更系 jj 操作を検出する (複合コマンドでは最後の操作の op が
/// op head に来るため)。読み取り系サブコマンドは検出しない。quote 内の文言 (commit message
/// 本文など) はトークナイズの時点で 1 トークンに畳まれるため、サブコマンド名として誤認しない。
fn detect_last_mutating_jj_op(command: &str) -> Option<MutatingJjOp> {
    let tokens = tokenize_respecting_quotes(command);
    let mut found = None;
    for (i, token) in tokens.iter().enumerate() {
        if token != "jj" {
            continue;
        }
        let Some(sub) = tokens.get(i + 1) else {
            continue;
        };
        let detected = match sub.as_str() {
            "new" => Some(("new", "new empty commit")),
            "describe" => Some(("describe", "describe commit")),
            "abandon" => Some(("abandon", "abandon commit")),
            "rebase" => Some(("rebase", "rebase commit")),
            "squash" => Some(("squash", "squash")),
            "restore" => Some(("restore", "restore into commit")),
            "split" => Some(("split", "split commit")),
            "undo" => Some(("undo", "undo: restore to operation")),
            "bookmark" => match tokens.get(i + 2).map(String::as_str) {
                Some("create") => Some(("bookmark create", "create bookmark")),
                Some("set") => Some(("bookmark set", "point bookmark")),
                Some("move") => Some(("bookmark move", "point bookmark")),
                Some("track") => Some(("bookmark track", "track remote bookmark")),
                Some("untrack") => Some(("bookmark untrack", "untrack remote bookmark")),
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

/// jj が「何も変えなかった」と報告したか (I/O なし)。
///
/// jj はこの 1 行を stderr に書くが、Bash tool では `2>&1` の有無で stdout 側に入ることも
/// あるため両方を見る。`tool_response` が無い・形が違うときは `false` (= 従来どおり照合する)。
fn reported_no_change(tool_response: Option<&serde_json::Value>) -> bool {
    let Some(response) = tool_response else {
        return false;
    };
    ["stdout", "stderr"].iter().any(|field| {
        response
            .get(field)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.lines().any(|line| line.trim() == "Nothing changed."))
    })
}

/// op log の 1 行 (`<op id> <description>`) から description を取り出す (I/O なし)。
fn op_description(op_line: &str) -> &str {
    op_line.split_once(' ').map_or("", |(_, rest)| rest).trim()
}

/// op head の description が操作に対応するか。
///
/// **description の前方一致で見る** (順位 282)。部分一致だと `untrack remote bookmark` が
/// `track remote bookmark` を含むため、`jj bookmark track` が落ちて直前に untrack の op が
/// 残っていても記録済みと誤判定する。キーワードは jj 0.42 の実機で、各操作の description の
/// 先頭に来ることを確かめてある (2026-10-07)。
fn op_matches_expectation(op_head: &str, expected_keyword: &str) -> bool {
    op_description(op_head)
        .to_lowercase()
        .starts_with(expected_keyword)
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
    let description = op_description(op_line);
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

/// 設定ファイルのパス解決 (exe の位置基準 — cwd に依存しない)。
/// 探索順序は [`lib_config_path`] が 1 箇所で持つ。
fn config_path() -> PathBuf {
    lib_config_path::resolve_config("hooks-config.toml")
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
        reason: None,
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
    if reported_no_change(hook_input.tool_response.as_ref()) {
        return;
    }

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
mod tests;
