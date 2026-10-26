//! 再 push のレビュー範囲 (ADR-089)。前回この branch でレビューがすべて APPROVE になって push した
//! commit があれば、そこから `@` までの差分だけをレビューする。なければ PR 全体をレビューする。
//!
//! # 祖先コミットが未レビューで merge される穴 (todo 順位 288) を再び開けない理由
//!
//! 順位 288 の穴は、tip のコミットしか reviewers に渡らず、祖先のコミットが**一度も**レビューされない
//! ことだった。差分の起点は「前回 reviewers が全体を見て APPROVE した状態」なので、その状態までは
//! レビュー済みで、そこから `@` までの tree の差分は commit の構造に関わらず全部渡る。起点の記録は、
//! 「push した状態をそのまま reviewers が承認した」と言えるときだけ残す ([`record_approved`] の条件)。
//!
//! # PR 全体に戻すとき
//!
//! 次のどれかなら PR 全体をレビューする (安全側に倒す)。
//! - `[diff] incremental_command` が無い (派生プロジェクトの既定)、`PUSH_REVIEW_FULL=1`
//! - push する bookmark が 1 つでない (起点を 1 つに決められない)
//! - 承認の記録が無い、記録時と基点 (trunk) の commit が違う (master への rebase など、2026-10-25 ユーザー判断)
//! - 承認した commit を読めない
//! - 承認した commit が、その bookmark の remote 追跡 ref (`<bookmark>@origin`) の位置と一致しない
//!
//! # 承認記録の偽装対策 (SEC-NEW-review-scope-approval-forgery)
//!
//! 承認記録 `.claude/push-review-approved.json` ([`approved_path`]) は fix step の `Write`/`Edit`/`Bash`
//! からも書き込める場所にある。fix step が (finding 文面への prompt injection 経由で、ADR-054)
//! 未レビューの commit を指す偽の記録を書くと、次回 push の [`decide`] がそれを信じて差分レビューの
//! 範囲を狭めてしまう。fix step はファイル単位で書き込みを禁止できない (ADR-083 の permission mode は
//! step ごとの readonly / edit の二択) ので、書かせないのではなく、書かれたら捨てる。
//!
//! - **takt の前後で記録が変わったら捨てる** ([`read_approved_raw`] / [`discard_if_changed`])。正規の記録は
//!   push の成功後にしか書かれない ([`record_approved`]) ので、takt が動いている間の変更はすべて不正。
//!   書き込みの手段を問わず、「これから push される commit」を先読みした偽の記録も捨てられる
//! - **記録の commit が remote 追跡 ref と一致するときだけ信用する** ([`tip_matches`])。takt の外で
//!   書かれた、push されていない commit を指す記録を弾く
//! - 起点には記録の生文字列ではなく、jj が解決した commit ID を使う ([`choose_since`])
//!
//! 今回の reviewers が読む `.takt/review-scope.md` と差分は、takt の後に `review_input_check` が
//! 作り直した内容と突き合わせる (fix step が書き換えたら今回の push を止める)。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::diff::resolve_diff_command;
use super::docs_only_routing::run_jj_diff_summary_args;
use crate::config::{Config, DiffConfig, DEFAULT_REMOTE, DIFF_FROM_PLACEHOLDER};
use crate::log::log_stage;

const STAGE: &str = "review_scope";
/// 承認の記録 (`.claude/` の下、gitignore 対象)。
const APPROVED_FILE: &str = "push-review-approved.json";
/// これを `1` にすると、差分だけのレビューをやめて PR 全体をレビューする。
const FULL_ENV_VAR: &str = "PUSH_REVIEW_FULL";
/// 差分だけのレビューのとき、reviewers に範囲を伝えるファイル。PR 全体のときは消す。
pub(crate) const SCOPE_NOTE_PATH: &str = ".takt/review-scope.md";
/// 今回の diff を作ったコマンド。fix step が diff を作り直すときに使う (同じ範囲にそろえる)。
pub(crate) const DIFF_COMMAND_PATH: &str = ".takt/review-diff-command.txt";
const JJ_TIMEOUT_SECS: u64 = 30;
const SHORT_LEN: usize = 12;

/// レビュー範囲。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewScope {
    /// PR 全体 (`<base>..@`)。
    Full { pr_range: String },
    /// 前回承認した commit から `@` まで。`template` は `[diff] incremental_command` (この範囲は
    /// それがあるときだけ作るので、コマンドの元が無い状態にはならない)。
    Since { from: String, template: String },
}

impl ReviewScope {
    /// diff を作るコマンド。
    pub(crate) fn command(&self, diff: &DiffConfig) -> String {
        match self {
            ReviewScope::Full { pr_range } => resolve_diff_command(&diff.command, pr_range),
            ReviewScope::Since { from, template } => template.replace(DIFF_FROM_PLACEHOLDER, from),
        }
    }

    /// 範囲カバレッジ検査の正 (`jj diff --summary`)。diff と同じ範囲を見る。
    pub(crate) fn summary(&self) -> Result<String, String> {
        match self {
            ReviewScope::Full { pr_range } => run_jj_diff_summary_args(&["-r", pr_range]),
            ReviewScope::Since { from, .. } => run_jj_diff_summary_args(&["--from", from, "--to", "@"]),
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            ReviewScope::Full { pr_range } => format!("PR 全体 ({pr_range})"),
            ReviewScope::Since { from, .. } => format!("前回承認した {} から @ まで", short(from)),
        }
    }

    pub(crate) fn telemetry_label(&self) -> &'static str {
        match self {
            ReviewScope::Full { .. } => "full",
            ReviewScope::Since { .. } => "incremental",
        }
    }
}

/// 承認の記録 1 件。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Approved {
    /// reviewers がそのまま承認して push した commit。
    pub(crate) commit: String,
    /// そのときの基点 (PR 範囲の base) の commit。
    pub(crate) base: String,
    pub(crate) at: String,
}

/// 決めたレビュー範囲と、そのときの基点の commit (承認を記録するときに使う)。`incremental_command` が
/// 無い (差分レビューが無効) ときは `base_commit` が `None` で、承認も記録しない。
pub(crate) struct ScopeDecision {
    pub(crate) scope: ReviewScope,
    pub(crate) base_commit: Option<String>,
}

/// 差分だけをレビューしてよいなら、その起点の commit を返す (I/O を除いた判定)。
///
/// `resolve` は記録の `commit` 文字列を正規化した commit ID に解決する (`jj log` で実在を確かめる)。
/// 戻り値は常にこの正規化後の ID を使う — 記録の生文字列は後段で diff コマンドのテンプレートに
/// 展開されシェル実行に渡るため ([`ReviewScope::command`])、正規化されていない生文字列をそのまま
/// 使うとそこが injection の経路になる。
///
/// `remotely_pushed` は、記録の `commit` がその bookmark の remote 追跡 ref の位置と一致するかを確かめる
/// (承認記録の偽装対策、モジュール doc 参照)。
pub(crate) fn choose_since(
    enabled: bool,
    approved: Option<&Approved>,
    base_now: Option<&str>,
    resolve: impl Fn(&str) -> Option<String>,
    remotely_pushed: impl Fn(&str) -> bool,
) -> Option<String> {
    let approved = approved.filter(|_| enabled)?;
    if base_now != Some(approved.base.as_str()) {
        return None;
    }
    let canonical = resolve(&approved.commit)?;
    remotely_pushed(&canonical).then_some(canonical)
}

/// レビュー範囲を決める。差分レビューが無効 (`incremental_command` が無い) なら、jj も記録も読まずに
/// PR 全体を返す (派生プロジェクトの既定では何もしない、ADR-039)。`PUSH_REVIEW_FULL=1` は今回だけ
/// PR 全体をレビューし、承認は記録する (次の push は差分に戻れる)。
pub(crate) fn decide(config: &Config, bookmarks: &[String]) -> ScopeDecision {
    let pr_range = config.diff_pr_range();
    let Some(template) = config.diff.as_ref().and_then(|d| d.incremental_command.clone()) else {
        return ScopeDecision { scope: ReviewScope::Full { pr_range }, base_commit: None };
    };
    let base_commit = commit_id(base_of(&pr_range));
    let bookmark = match bookmarks {
        [bookmark] => Some(bookmark.as_str()),
        _ => None,
    };
    let approved = bookmark.and_then(|b| load_approved().remove(b));
    let since = choose_since(
        !forced_full(),
        approved.as_ref(),
        base_commit.as_deref(),
        commit_id,
        |c| bookmark.is_some_and(|b| is_remote_tip(b, c)),
    );
    let scope = match since {
        Some(from) => ReviewScope::Since { from, template },
        None => ReviewScope::Full { pr_range },
    };
    log_stage(STAGE, &format!("レビュー範囲: {}", scope.describe()));
    ScopeDecision { scope, base_commit }
}

/// push した状態を reviewers がそのまま承認したので、次の push の起点として記録する。
/// 書けなくても push は失敗にしない (次の push が PR 全体をレビューするだけ)。
pub(crate) fn record_approved(bookmarks: &[String], base_commit: Option<&str>) {
    let ([bookmark], Some(base)) = (bookmarks, base_commit) else {
        return;
    };
    let Some(commit) = commit_id(bookmark) else {
        return;
    };
    let mut all = load_approved();
    all.insert(bookmark.clone(), Approved { commit, base: base.to_string(), at: lib_pending_file::utc_now_iso8601() });
    if let Some(path) = approved_path() {
        match serde_json::to_string_pretty(&all) {
            Ok(text) => {
                if let Err(e) = write_atomic(&path, &text) {
                    log_stage(STAGE, &format!("承認の記録を書けなかった ({}): {e}", path.display()));
                }
            }
            Err(e) => log_stage(STAGE, &format!("承認の記録を作れなかった: {e}")),
        }
    }
}

/// diff を作ったコマンドと、差分だけのときの範囲の説明を書く。fix step と reviewers が読む。
/// 失敗は `Err` で返し、呼び出し側が takt の前に止める。PR 全体のときに前回の範囲の説明が消せずに
/// 残ると、reviewers は PR 全体を「承認済みの起点より後だけ」と読んでしまう。
pub(crate) fn write_scope_files(scope: &ReviewScope, command: &str) -> Result<(), String> {
    write_file(Path::new(DIFF_COMMAND_PATH), command)?;
    match scope {
        ReviewScope::Since { from, .. } => write_file(Path::new(SCOPE_NOTE_PATH), &scope_note(from)),
        ReviewScope::Full { .. } => match std::fs::remove_file(SCOPE_NOTE_PATH) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{SCOPE_NOTE_PATH} を消せなかった: {e}")),
            _ => Ok(()),
        },
    }
}

/// reviewers に渡す、差分だけのレビューの説明。
pub(crate) fn scope_note(from: &str) -> String {
    format!(
        "# このレビューの範囲\n\n\
         `.takt/review-diff.txt` は、この branch の前回の push でレビューがすべて APPROVE になった commit \
         `{from}` から `@` までの差分です (ADR-089)。それより前の変更は承認済みなので、指摘の対象にしません。\n\n\
         差分の意味を判断するのに周りのコードが要るときは、ファイルを直接読んでください。\n"
    )
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, content).map_err(|e| format!("{} を書けなかった: {e}", path.display()))
}

fn forced_full() -> bool {
    lib_telemetry::is_truthy(std::env::var(FULL_ENV_VAR).unwrap_or_default().as_str())
}

/// PR 範囲 (`<base>..@`) の base。
pub(crate) fn base_of(pr_range: &str) -> &str {
    pr_range.strip_suffix("..@").unwrap_or(pr_range)
}

/// tmp に書いて rename する atomic write (`cli-review-threads/src/pending.rs::save` と同じ流儀)。
/// 中断時に破損させない — `load_approved` は壊れた記録を空として扱い PR 全体レビューに安全に倒すので
/// 非致命的だが、承認記録を無駄に失う (= 次回も PR 全体をレビューするだけ) ことは避けられる。
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn approved_path() -> Option<PathBuf> {
    lib_jj_helpers::pipeline_lock::exe_claude_dir().map(|d| d.join(APPROVED_FILE))
}

/// 承認の記録を読む。無い・壊れている場合は空 (PR 全体のレビューに倒れる)。
fn load_approved() -> BTreeMap<String, Approved> {
    approved_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// `rev` がちょうど 1 つの commit を指すなら、その commit ID。
fn commit_id(rev: &str) -> Option<String> {
    let out = lib_subprocess::run_cmd_direct_capture(
        "jj",
        &["log", "-r", rev, "--no-graph", "-T", "commit_id ++ \"\\n\""],
        JJ_TIMEOUT_SECS,
    );
    if !out.ok {
        return None;
    }
    match out.stdout.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().as_slice() {
        [id] => Some(id.trim().to_string()),
        _ => None,
    }
}

/// `commit` が、`bookmark` の remote 追跡 ref (`<bookmark>@origin`) の指す commit と一致するか
/// (承認記録の偽装対策、モジュール doc 参照)。
fn is_remote_tip(bookmark: &str, commit: &str) -> bool {
    tip_matches(commit_id(&format!("{bookmark}@{DEFAULT_REMOTE}")).as_deref(), commit)
}

/// takt を動かす前の承認記録 (ファイルの中身そのまま。無ければ `None`)。[`discard_if_changed`] と対で使う。
pub(crate) fn read_approved_raw() -> Option<String> {
    approved_path().and_then(|p| std::fs::read_to_string(p).ok())
}

/// takt の前 (`before`) と比べて承認記録が変わっていたら、記録を捨てる (承認記録の偽装対策、
/// モジュール doc 参照)。次の push は PR 全体をレビューする。
pub(crate) fn discard_if_changed(before: Option<&str>) {
    let Some(path) = approved_path() else {
        return;
    };
    let after = std::fs::read_to_string(&path).ok();
    if !approvals_changed(before, after.as_deref()) {
        return;
    }
    log_stage(STAGE, "承認の記録が takt の実行中に変わったため捨てます (次の push は PR 全体をレビュー)");
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log_stage(STAGE, &format!("承認の記録を消せなかった ({}): {e}", path.display()));
        }
    }
}

/// 承認記録が takt の前後で変わったか。消えた場合も「変わった」とする。
pub(crate) fn approvals_changed(before: Option<&str>, after: Option<&str>) -> bool {
    before != after
}

/// remote 追跡 ref の commit (読めなければ `None`) が `commit` と一致するか。読めないときは一致しない扱い
/// (= PR 全体レビューに倒す)。
pub(crate) fn tip_matches(remote_tip: Option<&str>, commit: &str) -> bool {
    remote_tip == Some(commit)
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(SHORT_LEN)]
}

#[cfg(test)]
#[path = "review_scope/tests.rs"]
mod tests;
