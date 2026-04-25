//! Post-merge feedback workflow runner (ADR-030 Phase B).
//!
//! 旧 pending file 機構 (ADR-029) を置き換え、takt workflow `post-merge-feedback`
//! を同期実行する決定論的経路を提供する。
//!
//! 入力:
//!
//! - PipelineContext (pr_number, owner_repo)
//!
//! 副作用:
//!
//! - `.takt/post-merge-feedback-context.json` — workflow が読む PR メタデータ
//! - `.takt/post-merge-feedback-transcript.jsonl` — 時刻 range filter 済セッション履歴
//! - takt workflow を spawn (`pnpm exec takt -w post-merge-feedback ...`)
//! - 成功時: `.claude/feedback-reports/<pr>.md` を生成 (takt 出力をコピー)
//! - 失敗時: `.claude/feedback-reports/<pr>.md.failed` marker を残す (soft fail)
//!
//! 失敗時も exit code は変えない (merge は完了済み)。L2 recovery で後続ターンに
//! UserPromptSubmit hook が拾う想定。

use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// takt workflow 名 / task ラベル
const TAKT_WORKFLOW: &str = "post-merge-feedback";
const TAKT_TASK_PREFIX: &str = "post-merge feedback for #";

/// takt 実行のデフォルトタイムアウト (10 分)
pub const TAKT_TIMEOUT_SECS: u64 = 600;

/// run_takt_workflow のポーリング間隔 (ms)
const POLL_INTERVAL_MS: u64 = 500;

/// 出力ファイルの相対パス (リポジトリルートからの相対)
pub const FEEDBACK_DIR: &str = ".claude/feedback-reports";
pub const CONTEXT_PATH: &str = ".takt/post-merge-feedback-context.json";
pub const TRANSCRIPT_PATH: &str = ".takt/post-merge-feedback-transcript.jsonl";

/// post-merge-feedback workflow の入力。
pub struct FeedbackInput<'a> {
    pub pr_number: u64,
    pub owner_repo: &'a str,
    /// リポジトリルート (`.takt/`, `.claude/` の親)。通常は `std::env::current_dir()`。
    pub repo_root: PathBuf,
    /// transcript ファイルが置かれるディレクトリ (`~/.claude/projects/<project-id>/`)。
    /// `None` なら transcript filter をスキップする (空 jsonl を出力)。
    pub transcript_source_dir: Option<PathBuf>,
}

/// PR の時刻 range (gh api の出力から取得)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrTimeRange {
    pub first_commit_time: String,
    pub merged_at: String,
}

/// takt workflow に渡す JSON コンテキスト。
#[derive(Serialize)]
struct WorkflowContext<'a> {
    pr_number: u64,
    owner_repo: &'a str,
    merged_at: &'a str,
    first_commit_time: &'a str,
    transcript_path: &'a str,
    prepush_reports_dir: &'a str,
}

/// `cwd` パス → `~/.claude/projects/` の project ID 形式へ変換する。
///
/// Windows: `E:\work\claude-code-hook-test` → `e--work-claude-code-hook-test`
/// (lowercase、`:` `\` `/` をすべて `-` に置換)。
pub fn cwd_to_project_id(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .to_lowercase()
        .replace([':', '\\', '/'], "-")
}

/// `~/.claude/projects/<project-id>/` を返す。`USERPROFILE` 未設定なら `None`。
pub fn project_transcript_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let project_id = cwd_to_project_id(cwd);
    let dir = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(project_id);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// gh api から PR の `first_commit_time` (oldest commit authoredDate) と `mergedAt` を取得する。
///
/// 失敗時は `Err` を返す (caller 側で fallback)。
pub fn fetch_pr_time_range(pr_number: u64, owner_repo: &str) -> Result<PrTimeRange, String> {
    let pr_str = pr_number.to_string();
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_str,
            "--repo",
            owner_repo,
            "--json",
            "commits,mergedAt",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("gh コマンド起動失敗: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "gh pr view 失敗: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("gh 出力 JSON パース失敗: {}", e))?;

    let merged_at = json
        .get("mergedAt")
        .and_then(|v| v.as_str())
        .ok_or("mergedAt が応答に含まれていません")?
        .to_string();

    let commits = json
        .get("commits")
        .and_then(|v| v.as_array())
        .ok_or("commits が応答に含まれていません")?;

    let first_commit_time = commits
        .iter()
        .filter_map(|c| {
            c.get("authoredDate")
                .or_else(|| c.get("committedDate"))
                .and_then(|v| v.as_str())
        })
        .min()
        .ok_or("commits 配列が空です")?
        .to_string();

    Ok(PrTimeRange {
        first_commit_time,
        merged_at,
    })
}

/// transcript jsonl をフィルタして書き出す。
///
/// 入力: `source_dir` 配下の `*.jsonl`
/// 出力: `out_path` に [first_commit_time, merged_at] かつ type が user/assistant の行のみ
/// 戻り値: 書き込んだ行数
pub fn filter_transcripts(
    source_dir: &Path,
    range: &PrTimeRange,
    out_path: &Path,
) -> Result<usize, String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("出力ディレクトリ作成失敗 {}: {}", parent.display(), e))?;
    }

    let mut writer = fs::File::create(out_path)
        .map(std::io::BufWriter::new)
        .map_err(|e| format!("出力ファイル作成失敗 {}: {}", out_path.display(), e))?;

    let mut written = 0usize;
    let entries = fs::read_dir(source_dir)
        .map_err(|e| format!("transcript dir 読込失敗 {}: {}", source_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue, // best-effort
        };
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if entry_matches_filter(&line, range) {
                writeln!(writer, "{}", line).map_err(|e| format!("出力書込失敗: {}", e))?;
                written += 1;
            }
        }
    }

    writer.flush().map_err(|e| format!("flush 失敗: {}", e))?;
    Ok(written)
}

/// ISO 8601 UTC タイムスタンプを lexicographic 比較用に正規化する。
///
/// `gh api` は秒精度 (`…:SSZ`) を返し、Claude transcript は ms 精度 (`…:SS.fffZ`) を返す。
/// `'.'` (0x2E) < `'Z'` (0x5A) のため、精度が混在すると境界判定が狂う。
/// `Z` 末尾かつ小数部なしの文字列を `…:SS.000Z` に揃えることで同一精度での比較を保証する。
///
/// 入力契約: タイムスタンプは UTC (`Z` 末尾) であること。`+09:00` 等のオフセット形式は
/// このシステムでは現れない前提。
fn normalize_timestamp_for_comparison(ts: &str) -> String {
    if ts.ends_with('Z') && !ts.contains('.') {
        format!("{}.000Z", &ts[..ts.len() - 1])
    } else {
        ts.to_string()
    }
}

/// transcript の 1 行が時刻 range + type filter に該当するかを判定する。
fn entry_matches_filter(line: &str, range: &PrTimeRange) -> bool {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let entry_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(entry_type, "user" | "assistant") {
        return false;
    }

    let timestamp = match value.get("timestamp").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return false,
    };

    let ts = normalize_timestamp_for_comparison(timestamp);
    let lower = normalize_timestamp_for_comparison(range.first_commit_time.as_str());
    let upper = normalize_timestamp_for_comparison(range.merged_at.as_str());
    ts >= lower && ts <= upper
}

/// `.takt/runs/` 配下で `suffix` に一致する最新ディレクトリ (lex-sort 末尾) を返す。
fn find_latest_run_dir(runs_dir: &Path, suffix: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = fs::read_dir(runs_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name.ends_with(suffix) {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next_back()
}

/// `.takt/runs/*-pre-push-review/reports/` のうち最新 (lex-sort 末尾) を返す。
pub fn find_latest_prepush_reports_dir(repo_root: &Path) -> Option<PathBuf> {
    let runs_dir = repo_root.join(".takt").join("runs");
    let latest = find_latest_run_dir(&runs_dir, "-pre-push-review")?;
    let reports = latest.join("reports");
    if reports.is_dir() {
        Some(reports)
    } else {
        None
    }
}

/// context file (workflow が Read で読む) を書き出す。
pub fn write_context_file(
    out_path: &Path,
    pr_number: u64,
    owner_repo: &str,
    range: &PrTimeRange,
    transcript_relpath: &str,
    prepush_reports_dir: &str,
) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("context dir 作成失敗 {}: {}", parent.display(), e))?;
    }
    let ctx = WorkflowContext {
        pr_number,
        owner_repo,
        merged_at: &range.merged_at,
        first_commit_time: &range.first_commit_time,
        transcript_path: transcript_relpath,
        prepush_reports_dir,
    };
    let json = serde_json::to_string_pretty(&ctx)
        .map_err(|e| format!("context JSON serialize 失敗: {}", e))?;
    fs::write(out_path, json).map_err(|e| format!("context 書込失敗: {}", e))
}

/// takt workflow を spawn し、終了まで待つ。
///
/// stdio は inherit (push-runner / pr-monitor と同じパターン)。
/// timeout 経過時は kill して false を返す。
pub fn run_takt_workflow(repo_root: &Path, pr_number: u64, timeout_secs: u64) -> bool {
    let task_label = format!("{}{}", TAKT_TASK_PREFIX, pr_number);
    let mut child = match Command::new("pnpm")
        .args(["exec", "takt", "-w", TAKT_WORKFLOW, "-t", &task_label])
        .current_dir(repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let exited_success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.success()),
            Ok(None) if std::time::Instant::now() >= deadline => break None,
            Err(_) => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
        }
    };

    match exited_success {
        Some(success) => success,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    }
}

/// takt 完了後、最新 run dir の `feedback-report.md` を `.claude/feedback-reports/<pr>.md` にコピーする。
pub fn copy_feedback_report(repo_root: &Path, pr_number: u64) -> Result<PathBuf, String> {
    let runs_dir = repo_root.join(".takt").join("runs");
    let latest = find_latest_run_dir(&runs_dir, &format!("-{}", TAKT_WORKFLOW))
        .ok_or("post-merge-feedback の run dir が見つかりません")?;

    let source = latest.join("reports").join("feedback-report.md");
    if !source.is_file() {
        return Err(format!(
            "feedback-report.md が見つかりません: {}",
            source.display()
        ));
    }

    let target_dir = repo_root.join(FEEDBACK_DIR);
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("feedback dir 作成失敗 {}: {}", target_dir.display(), e))?;
    let target = target_dir.join(format!("{}.md", pr_number));
    fs::copy(&source, &target).map_err(|e| {
        format!(
            "コピー失敗 {} → {}: {}",
            source.display(),
            target.display(),
            e
        )
    })?;
    Ok(target)
}

/// `.failed` marker を書き出す (L2 recovery が拾う前提)。
pub fn write_failed_marker(
    repo_root: &Path,
    pr_number: u64,
    reason: &str,
) -> Result<PathBuf, String> {
    let dir = repo_root.join(FEEDBACK_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("feedback dir 作成失敗 {}: {}", dir.display(), e))?;
    let path = dir.join(format!("{}.md.failed", pr_number));
    let body = format!(
        "# post-merge-feedback failed (PR #{})\n\n\
         takt workflow `{}` の同期実行が失敗しました。\n\n\
         ## 失敗理由\n\n{}\n\n\
         ## 復旧手順\n\n\
         1. このマーカー (`{}`) を残したまま、Claude Code セッションで何か入力する\n\
         2. UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) が検出し、再実行を促す\n\
         3. または直接: `pnpm feedback-retry {}` (Phase C 以降で実装)\n",
        pr_number,
        TAKT_WORKFLOW,
        reason,
        path.display(),
        pr_number,
    );
    fs::write(&path, body).map_err(|e| format!("failed marker 書込失敗: {}", e))?;
    Ok(path)
}

/// 成功時に `.failed` marker が残っていたら削除する。
fn cleanup_failed_marker(repo_root: &Path, pr_number: u64) {
    let path = repo_root
        .join(FEEDBACK_DIR)
        .join(format!("{}.md.failed", pr_number));
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_id_windows_drive() {
        let p = Path::new("E:\\work\\claude-code-hook-test");
        assert_eq!(cwd_to_project_id(p), "e--work-claude-code-hook-test");
    }

    #[test]
    fn project_id_unix_path() {
        let p = Path::new("/home/user/project");
        assert_eq!(cwd_to_project_id(p), "-home-user-project");
    }

    #[test]
    fn entry_matches_user_in_range() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_assistant_outside_range() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"assistant","timestamp":"2026-04-25T11:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_queue_operation() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_attachment() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let line = r#"{"type":"attachment","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_invalid_json() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        assert!(!entry_matches_filter("not-json", &range));
    }

    #[test]
    fn entry_includes_boundary_timestamps() {
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        let upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.000Z"}"#;
        assert!(entry_matches_filter(lower, &range));
        assert!(entry_matches_filter(upper, &range));
    }

    // gh api は秒精度 (`Z`), transcript は ms 精度 (`.000Z`) を返すため
    // 精度が混在しても境界判定が正しく動くことを保証するリグレッションテスト。
    #[test]
    fn entry_includes_lower_boundary_with_mixed_precision() {
        // first_commit_time が秒精度 (Z 末尾), entry が ms 精度 (.000Z)
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00Z".into(),
            merged_at: "2026-04-25T10:00:00Z".into(),
        };
        // 下限境界: entry timestamp == first_commit_time (ms = 0) → 含まれるべき
        let at_lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        assert!(entry_matches_filter(at_lower, &range));
    }

    #[test]
    fn entry_excludes_past_upper_boundary_with_mixed_precision() {
        // merged_at が秒精度 (Z 末尾), entry が ms 精度 (.000Z)
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00Z".into(),
            merged_at: "2026-04-25T10:00:00Z".into(),
        };
        // 上限超過: entry timestamp > merged_at (500ms 後) → 含まれないべき
        let past_upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.500Z"}"#;
        assert!(!entry_matches_filter(past_upper, &range));
    }

    #[test]
    fn filter_transcripts_writes_only_in_range() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();

        let session_path = dir.join("session-a.jsonl");
        let mut content = String::new();
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T07:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"assistant","timestamp":"2026-04-25T09:30:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T11:00:00.000Z"}"#);
        content.push('\n');
        fs::write(&session_path, content).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        let written = filter_transcripts(&dir, &range, &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("09:00:00"));
        assert!(out.contains("09:30:00"));
        assert!(!out.contains("07:00:00"));
        assert!(!out.contains("11:00:00"));
        assert!(!out.contains("queue-operation"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_context_file_serializes_fields() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("context.json");
        let range = PrTimeRange {
            first_commit_time: "2026-04-25T08:00:00.000Z".into(),
            merged_at: "2026-04-25T10:00:00.000Z".into(),
        };
        write_context_file(
            &out,
            42,
            "owner/repo",
            &range,
            ".takt/transcript.jsonl",
            ".takt/runs/foo/reports",
        )
        .unwrap();

        let raw = fs::read_to_string(&out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.get("pr_number").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            parsed.get("owner_repo").and_then(|v| v.as_str()),
            Some("owner/repo")
        );
        assert_eq!(
            parsed.get("merged_at").and_then(|v| v.as_str()),
            Some("2026-04-25T10:00:00.000Z")
        );
        assert_eq!(
            parsed.get("transcript_path").and_then(|v| v.as_str()),
            Some(".takt/transcript.jsonl")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_failed_marker_creates_file() {
        let root = std::env::temp_dir().join(format!(
            "feedback-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&root).unwrap();
        let path = write_failed_marker(&root, 7, "takt timeout (10 minutes)").unwrap();
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("PR #7"));
        assert!(body.contains("takt timeout (10 minutes)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_failed_marker_removes_existing() {
        let root = std::env::temp_dir().join(format!(
            "feedback-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(root.join(FEEDBACK_DIR)).unwrap();
        let marker = root.join(FEEDBACK_DIR).join("5.md.failed");
        fs::write(&marker, "old failure").unwrap();
        assert!(marker.exists());
        cleanup_failed_marker(&root, 5);
        assert!(!marker.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_latest_prepush_picks_lexicographic_max() {
        let root = std::env::temp_dir().join(format!(
            "feedback-prepush-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        let runs = root.join(".takt").join("runs");
        fs::create_dir_all(runs.join("20260425-000000-pre-push-review").join("reports")).unwrap();
        fs::create_dir_all(runs.join("20260425-094925-pre-push-review").join("reports")).unwrap();
        fs::create_dir_all(runs.join("20260425-100000-other-workflow").join("reports")).unwrap();

        let latest = find_latest_prepush_reports_dir(&root).unwrap();
        assert!(latest
            .to_string_lossy()
            .contains("20260425-094925-pre-push-review"));

        let _ = fs::remove_dir_all(&root);
    }
}

/// 全工程を実行する高水準エントリポイント。
///
/// 失敗時は `Err(reason)` を返す。caller は `write_failed_marker` で marker を残す前提。
/// 成功時は `Ok(report_path)` (生成された feedback report の絶対パス)。
pub fn run(input: &FeedbackInput) -> Result<PathBuf, String> {
    let range = fetch_pr_time_range(input.pr_number, input.owner_repo)
        .map_err(|e| format!("PR 時刻 range 取得失敗: {}", e))?;

    let context_path = input.repo_root.join(CONTEXT_PATH);
    let transcript_path = input.repo_root.join(TRANSCRIPT_PATH);

    let written = match input.transcript_source_dir.as_ref() {
        Some(dir) => filter_transcripts(dir, &range, &transcript_path)
            .map_err(|e| format!("transcript filter 失敗: {}", e))?,
        None => {
            // ソース dir 不明: 空 jsonl を出力 (facet が「データなし」分岐に進む)
            if let Some(parent) = transcript_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&transcript_path, "");
            0
        }
    };
    eprintln!(
        "[merge-pipeline] [feedback] transcript filter 完了 ({} entries → {})",
        written,
        transcript_path.display()
    );

    let prepush_dir = find_latest_prepush_reports_dir(&input.repo_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    write_context_file(
        &context_path,
        input.pr_number,
        input.owner_repo,
        &range,
        TRANSCRIPT_PATH,
        &prepush_dir,
    )?;

    if !run_takt_workflow(&input.repo_root, input.pr_number, TAKT_TIMEOUT_SECS) {
        return Err(format!(
            "takt workflow `{}` が失敗または timeout しました",
            TAKT_WORKFLOW
        ));
    }

    let report = copy_feedback_report(&input.repo_root, input.pr_number)?;
    cleanup_failed_marker(&input.repo_root, input.pr_number);
    Ok(report)
}
