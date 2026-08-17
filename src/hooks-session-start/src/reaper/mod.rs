//! Orphan run reaper (Bundle c-1 順位 64、ADR-030 §L2 out-of-process)。
//!
//! `.takt/runs/<slug>/meta.json` を scan し、`status: "running"` のまま
//! `ORPHAN_THRESHOLD_SECS` を超えた post-merge-feedback run を「abrupt
//! termination で死んだ」とみなして `.failed` marker を生成 + meta.json
//! `status` を `failed` に更新する。kill -9 / SIGKILL / power loss /
//! OOM Killer など in-process Drop guard (§L1) で救済できない致命系で
//! `.failed` marker が書かれなかった orphan run を L2 で拾う。

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::past_time::PastTime;

/// post-merge-feedback task label prefix (ADR-030 §task labeling convention)。
///
/// 値は `cli-merge-pipeline::feedback::TAKT_TASK_PREFIX` と同一でなければならない。
/// crate 間直接依存を避けるため inline duplicate しているが、両 crate の unit test
/// で literal `"post-merge-feedback for #"` を pin する drift 検出を行う
/// (`task_prefix_matches_canonical_literal` 系 test)。
pub(crate) const TAKT_TASK_PREFIX_PMF: &str = "post-merge-feedback for #";

/// orphan reaper の閾値秒数 (ADR-030 §L2 out-of-process)。
///
/// `cli-merge-pipeline::feedback::TAKT_TIMEOUT_SECS` (1200s) + 余裕 5 分。正常 run は
/// 1200s 以内に completed / failed のいずれかに遷移するため、本値を超えても
/// `status: "running"` のまま放置される run は abrupt termination で in-process Drop
/// guard を経由せず死んだとみなす。両 crate の test で `1500` を pin する。
pub(crate) const ORPHAN_THRESHOLD_SECS: u64 = 1500;

/// `.claude/feedback-reports/` の相対パス (repo root から)。
pub(crate) const FEEDBACK_DIR_REPO_RELATIVE: &str = ".claude/feedback-reports";

/// takt run ディレクトリ配下でレポートが置かれる sub directory 名。
///
/// `meta.json` の `reportDirectory` が使えない場合のフォールバック先 (→ [`run_report_path`])。
const RUN_REPORTS_SUBDIR: &str = "reports";

/// takt が run ごとに書く feedback レポートのファイル名。
///
/// 値は `cli-merge-pipeline::feedback::run_registry::REPORT_FILE_NAME` と同一でなければ
/// ならない。crate 間直接依存を避けるため inline duplicate しており、drift は両 crate の
/// unit test が literal を pin して検出する ([`TAKT_TASK_PREFIX_PMF`] と同じ方式)。
const RUN_REPORT_FILE_NAME: &str = "feedback-report.md";

/// `.takt/runs/` の相対パス (repo root から)。
pub(crate) const TAKT_RUNS_DIR: &str = ".takt/runs";

/// takt meta.json の必要 field のみ部分デシリアライズ。
#[derive(Deserialize)]
pub(crate) struct TaktMeta {
    pub(crate) task: Option<String>,
    pub(crate) status: Option<String>,
    #[serde(rename = "startTime")]
    pub(crate) start_time: Option<String>,
    #[serde(rename = "reportDirectory")]
    pub(crate) report_directory: Option<String>,
}

/// 検出された orphan run の情報。`reap_orphans` が `.failed` marker を書く際に使う。
pub(crate) struct OrphanRun {
    pub(crate) meta_path: PathBuf,
    pub(crate) pr_number: u64,
    pub(crate) age_secs: u64,
    /// `meta.json` の `reportDirectory` (repo root からの相対パス)。
    /// この run **自身**の成果物を探すために使う (→ [`run_report_path`])。
    pub(crate) report_directory: Option<String>,
}

/// `2026-05-13T12:33:23.908Z` 形式の ISO 8601 文字列を Unix 秒に変換する。
///
/// 失敗 (invalid date / 構造不正 / 月日範囲外) 時は `None`。fractional 秒は truncate
/// (整数秒精度で十分)。
///
/// 実体は `lib-pending-file::iso8601_to_epoch_secs` への薄い delegation。本 crate と
/// `cli-merge-pipeline` が同一実装を重複させていたため、後者が既に使っている集約先へ寄せた
/// (ADR-044)。**`check-ci-coderabbit::rate_limit::parse_iso8601_to_unix` は未統合**である
/// — あちらは fractional 秒を受理しない別挙動のため、寄せるには呼び手側の確認が要る。
///
/// 呼び手 (`past_time.rs` の proptest 含む) が `crate::reaper::parse_iso8601_to_unix` を
/// 直接参照するため、関数名・シグネチャは維持する。
pub(crate) fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    lib_pending_file::iso8601_to_epoch_secs(s)
}

fn read_takt_meta(path: &Path) -> Option<TaktMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// task label `"post-merge-feedback for #N"` から PR 番号 N を抽出する。
fn extract_pr_number_from_task(task: &str) -> Option<u64> {
    task.strip_prefix(TAKT_TASK_PREFIX_PMF)?.trim().parse().ok()
}

/// meta.json から orphan 判定に必要な要素 (pr_number, start_unix) を抽出する。
///
/// status / task / startTime のいずれかが orphan 条件を満たさなければ `None`。
fn meta_to_orphan_inputs(meta: &TaktMeta) -> Option<(u64, i64)> {
    if meta.status.as_deref() != Some("running") {
        return None;
    }
    let pr = extract_pr_number_from_task(meta.task.as_deref()?)?;
    let start = parse_iso8601_to_unix(meta.start_time.as_deref()?)?;
    Some((pr, start))
}

/// `.takt/runs/<slug>/meta.json` を scan して orphan な post-merge-feedback run を返す。
///
/// 条件: `status: "running"` AND task が `TAKT_TASK_PREFIX_PMF` で始まる AND
/// `now_unix - startTime >= ORPHAN_THRESHOLD_SECS`。malformed meta.json / non-PMF task /
/// PR 番号 parse 失敗 / startTime parse 失敗は defensive に skip。
pub(crate) fn find_orphan_post_merge_feedback_runs(
    runs_dir: &Path,
    now_unix: i64,
) -> Vec<OrphanRun> {
    let entries = match std::fs::read_dir(runs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.json");
        let Some(meta) = read_takt_meta(&meta_path) else {
            continue;
        };
        let Some((pr_number, start_unix)) = meta_to_orphan_inputs(&meta) else {
            continue;
        };
        let Some(past) = PastTime::from_parts(start_unix, now_unix) else {
            continue;
        };
        let age = past.age_secs();
        if age < ORPHAN_THRESHOLD_SECS as i64 {
            continue;
        }
        orphans.push(OrphanRun {
            meta_path,
            pr_number,
            age_secs: age as u64,
            report_directory: meta.report_directory,
        });
    }
    orphans
}

/// orphan の meta.json に書き戻す終端 status。
///
/// takt process が死んで書き戻せなかった `status` を、reaper が観測事実から確定させる。
/// `"running"` のまま残すと `cli-merge-pipeline` の並行起動 guard が以後の
/// post-merge-feedback を止め続ける (2026-08-17 の #249 incident)。
const TERMINAL_STATUS_FAILED: &str = "failed";
const TERMINAL_STATUS_COMPLETED: &str = "completed";

/// orphan の meta.json を終端 status へ書き換える。reaper の責任明示のため
/// `reaped_by: "hooks-session-start"` も追加する。malformed JSON は skip (Err 返す)。
///
/// # `endTime` は書かない
///
/// reaper は run の完了時刻を観測していない。レポートの mtime で代用すると、それは
/// 「レポートが書かれた時刻」であって「この run が終わった時刻」ではない。実際 2026-08-13 の
/// 手動復旧はこの代用を行い、成果物ゼロで死んだ #374 の 1 本目に「40.1 分の正常完了」という
/// 架空の記録を残した (実際に 8.1 分で完走したのは 32 分後に起動された 2 本目)。
///
/// `endTime` が無い run は所要時間の集計から自然に外れる。異常終了した run を分布に混ぜない
/// という点で、これは欠損ではなく正しい振る舞いである。
fn settle_meta_status(meta_path: &Path, terminal_status: &str) -> std::io::Result<()> {
    let content = std::fs::read_to_string(meta_path)?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "status".to_string(),
            serde_json::Value::String(terminal_status.to_string()),
        );
        obj.insert(
            "reaped_by".to_string(),
            serde_json::Value::String("hooks-session-start".to_string()),
        );
    }
    let serialized = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
    std::fs::write(meta_path, serialized)
}

/// `.failed` marker の本文を組み立てる。L2 recovery が拾う際の根拠 + 復旧手順を含む。
fn build_reaper_failed_marker_body(orphan: &OrphanRun) -> String {
    format!(
        "# post-merge-feedback failed (PR #{pr})\n\n\
         takt workflow が abrupt 終了 (kill -9 / SIGKILL / power loss / OOM 等) で中断され、\n\
         in-process Drop guard 経路を経由せずに死んだとみなされました\n\
         (orphan reaper, ADR-030 §L2 out-of-process)。\n\n\
         ## 検出情報\n\n\
         - meta.json: `{meta}`\n\
         - 経過時間: {age} 秒 (閾値: {threshold} 秒 = TAKT_TIMEOUT_SECS + 余裕 5 分)\n\n\
         ## 復旧手順\n\n\
         1. このマーカーを残したまま、Claude Code セッションで何か入力する\n\
         2. UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出し、\n   \
         Claude に再実行を促す\n\
         3. 手動で再実行する場合: `pnpm exec takt -w post-merge-feedback -t \"post-merge-feedback for #{pr}\"`\n",
        pr = orphan.pr_number,
        meta = orphan.meta_path.display(),
        age = orphan.age_secs,
        threshold = ORPHAN_THRESHOLD_SECS,
    )
}

/// 検出された orphan run に対し `.failed` marker と meta.json `status=failed` を書く。
///
fn write_new_marker_file(path: &std::path::Path, body: &str) -> bool {
    use std::io::Write as _;
    let Ok(mut f) = std::fs::File::create_new(path) else {
        return false;
    };
    if f.write_all(body.as_bytes()).is_err() {
        let _ = std::fs::remove_file(path);
        return false;
    }
    true
}

/// reaper が 1 pass で行った処置。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ReapOutcome {
    /// 新規に `.failed` marker を書いた (PR 番号, age_secs)。ユーザーへの nag 対象。
    pub(crate) marked_failed: Vec<(u64, u64)>,
    /// marker は書かず、meta.json の stale な `status` だけを終端状態へ確定させた PR 番号。
    pub(crate) settled_only: Vec<u64>,
}

/// 検出された orphan run の `.failed` marker と meta.json `status` を確定させる。
///
/// # marker を書くかどうかと、status を直すかどうかは別問題 (2026-08-17 incident)
///
/// 旧実装は marker / 成功レポートのいずれかがあれば orphan 全体を skip しており、
/// **`status: "running"` を直すことまで skip していた**。その結果 `20260706-...-for-249` は
/// 6 週間 `"running"` のまま残り、`cli-merge-pipeline` の並行起動 guard が以後の
/// post-merge-feedback (#394 / #408) を恒久的に block した。
///
/// marker はユーザーへの nag なので二重に書かない。一方 meta.json の `status` は
/// 機構が読む状態なので、どの経路でも必ず終端状態へ確定させる。
///
/// # 証拠のスコープも marker と status で異なる (2026-08-18 修正)
///
/// `status` は **この run 自身**が書いた `feedback-report.md` だけを根拠にする。PR 単位の
/// `<pr>.md` を使うと、feedback が再実行された PR で**後続 run の成果物を先に死んだ run の
/// 成功と誤判定**する。実際 #281 / #374 では 1 本目が analyze 開始 34 秒以内に死んで成果物
/// ゼロだったにもかかわらず、2 本目が書いた `<pr>.md` を根拠に手動復旧が「成功」と記録した。
///
/// 一方 `marker` は PR 単位のままでよい。marker は「この PR の feedback をやり直せ」という
/// ユーザーへの nag であり、別 run が既に `<pr>.md` を作っているなら nag は不要だからである。
///
/// | この run の `feedback-report.md` | `<pr>.md` | 既存 marker | marker | status |
/// |---|---|---|---|---|
/// | あり | あり | — | 書かない | `completed` |
/// | あり | なし | なし | 書く | `completed` |
/// | なし | あり | — | 書かない | `failed` |
/// | なし | なし | なし | 書く | `failed` |
/// | — | — | あり | 既存を保持 | 上記に同じ |
///
/// `endTime` は書かない (→ [`settle_meta_status`])。
///
/// marker 書込失敗時は当該 orphan を skip して次に進む (best-effort)。
pub(crate) fn reap_orphans(repo_root: &Path, orphans: &[OrphanRun]) -> ReapOutcome {
    let mut outcome = ReapOutcome::default();
    for orphan in orphans {
        let feedback_dir = repo_root.join(FEEDBACK_DIR_REPO_RELATIVE);
        let marker = feedback_dir.join(format!("{}.md.failed", orphan.pr_number));
        let pr_report = feedback_dir.join(format!("{}.md", orphan.pr_number));
        let terminal_status = if run_report_path(orphan, repo_root).exists() {
            TERMINAL_STATUS_COMPLETED
        } else {
            TERMINAL_STATUS_FAILED
        };
        if marker.exists() || pr_report.exists() {
            if settle_meta_status_logged(&orphan.meta_path, terminal_status) {
                outcome.settled_only.push(orphan.pr_number);
            }
            continue;
        }
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body = build_reaper_failed_marker_body(orphan);
        if !write_new_marker_file(&marker, &body) {
            continue;
        }
        settle_meta_status_logged(&orphan.meta_path, terminal_status);
        outcome
            .marked_failed
            .push((orphan.pr_number, orphan.age_secs));
    }
    outcome
}

/// この run **自身**が書いた `feedback-report.md` のパス。
///
/// `reportDirectory` が run ディレクトリの外を指す場合は `<run dir>/reports` へ落とす。
/// 別 run のディレクトリを指す値を信じると、判定根拠を run 単位に閉じた意味が失われる。
fn run_report_path(orphan: &OrphanRun, repo_root: &Path) -> PathBuf {
    let run_dir = orphan
        .meta_path
        .parent()
        .unwrap_or(orphan.meta_path.as_path());
    orphan
        .report_directory
        .as_deref()
        .map(|relative| repo_root.join(relative))
        .filter(|resolved| resolved.starts_with(run_dir))
        .unwrap_or_else(|| run_dir.join(RUN_REPORTS_SUBDIR))
        .join(RUN_REPORT_FILE_NAME)
}

/// `settle_meta_status` の失敗を silent drop せず stderr に残す。戻り値は成功可否。
///
/// 書き換えに失敗しても SessionStart 自体は止めない (best-effort)。ただし失敗を成功として
/// 数えると、nudge が「終端状態へ確定させた」と報告しながら `meta.json` は `running` の
/// まま残り、恒久 block が続いていることに気づけなくなる。
fn settle_meta_status_logged(meta_path: &Path, terminal_status: &str) -> bool {
    match settle_meta_status(meta_path, terminal_status) {
        Ok(()) => true,
        Err(e) => {
            eprintln!(
                "[session-start] [reaper] meta.json の status 確定に失敗 (続行) {}: {}",
                meta_path.display(),
                e
            );
            false
        }
    }
}

/// SessionStart 時の reaper エントリポイント。orphan を検出 + reap し、
/// nudge メッセージを返す。何も検出しなければ `None`。
pub(crate) fn compute_reaper_nudge(repo_root: &Path, now_unix: i64) -> Option<String> {
    let runs_dir = repo_root.join(TAKT_RUNS_DIR);
    let orphans = find_orphan_post_merge_feedback_runs(&runs_dir, now_unix);
    let outcome = reap_orphans(repo_root, &orphans);
    if outcome.marked_failed.is_empty() && outcome.settled_only.is_empty() {
        return None;
    }
    let mut lines = vec!["[POST_MERGE_FEEDBACK_REAPER]".to_string()];
    if !outcome.marked_failed.is_empty() {
        lines.push(format!(
            "orphan post-merge-feedback runs を {} 件検出、`.failed` marker を生成しました \
             (abrupt termination 経路の L2 recovery、ADR-030 §L2)",
            outcome.marked_failed.len()
        ));
        for (pr, age) in &outcome.marked_failed {
            lines.push(format!("  - PR #{} (経過 {} 秒)", pr, age));
        }
    }
    if !outcome.settled_only.is_empty() {
        lines.push(format!(
            "stale な meta.json の status を {} 件終端状態へ確定しました \
             (marker / レポートは既存のため nag なし。放置すると merge pipeline の \
             並行起動 guard を恒久 block する)",
            outcome.settled_only.len()
        ));
        for pr in &outcome.settled_only {
            lines.push(format!("  - PR #{}", pr));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests;
