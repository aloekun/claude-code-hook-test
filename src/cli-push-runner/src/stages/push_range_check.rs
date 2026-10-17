//! Push range check stage — 順位 537 (merge-pr 後の空コミットを push 前に止める)
//!
//! `jj git push` は、push で新しくリモートへ送るコミットの中に説明の無いものがあると
//! `Won't push commit <id> since it has no description` で拒否する。ところが
//! `bookmark_check` は `@` の説明しか見ないため、**祖先**に説明の無いコミットがあると
//! quality_gate と takt レビュー (2〜3 分) を通過した後の最終段で初めて落ちていた。
//!
//! 典型的な発生経路は決定的に起きる (#484 で 2 回連続): `pnpm merge-pr` は同期の最後に
//! `jj new <branch>@origin` を実行して `@` を空・説明なしにし (post_steps の残骸検知の
//! 基準線)、その上で [ADR-045] の規約どおり `jj new -m X` で作業を始めると、空で説明なしの
//! 親が push 範囲に残る。
//!
//! 本 stage は `bookmark_check` が push 対象の bookmark を確定した直後 (takt より前) に、
//! **jj が拒否するのと同じ範囲**を照会して止める:
//!
//! - 範囲は `remote_bookmarks(remote=exact:"origin")..<bookmark>` (リモートにまだ無いコミット)。
//!   root commit は jj の検査対象外なので除く
//! - 説明の無いコミットがあれば停止し、空かどうかを区別して修復コマンドを示す
//!   (空なら `jj abandon`、中身があれば `jj describe -r`)。**自動では捨てない** —
//!   空でも子の親や他 workspace からの参照として役割を持ちうるため (順位 537 決定事項)
//! - 照会に失敗したら止める ([ADR-043] fail-closed)。続行すると、検査していないのに
//!   通ったのと区別がつかない
//!
//! `bookmark_check` と同じく config section は持たない (無効化すべき正当な use case が無い)。
//!
//! [ADR-043]: ../../../../docs/adr/adr-043-security-gates-fail-closed.md
//! [ADR-045]: ../../../../docs/adr/adr-045-jj-workspace-parallel-sessions.md

use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::DEFAULT_REMOTE;
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;

/// 説明の無いコミット 1 件。
#[derive(Debug, PartialEq)]
struct Undescribed {
    change_id: String,
    commit_id: String,
    empty: bool,
}

/// 1 つの bookmark についての判定。
#[derive(Debug, PartialEq)]
enum RangeCheck {
    /// push 範囲に説明の無いコミットは無い。
    Clean,
    /// 説明の無いコミットがある (push 不可)。
    Undescribed { bookmark: String, commits: Vec<Undescribed> },
    /// 照会に失敗した (push 不可、fail-closed)。
    Unavailable { bookmark: String, reason: String },
}

/// push 対象の各 bookmark について push 範囲を検査する。`false` = pipeline を中断する。
pub(crate) fn run_push_range_check(bookmarks: &[String]) -> bool {
    let results: Vec<RangeCheck> = bookmarks
        .iter()
        .map(|b| classify(b, query_undescribed(b, None)))
        .collect();
    report(&results)
}

/// jj が push を拒否するのと同じ範囲で、説明の無いコミットを列挙する revset。
///
/// bookmark 名は文字列リテラルとして埋める。名前に `-` を含むと素の記号では
/// 演算子として解釈されうるため (例: `fix/a-b`)。
fn undescribed_revset(bookmark: &str) -> String {
    format!("({}) & description(exact:\"\") ~ root()", range_revset(bookmark))
}

/// push で新しくリモートへ送るコミットの範囲 (リモートにまだ無い、bookmark の祖先)。
fn range_revset(bookmark: &str) -> String {
    format!(
        "remote_bookmarks(remote=exact:{})..{}",
        revset_string(DEFAULT_REMOTE),
        revset_string(bookmark),
    )
}

/// jj の revset 文字列リテラル。`"` と `\` をエスケープする。
fn revset_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 1 行 = `<change_id>\t<commit_id>\t<empty|nonempty>`。
const TEMPLATE: &str =
    "change_id.short(12) ++ \"\\t\" ++ commit_id.short(12) ++ \"\\t\" ++ if(empty, \"empty\", \"nonempty\") ++ \"\\n\"";

/// jj を起動して revset を照会する。`dir` はテスト用 (本番は `None` = カレントディレクトリ)。
/// プロセス全体の cwd を書き換えずに済むよう、ディレクトリは子プロセスにだけ渡す。
fn query_undescribed(bookmark: &str, dir: Option<&Path>) -> Result<String, String> {
    let revset = undescribed_revset(bookmark);
    let mut cmd = Command::new("jj");
    cmd.args(["log", "--no-graph", "--ignore-working-copy", "-r", &revset, "-T", TEMPLATE]);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj log 起動失敗: {}", e))?;
    let stdout_handle =
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));
    let status = lib_subprocess::wait_with_timeout_basic("jj log", &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("jj log wait 失敗: {}", e))?;
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    match status {
        None => Err(format!("jj log タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

/// 照会結果を判定に落とす (I/O なし)。
///
/// 解釈できない行が 1 行でもあれば `Unavailable` にする — 読み飛ばすと、出力形式が
/// 変わったときに「説明の無いコミットは無い」へ黙って倒れる (fail-open)。
fn classify(bookmark: &str, raw: Result<String, String>) -> RangeCheck {
    let raw = match raw {
        Ok(raw) => raw,
        Err(reason) => {
            return RangeCheck::Unavailable { bookmark: bookmark.to_string(), reason };
        }
    };
    let mut commits = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let parsed = match fields.as_slice() {
            [change, commit, "empty"] => Some((change, commit, true)),
            [change, commit, "nonempty"] => Some((change, commit, false)),
            _ => None,
        };
        let Some((change, commit, empty)) = parsed else {
            return RangeCheck::Unavailable {
                bookmark: bookmark.to_string(),
                reason: format!("jj log の出力を解釈できません: {:?}", line),
            };
        };
        commits.push(Undescribed {
            change_id: change.trim().to_string(),
            commit_id: commit.trim().to_string(),
            empty,
        });
    }
    if commits.is_empty() {
        RangeCheck::Clean
    } else {
        RangeCheck::Undescribed { bookmark: bookmark.to_string(), commits }
    }
}

/// 判定を出力し、続行してよいかを返す。
fn report(results: &[RangeCheck]) -> bool {
    let mut ok = true;
    for r in results {
        if let Some((summary, hint)) = abort_report(r) {
            log_stage("push_range", &summary);
            log_info(&hint);
            ok = false;
        }
    }
    if ok {
        log_stage("push_range", "push 範囲に説明の無いコミットはありません");
    }
    ok
}

/// 中断するときの (stage ログ用 summary, 対処案内)。`Clean` なら `None`。
/// 出力を戻り値にして、案内文の内容を単体テストで直接確かめられるようにする。
fn abort_report(result: &RangeCheck) -> Option<(String, String)> {
    match result {
        RangeCheck::Clean => None,
        RangeCheck::Unavailable { bookmark, reason } => Some((
            format!("push 範囲を確認できませんでした (push 不可、bookmark: {}): {}", bookmark, reason),
            "  push 不可: push で送るコミットに説明の無いものが無いかを確認できませんでした。\n  \
             このまま続行すると、検査していないのに通ったのと区別がつきません。\n  \
             対処: jj の不調 (lock 競合 / 応答遅延) が解消してから再実行してください"
                .to_string(),
        )),
        RangeCheck::Undescribed { bookmark, commits } => {
            let mut lines = Vec::new();
            for c in commits {
                let (kind, fix) = if c.empty {
                    ("空", format!("jj abandon {}", c.change_id))
                } else {
                    ("中身あり", format!("jj describe -r {} -m \"<説明>\"", c.change_id))
                };
                lines.push(format!(
                    "    - {} ({}) [{}] → 対処: `{}`",
                    c.change_id, c.commit_id, kind, fix
                ));
            }
            let hint = format!(
                "  push 不可: jj は説明の無いコミットを push できません (`Won't push commit ... no description`)。\n  \
                 push で送るコミット ({}) に、説明の無いものが {} 件あります:\n{}\n  \
                 空のコミットは、merge-pr の後に `jj new -m` で作業を始めたときに残る基準コミットであることが多いです\n  \
                 (`jj abandon` で捨てると、子のコミットは 1 つ上の親へ付け替わります)。\n  \
                 親子関係は `jj log -r '{}'` で確認できます。修復してから再実行してください",
                bookmark,
                commits.len(),
                lines.join("\n"),
                range_revset(bookmark),
            );
            Some((
                format!("push 範囲に説明の無いコミットがあります (push 不可、bookmark: {})", bookmark),
                hint,
            ))
        }
    }
}

#[cfg(test)]
mod tests;
