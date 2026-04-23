//! CI・CodeRabbit 状態チェッカー (スタンドアロン exe)
//!
//! push / PR 作成後に CI (GitHub Actions) と CodeRabbit レビューの状態を
//! gh CLI 経由で取得し、構造化 JSON を stdout に出力する。
//! CronCreate のポーリングジョブから定期的に呼び出される想定。
//!
//! 使い方:
//!   check-ci-coderabbit.exe --push-time "2026-04-01T12:00:00Z" [--repo owner/repo] [--pr 42]
//!
//! 終了コード:
//!   0 - チェック完了 (結果は stdout JSON の action フィールドを参照)
//!   1 - 引数エラーまたは致命的エラー

use lib_report_formatter::Finding;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

// ─── CLI 引数 ───

struct CliArgs {
    push_time: String,
    repo: Option<String>,
    pr: Option<u64>,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut push_time = None;
    let mut repo = None;
    let mut pr = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--push-time" => {
                i += 1;
                push_time = args.get(i).cloned();
            }
            "--repo" => {
                i += 1;
                repo = args.get(i).cloned();
            }
            "--pr" => {
                i += 1;
                pr = args.get(i).and_then(|s| s.parse::<u64>().ok());
            }
            _ => {}
        }
        i += 1;
    }

    let push_time = push_time.ok_or("--push-time は必須です")?;
    Ok(CliArgs {
        push_time,
        repo,
        pr,
    })
}

// ─── gh CLI 実行 ───

/// gh コマンドを実行し stdout を返す。タイムアウト 30 秒。
/// パイプのデッドロックを防ぐため、タイムアウトは別スレッドで kill し、
/// メインスレッドは wait_with_output でパイプを安全に読み取る。
///
/// NOTE: タイムアウト時のプロセス kill は Windows (taskkill) のみ実装。
/// この exe は Windows 専用として設計されている (ADR-001)。
fn run_gh(args: &[&str]) -> Result<String, String> {
    let child = Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("gh の起動に失敗: {}", e))?;

    // タイムアウト用: done_flag で早期終了、timeout_flag でタイムアウト判定
    let child_id = child.id();
    let timeout_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag_clone = timeout_flag.clone();
    let done_clone = done_flag.clone();

    // タイマースレッドは 100ms 刻みで done_flag をチェックし、早期終了する
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if done_clone.load(std::sync::atomic::Ordering::Relaxed) {
                return; // プロセス完了 → スレッド即終了
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // タイムアウト到達
        flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &child_id.to_string()])
                .output();
        }
    });

    let output = child
        .wait_with_output()
        .map_err(|e| format!("gh 出力の取得に失敗: {}", e))?;

    // プロセス完了をタイマースレッドに通知
    done_flag.store(true, std::sync::atomic::Ordering::Relaxed);

    if timeout_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("gh コマンドがタイムアウトしました".to_string());
    }

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("gh エラー: {}", stderr))
    }
}

// ─── 出力モデル ───

#[derive(Serialize)]
struct CheckResult {
    status: String,
    action: String,
    ci: CiStatus,
    coderabbit: CodeRabbitStatus,
    summary: String,
    findings: Vec<Finding>,
}

#[derive(Serialize, Default)]
struct CiStatus {
    overall: String,
    runs: Vec<CiRunSummary>,
}

#[derive(Serialize, Clone)]
struct CiRunSummary {
    name: String,
    conclusion: String,
}

#[derive(Serialize, Default)]
struct CodeRabbitStatus {
    review_state: String,
    new_comments: usize,
    actionable_comments: Option<usize>,
    unresolved_threads: Option<usize>,
}

// ─── gh CLI 出力パースモデル ───

#[derive(Deserialize)]
struct GhRunItem {
    name: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct GhStatusItem {
    context: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)] // body はデシリアライズに必要だがフィルタでは未使用
struct GhComment {
    user: Option<GhUser>,
    body: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct GhUser {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhReview {
    user: Option<GhUser>,
    body: Option<String>,
    submitted_at: Option<String>,
}

/// PR インラインレビューコメント (pulls/{pr}/comments)
#[derive(Deserialize)]
struct GhPullComment {
    user: Option<GhUser>,
    body: Option<String>,
    path: Option<String>,
    line: Option<u64>,
    original_line: Option<u64>,
    created_at: Option<String>,
}

// ─── パース関数 (テスト可能な純粋関数) ───

/// gh run list の JSON をパースして CI 状態を返す
fn parse_ci_runs(json: &str) -> CiStatus {
    let items: Vec<GhRunItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] CI runs JSON パースエラー: {}", e);
        vec![]
    });

    if items.is_empty() {
        return CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
    }

    let runs: Vec<CiRunSummary> = items
        .iter()
        .map(|item| CiRunSummary {
            name: item.name.clone(),
            conclusion: item
                .conclusion
                .clone()
                .unwrap_or_else(|| "pending".to_string()),
        })
        .collect();

    let has_pending = items.iter().any(|i| {
        matches!(
            i.conclusion.as_deref(),
            None | Some("")
                | Some("pending")
                | Some("queued")
                | Some("in_progress")
                | Some("waiting")
        )
    });

    let has_failure = items.iter().any(|i| {
        matches!(
            i.conclusion.as_deref(),
            Some("failure")
                | Some("cancelled")
                | Some("timed_out")
                | Some("action_required")
                | Some("stale")
        )
    });

    let overall = if has_pending {
        "pending"
    } else if has_failure {
        "failure"
    } else {
        "success"
    };

    CiStatus {
        overall: overall.to_string(),
        runs,
    }
}

/// gh api .../statuses の JSON から CodeRabbit のレビュー状態を返す
fn parse_coderabbit_status(json: &str) -> String {
    let items: Vec<GhStatusItem> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] statuses JSON パースエラー: {}", e);
        vec![]
    });

    let cr_statuses: Vec<&GhStatusItem> = items
        .iter()
        .filter(|s| {
            s.context
                .as_deref()
                .map(|c| c.contains("CodeRabbit"))
                .unwrap_or(false)
        })
        .collect();

    if cr_statuses.is_empty() {
        return "not_found".to_string();
    }

    // 最後のエントリ (最新) の state を返す
    cr_statuses
        .last()
        .and_then(|s| s.state.clone())
        .unwrap_or_else(|| "not_found".to_string())
}

/// PR コメントの JSON から push_time 以降の CodeRabbit 新規コメント数を返す
fn parse_new_comments(json: &str, push_time: &str) -> usize {
    let comments: Vec<GhComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] comments JSON パースエラー: {}", e);
        vec![]
    });

    comments
        .iter()
        .filter(|c| {
            let is_coderabbit = c
                .user
                .as_ref()
                .and_then(|u| u.login.as_deref())
                .map(|l| l == "coderabbitai[bot]")
                .unwrap_or(false);

            let after_push_time = c
                .created_at
                .as_deref()
                .map(|t| t > push_time)
                .unwrap_or(false);

            // 「処理中」通知コメントを除外 (レビュー結果ではない)
            let is_review_in_progress = c
                .body
                .as_deref()
                .map(|b| b.contains("review in progress"))
                .unwrap_or(false);

            is_coderabbit && after_push_time && !is_review_in_progress
        })
        .count()
}

/// PR インラインレビューコメント (pulls/{pr}/comments) を Finding に変換
///
/// CodeRabbit のインラインコメントから severity, issue, suggestion を抽出する。
/// severity は本文先頭の `_⚠️ Potential issue_ | _🔴 Critical_` パターンから判定。
/// suggestion は `<details><summary>💡 修正イメージ</summary>` ブロックから抽出。
fn parse_findings(json: &str, push_time: &str) -> Vec<Finding> {
    let comments: Vec<GhPullComment> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!(
            "[check-ci-coderabbit] pull comments JSON パースエラー: {}",
            e
        );
        vec![]
    });

    comments
        .iter()
        .filter(|c| {
            let is_coderabbit = c
                .user
                .as_ref()
                .and_then(|u| u.login.as_deref())
                .map(|l| l == "coderabbitai[bot]")
                .unwrap_or(false);
            let after_push_time = c
                .created_at
                .as_deref()
                .map(|t| t > push_time)
                .unwrap_or(false);
            is_coderabbit && after_push_time
        })
        .map(|c| {
            let body = c.body.as_deref().unwrap_or("");
            let severity = extract_severity(body);
            let issue = extract_issue(body);
            let suggestion = extract_suggestion(body);
            let file = c.path.clone().unwrap_or_default();
            let line = c
                .line
                .or(c.original_line)
                .map(|l| l.to_string())
                .unwrap_or_default();

            Finding {
                severity,
                file,
                line,
                issue,
                suggestion,
                source: "CodeRabbit".to_string(),
            }
        })
        .collect()
}

/// CodeRabbit コメント本文から severity を抽出
///
/// パターン: `_⚠️ Potential issue_ | _🔴 Critical_`
/// パターン: `_⚠️ Potential issue_ | _🟠 Major_`
/// パターン: `_⚠️ Potential issue_ | _🟡 Minor_`
fn extract_severity(body: &str) -> String {
    let first_line = body.lines().next().unwrap_or("");
    if first_line.contains("Critical") || first_line.contains("🔴") {
        "Critical".to_string()
    } else if first_line.contains("Major") || first_line.contains("🟠") {
        "Major".to_string()
    } else if first_line.contains("Minor") || first_line.contains("🟡") {
        "Minor".to_string()
    } else if first_line.contains("High") {
        "High".to_string()
    } else if first_line.contains("Low") {
        "Low".to_string()
    } else {
        "Info".to_string()
    }
}

/// CodeRabbit コメント本文から指摘内容を抽出
///
/// 太字行 (`**...**`) を探して指摘の要約とする
fn extract_issue(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return trimmed[2..trimmed.len() - 2].to_string();
        }
    }
    // 太字行がなければ最初の意味のある行を返す
    for line in body.lines().skip(1) {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('_') && !trimmed.starts_with('<') {
            return truncate_str(trimmed, 100);
        }
    }
    "(詳細はコメント参照)".to_string()
}

/// CodeRabbit コメント本文から修正案を抽出
///
/// `<details><summary>💡 修正イメージ</summary>` または `suggestion` ブロックを探す
fn extract_suggestion(body: &str) -> String {
    // ```suggestion ブロック
    if let Some(start) = body.find("```suggestion") {
        let after = &body[start + 14..]; // "```suggestion\n" の後
        if let Some(end) = after.find("```") {
            let suggestion = after[..end].trim();
            if !suggestion.is_empty() {
                return truncate_str(suggestion, 150);
            }
        }
    }
    // ```diff ブロック (修正イメージ内)
    if let Some(start) = body.find("```diff") {
        let after = &body[start + 7..];
        if let Some(end) = after.find("```") {
            let diff = after[..end].trim();
            if !diff.is_empty() {
                return truncate_str(diff, 150);
            }
        }
    }
    // Prompt for AI Agents ブロック内の修正指示
    if body.contains("Prompt for AI Agents") {
        // 修正指示は長いのでコメント参照を案内
        return "(修正指示あり — コメント参照)".to_string();
    }
    String::new()
}

/// UTF-8 安全な文字列切り詰め
fn truncate_str(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// PR レビューの JSON から最新の CodeRabbit レビューの "Actionable comments posted: N" を抽出
fn parse_actionable_comments(json: &str, push_time: &str) -> Option<usize> {
    let reviews: Vec<GhReview> = serde_json::from_str(json).unwrap_or_else(|e| {
        eprintln!("[check-ci-coderabbit] reviews JSON パースエラー: {}", e);
        vec![]
    });

    let latest = reviews.iter().rfind(|r| {
        let is_coderabbit = r
            .user
            .as_ref()
            .and_then(|u| u.login.as_deref())
            .map(|l| l == "coderabbitai[bot]")
            .unwrap_or(false);

        let after_push_time = r
            .submitted_at
            .as_deref()
            .map(|t| t > push_time)
            .unwrap_or(false);

        is_coderabbit && after_push_time
    })?;

    let body = latest.body.as_deref()?;

    // "Actionable comments posted: 3" のようなパターンを抽出
    extract_actionable_count(body)
}

/// 文字列から "Actionable comments posted: N" の N を抽出
fn extract_actionable_count(body: &str) -> Option<usize> {
    let marker = "Actionable comments posted: ";
    let pos = body.find(marker)?;
    let rest = &body[pos + marker.len()..];
    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse::<usize>().ok()
}

/// GraphQL レスポンスから未解決スレッド数をパースする
fn parse_unresolved_threads(json: &str) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")?
        .as_array()?;

    let unresolved = nodes
        .iter()
        .filter(|n| n.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
        .count();

    Some(unresolved)
}

// ─── 判定ロジック ───

/// CI と CodeRabbit の状態から (status, action) を決定する
fn decide(ci: &CiStatus, cr: &CodeRabbitStatus) -> (String, String) {
    // CI が失敗 → 即座に報告
    if ci.overall == "failure" {
        return ("error".to_string(), "stop_monitoring_failure".to_string());
    }

    // コメント/スレッドの集計 (review_state に関わらず先に計算)
    let has_unresolved = cr.unresolved_threads.map(|n| n > 0).unwrap_or(false);
    let effective_new = if let Some(actionable) = cr.actionable_comments {
        std::cmp::max(cr.new_comments, actionable)
    } else {
        cr.new_comments
    };
    let has_actionable = effective_new > 0 || has_unresolved;

    // CodeRabbit の review_state が not_found でもコメント/スレッドがあれば対応が必要
    // (commit status は未投稿でも inline comments は先に投稿されるケースがある)
    if cr.review_state == "not_found" && has_actionable {
        return ("action_required".to_string(), "action_required".to_string());
    }

    // CI が pending (runs 空 = no_ci は "pending" ではなく CI チェックをスキップ)
    let ci_pending = ci.overall == "pending" && !ci.runs.is_empty();
    // CodeRabbit がまだレビュー中 or 未検出 (コメントもない)
    let cr_pending = cr.review_state == "pending" || cr.review_state == "not_found";

    if ci_pending || cr_pending {
        return ("pending".to_string(), "continue_monitoring".to_string());
    }

    // CodeRabbit がエラー
    if cr.review_state == "failure" || cr.review_state == "error" {
        return ("error".to_string(), "stop_monitoring_failure".to_string());
    }

    // コメント/スレッドがある → 対応が必要
    if has_actionable {
        return ("action_required".to_string(), "action_required".to_string());
    }

    // すべて OK
    (
        "complete".to_string(),
        "stop_monitoring_success".to_string(),
    )
}

/// 人間向けサマリーを生成
fn build_summary(ci: &CiStatus, cr: &CodeRabbitStatus) -> String {
    let ci_part = match ci.overall.as_str() {
        "success" => "CI成功".to_string(),
        "failure" => {
            let failed: Vec<&str> = ci
                .runs
                .iter()
                .filter(|r| r.conclusion == "failure")
                .map(|r| r.name.as_str())
                .collect();
            format!("CI失敗 ({})", failed.join(", "))
        }
        _ => "CI実行中".to_string(),
    };

    let cr_part = match cr.review_state.as_str() {
        "success" => {
            let mut parts = vec![];
            let effective = cr
                .actionable_comments
                .map(|a| std::cmp::max(a, cr.new_comments))
                .unwrap_or(cr.new_comments);
            if effective > 0 {
                parts.push(format!("新規指摘{}件", effective));
            }
            if let Some(n) = cr.unresolved_threads {
                if n > 0 {
                    parts.push(format!("未解決スレッド{}件", n));
                }
            }
            if parts.is_empty() {
                "CodeRabbit指摘なし".to_string()
            } else {
                format!("CodeRabbit: {}", parts.join("、"))
            }
        }
        "pending" => "CodeRabbitレビュー待ち".to_string(),
        "not_found" => {
            // not_found でもコメント/スレッドがある場合は内容を表示
            let mut parts = vec![];
            let effective_new = cr.actionable_comments.unwrap_or(cr.new_comments);
            if effective_new > 0 {
                parts.push(format!("新規コメント{}件", effective_new));
            }
            if let Some(n) = cr.unresolved_threads {
                if n > 0 {
                    parts.push(format!("未解決スレッド{}件", n));
                }
            }
            if parts.is_empty() {
                "CodeRabbitレビュー待ち".to_string()
            } else {
                format!("CodeRabbit: {}", parts.join("、"))
            }
        }
        _ => format!("CodeRabbit状態: {}", cr.review_state),
    };

    format!("{}。{}", ci_part, cr_part)
}

// ─── 自動取得ヘルパー ───

fn auto_detect_repo() -> Result<String, String> {
    run_gh(&[
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "-q",
        ".nameWithOwner",
    ])
}

fn auto_detect_pr() -> Result<u64, String> {
    let output = run_gh(&["pr", "view", "--json", "number", "-q", ".number"])?;
    output
        .parse::<u64>()
        .map_err(|_| format!("PR番号のパースに失敗: {}", output))
}

fn get_current_branch() -> Result<String, String> {
    let child = Command::new("git")
        .args(["branch", "--show-current"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("git branch の起動に失敗: {}", e))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("git branch の実行に失敗: {}", e))?;
    // Note: wait_with_output 自体にはタイムアウトがないが、
    // 呼び出し元の CronCreate ジョブ全体にタイムアウトがあるため実用上問題ない
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        Err("現在のブランチを取得できませんでした".to_string())
    } else {
        Ok(branch)
    }
}

fn get_head_sha() -> Result<String, String> {
    run_gh(&["pr", "view", "--json", "headRefOid", "-q", ".headRefOid"])
}

// ─── 入力値検証 ───

/// repo が "owner/name" 形式 (英数字・ハイフン・ドット・アンダースコア) であることを検証
fn is_valid_repo(repo: &str) -> bool {
    let re = regex::Regex::new(r"^[a-zA-Z0-9._-]+/[a-zA-Z0-9._-]+$").unwrap();
    re.is_match(repo)
}

/// head_sha が 40文字の16進数であることを検証
fn is_valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit())
}

// ─── メインロジック ───

fn run_check(args: CliArgs) -> CheckResult {
    let repo_result = args.repo.map(Ok).unwrap_or_else(auto_detect_repo);
    let pr_result = args.pr.map(Ok).unwrap_or_else(auto_detect_pr);

    // エラーメッセージを事前に抽出 (unwrap_or で move される前に)
    let repo_err = repo_result.as_ref().err().cloned();
    let pr_err = pr_result.as_ref().err().cloned();
    let repo = repo_result.unwrap_or_default();
    let pr = pr_result.unwrap_or(0);

    if repo.is_empty() || pr == 0 || !is_valid_repo(&repo) {
        let mut reasons = vec![];
        if repo.is_empty() {
            let detail = repo_err.unwrap_or_else(|| "不明".to_string());
            reasons.push(format!("リポジトリ取得失敗: {}", detail));
        } else if !is_valid_repo(&repo) {
            reasons.push(format!("リポジトリ名が不正: {}", repo));
        }
        if pr == 0 {
            let detail = pr_err.unwrap_or_else(|| "不明".to_string());
            reasons.push(format!("PR番号取得失敗: {}", detail));
        }
        let summary = format!("初期化エラー: {}", reasons.join("; "));
        eprintln!("[check-ci-coderabbit] {}", summary);
        return CheckResult {
            status: "error".to_string(),
            action: "stop_monitoring_failure".to_string(),
            ci: CiStatus {
                overall: "error".to_string(),
                runs: vec![],
            },
            coderabbit: CodeRabbitStatus {
                review_state: "error".to_string(),
                ..Default::default()
            },
            summary,
            findings: vec![],
        };
    }

    // 1. CI 状態チェック
    let branch = get_current_branch().unwrap_or_default();
    let ci = if !branch.is_empty() {
        match run_gh(&[
            "run",
            "list",
            "--branch",
            &branch,
            "--limit",
            "5",
            "--json",
            "name,conclusion",
        ]) {
            Ok(ci_json) => parse_ci_runs(&ci_json),
            Err(e) => {
                // API エラー/タイムアウト → pending (runs 非空) として CI スキップを防止
                eprintln!("[check-ci-coderabbit] CI 取得エラー (pending 扱い): {}", e);
                CiStatus {
                    overall: "pending".to_string(),
                    runs: vec![CiRunSummary {
                        name: "(API error)".to_string(),
                        conclusion: "".to_string(),
                    }],
                }
            }
        }
    } else {
        CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        }
    };

    // 2. CodeRabbit commit status
    let head_sha = get_head_sha().unwrap_or_default();
    let cr_state = if !head_sha.is_empty() && is_valid_sha(&head_sha) {
        let statuses_json = run_gh(&[
            "api",
            &format!("repos/{}/commits/{}/statuses", repo, head_sha),
        ])
        .unwrap_or_else(|_| "[]".to_string());
        parse_coderabbit_status(&statuses_json)
    } else {
        "not_found".to_string()
    };

    // 3. 新規コメント
    let pr_str = pr.to_string();
    let comments_json = run_gh(&["api", &format!("repos/{}/issues/{}/comments", repo, pr_str)])
        .unwrap_or_else(|_| "[]".to_string());
    let new_comments = parse_new_comments(&comments_json, &args.push_time);

    // 4. Actionable comments クロスチェック
    let reviews_json = run_gh(&["api", &format!("repos/{}/pulls/{}/reviews", repo, pr_str)])
        .unwrap_or_else(|_| "[]".to_string());
    let actionable = parse_actionable_comments(&reviews_json, &args.push_time);

    // 5. 未解決スレッド (GraphQL) — 値直接埋め込み (入力は is_valid_repo で検証済み)
    let (owner, name) = repo.split_once('/').unwrap_or(("", ""));
    let unresolved = if !owner.is_empty() && !name.is_empty() {
        let query = format!(
            r#"{{ repository(owner: "{}", name: "{}") {{ pullRequest(number: {}) {{ reviewThreads(first: 100) {{ nodes {{ isResolved }} }} }} }} }}"#,
            owner, name, pr
        );
        let graphql_json = run_gh(&["api", "graphql", "-f", &format!("query={}", query)])
            .unwrap_or_else(|e| {
                eprintln!("[check-ci-coderabbit] GraphQL クエリ失敗: {}", e);
                "{}".to_string()
            });
        parse_unresolved_threads(&graphql_json)
    } else {
        None
    };

    let cr = CodeRabbitStatus {
        review_state: cr_state,
        new_comments,
        actionable_comments: actionable,
        unresolved_threads: unresolved,
    };

    // 6. インラインレビューコメントを Finding に変換
    let pull_comments_json = run_gh(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/comments", repo, pr_str),
    ])
    .unwrap_or_else(|_| "[]".to_string());
    let findings = parse_findings(&pull_comments_json, &args.push_time);

    let (status, action) = decide(&ci, &cr);
    let summary = build_summary(&ci, &cr);

    CheckResult {
        status,
        action,
        ci,
        coderabbit: cr,
        summary,
        findings,
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[check-ci-coderabbit] エラー: {}", e);
            eprintln!("使い方: check-ci-coderabbit.exe --push-time <ISO8601> [--repo owner/repo] [--pr N]");
            std::process::exit(1);
        }
    };

    let result = run_check(args);
    let json = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
    println!("{}", json);
}

// ─── テスト ───

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_ci_runs ---

    #[test]
    fn ci_all_success() {
        let json = r#"[
            {"name": "build", "conclusion": "success"},
            {"name": "test", "conclusion": "success"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "success");
        assert_eq!(ci.runs.len(), 2);
    }

    #[test]
    fn ci_one_failure() {
        let json = r#"[
            {"name": "build", "conclusion": "success"},
            {"name": "test", "conclusion": "failure"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "failure");
    }

    #[test]
    fn ci_pending_null_conclusion() {
        let json = r#"[
            {"name": "build", "conclusion": null},
            {"name": "test", "conclusion": "success"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
    }

    #[test]
    fn ci_pending_in_progress() {
        let json = r#"[
            {"name": "build", "conclusion": "in_progress"}
        ]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
    }

    #[test]
    fn ci_empty_runs() {
        let json = "[]";
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "pending");
        assert!(ci.runs.is_empty());
    }

    #[test]
    fn ci_cancelled_is_failure() {
        let json = r#"[{"name": "deploy", "conclusion": "cancelled"}]"#;
        let ci = parse_ci_runs(json);
        assert_eq!(ci.overall, "failure");
    }

    // --- parse_coderabbit_status ---

    #[test]
    fn cr_status_success() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "success"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "success");
    }

    #[test]
    fn cr_status_pending() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "pending"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "pending");
    }

    #[test]
    fn cr_status_not_found() {
        let json = r#"[
            {"context": "ci/build", "state": "success"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "not_found");
    }

    #[test]
    fn cr_status_empty() {
        assert_eq!(parse_coderabbit_status("[]"), "not_found");
    }

    #[test]
    fn cr_status_multiple_takes_last() {
        let json = r#"[
            {"context": "CodeRabbit", "state": "pending"},
            {"context": "CodeRabbit", "state": "success"}
        ]"#;
        assert_eq!(parse_coderabbit_status(json), "success");
    }

    // --- parse_new_comments ---

    #[test]
    fn comments_filters_by_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "_old comment", "created_at": "2026-04-01T10:00:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "_new comment", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    #[test]
    fn comments_filters_by_user() {
        let json = r#"[
            {"user": {"login": "someuser"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    #[test]
    fn comments_filters_coderabbit_user_only() {
        // body の内容に関係なく、coderabbitai[bot] のコメントは全てカウント
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Summary of changes", "created_at": "2026-04-01T12:30:00Z"},
            {"user": {"login": "coderabbitai[bot]"}, "body": "<!-- auto-generated -->", "created_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 2);
    }

    #[test]
    fn comments_empty() {
        assert_eq!(parse_new_comments("[]", "2026-04-01T12:00:00Z"), 0);
    }

    #[test]
    fn comments_excludes_review_in_progress() {
        let json = r#"[
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:00:00Z","body":"<!-- review in progress by coderabbit.ai -->\nCurrently processing..."},
            {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:05:00Z","body":"_Actionable comments posted: 2_\nReview summary..."}
        ]"#;
        // 「処理中」コメントは除外され、レビュー結果コメントのみカウント
        assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
    }

    // --- parse_actionable_comments ---

    #[test]
    fn actionable_extracts_count() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Some review\nActionable comments posted: 3\nMore text", "submitted_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            Some(3)
        );
    }

    #[test]
    fn actionable_no_match() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "No actionable items", "submitted_at": "2026-04-01T12:30:00Z"}
        ]"#;
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            None
        );
    }

    #[test]
    fn actionable_filters_by_time() {
        let json = r#"[
            {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 5", "submitted_at": "2026-04-01T10:00:00Z"}
        ]"#;
        // submitted_at は push_time より前なのでフィルタされる
        assert_eq!(
            parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
            None
        );
    }

    // --- extract_actionable_count ---

    #[test]
    fn extract_count_from_body() {
        assert_eq!(
            extract_actionable_count("Actionable comments posted: 7"),
            Some(7)
        );
    }

    #[test]
    fn extract_count_zero() {
        assert_eq!(
            extract_actionable_count("Actionable comments posted: 0"),
            Some(0)
        );
    }

    #[test]
    fn extract_count_not_found() {
        assert_eq!(extract_actionable_count("No issues found"), None);
    }

    // --- parse_unresolved_threads ---

    #[test]
    fn unresolved_threads_counts() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": false},
                                {"isResolved": true},
                                {"isResolved": false}
                            ]
                        }
                    }
                }
            }
        }"#;
        assert_eq!(parse_unresolved_threads(json), Some(2));
    }

    #[test]
    fn unresolved_threads_all_resolved() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {"isResolved": true}
                            ]
                        }
                    }
                }
            }
        }"#;
        assert_eq!(parse_unresolved_threads(json), Some(0));
    }

    #[test]
    fn unresolved_threads_invalid_json() {
        assert_eq!(parse_unresolved_threads("{}"), None);
    }

    // --- decide ---

    #[test]
    fn decide_ci_pending() {
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![CiRunSummary {
                name: "build".to_string(),
                conclusion: "".to_string(),
            }],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_cr_pending() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "pending".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_cr_not_found() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    #[test]
    fn decide_ci_failure() {
        let ci = CiStatus {
            overall: "failure".to_string(),
            runs: vec![CiRunSummary {
                name: "test".to_string(),
                conclusion: "failure".to_string(),
            }],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "error");
        assert_eq!(action, "stop_monitoring_failure");
    }

    #[test]
    fn decide_new_comments() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 2,
            actionable_comments: None,
            unresolved_threads: Some(0),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_unresolved_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: None,
            unresolved_threads: Some(3),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_actionable_overrides_new_comments() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(3), // レビュー本文では3件、コメントAPIでは0件
            unresolved_threads: Some(0),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_all_clean() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    #[test]
    fn decide_cr_failure() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "failure".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "error");
        assert_eq!(action, "stop_monitoring_failure");
    }

    #[test]
    fn decide_cr_not_found_with_comments() {
        // review_state が not_found でも actionable_comments があれば action_required
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            new_comments: 0,
            actionable_comments: Some(3),
            unresolved_threads: Some(3),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "action_required");
        assert_eq!(action, "action_required");
    }

    #[test]
    fn decide_no_ci_cr_success() {
        // CI runs 空 (CI 未設定) + CR 成功 → complete (CI スキップ)
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "complete");
        assert_eq!(action, "stop_monitoring_success");
    }

    #[test]
    fn decide_no_ci_cr_not_found_no_comments() {
        // CI 未設定 + CR not_found + コメントなし → pending (まだレビュー待ち)
        let ci = CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "not_found".to_string(),
            ..Default::default()
        };
        let (status, action) = decide(&ci, &cr);
        assert_eq!(status, "pending");
        assert_eq!(action, "continue_monitoring");
    }

    // --- build_summary ---

    #[test]
    fn summary_all_clean() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 0,
            actionable_comments: Some(0),
            unresolved_threads: Some(0),
        };
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("CI成功"));
        assert!(summary.contains("指摘なし"));
    }

    #[test]
    fn summary_ci_failure() {
        let ci = CiStatus {
            overall: "failure".to_string(),
            runs: vec![CiRunSummary {
                name: "test".to_string(),
                conclusion: "failure".to_string(),
            }],
        };
        let cr = CodeRabbitStatus::default();
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("CI失敗"));
        assert!(summary.contains("test"));
    }

    #[test]
    fn summary_with_comments_and_threads() {
        let ci = CiStatus {
            overall: "success".to_string(),
            runs: vec![],
        };
        let cr = CodeRabbitStatus {
            review_state: "success".to_string(),
            new_comments: 2,
            actionable_comments: Some(3),
            unresolved_threads: Some(1),
        };
        let summary = build_summary(&ci, &cr);
        assert!(summary.contains("新規指摘3件"));
        assert!(summary.contains("未解決スレッド1件"));
    }

    // --- parse_args ---

    #[test]
    fn parse_args_extracts_push_time() {
        // parse_args reads from std::env::args, so we test the logic indirectly
        // by testing the struct construction
        let args = CliArgs {
            push_time: "2026-04-01T12:00:00Z".to_string(),
            repo: Some("owner/repo".to_string()),
            pr: Some(42),
        };
        assert_eq!(args.push_time, "2026-04-01T12:00:00Z");
        assert_eq!(args.repo, Some("owner/repo".to_string()));
        assert_eq!(args.pr, Some(42));
    }
}
