//! CI・CodeRabbit 状態チェッカー (スタンドアロン exe)
//!
//! push / PR 作成後に CI (GitHub Actions) と CodeRabbit レビューの状態を
//! gh CLI 経由で取得し、構造化 JSON を stdout に出力する。
//! CronCreate のポーリングジョブから定期的に呼び出される想定。
//!
//! 使い方:
//!   check-ci-coderabbit.exe --push-time "2026-04-01T12:00:00Z" [--repo owner/repo] [--pr 42]
//!   check-ci-coderabbit.exe --list-findings --pr 42 [--repo owner/repo]
//!
//! `--list-findings` モード: ADR-034 Bundle a Sub-PR 1 で追加。
//! CodeRabbit のインラインレビューコメントを構造化 JSON `{"findings": [...]}`
//! として stdout に出力する。CI / rate-limit / status check は実行しない。
//! cli-pr-monitor および Claude (応答時の listing) からの呼び出しを想定。
//!
//! 終了コード:
//!   0 - チェック完了 (結果は stdout JSON の action フィールドを参照)
//!   1 - 引数エラーまたは致命的エラー

mod decide;
mod findings;
mod markers;
mod models;
mod parsers;
mod rate_limit;

use std::process::Command;
use std::time::Duration;

use crate::decide::{build_summary, decide};
use crate::findings::{parse_findings, parse_listed_findings};
use crate::models::{
    CheckResult, CiRunSummary, CiStatus, CodeRabbitStatus, ListFindingsOutput,
};
use crate::parsers::{
    parse_actionable_comments, parse_ci_runs, parse_coderabbit_status, parse_new_comments,
    parse_unresolved_threads, parse_walkthrough_clean_marker,
};
use crate::rate_limit::parse_rate_limit;

const EPOCH_PUSH_TIME: &str = "1970-01-01T00:00:00Z";

struct CliArgs {
    push_time: String,
    repo: Option<String>,
    pr: Option<u64>,
    list_findings: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut push_time = None;
    let mut repo = None;
    let mut pr = None;
    let mut list_findings = false;

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
            "--list-findings" => {
                list_findings = true;
            }
            _ => {}
        }
        i += 1;
    }

    let push_time = if list_findings {
        push_time.unwrap_or_else(|| EPOCH_PUSH_TIME.to_string())
    } else {
        push_time.ok_or("--push-time は必須です")?
    };

    Ok(CliArgs {
        push_time,
        repo,
        pr,
        list_findings,
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
    let (repo, pr) = match resolve_repo_and_pr(&args) {
        Ok(v) => v,
        Err(error_result) => return *error_result,
    };

    let ci = fetch_ci(&get_current_branch().unwrap_or_default());
    let cr_state = fetch_coderabbit_commit_state(&repo);

    let comments_json = fetch_issue_comments_json(&repo, pr);
    let new_comments = parse_new_comments(&comments_json, &args.push_time);
    let walkthrough_clean = parse_walkthrough_clean_marker(&comments_json, &args.push_time);
    let rate_limit = parse_rate_limit(&comments_json, &args.push_time);

    let actionable = fetch_actionable_count(&repo, pr, &args.push_time);
    let unresolved = fetch_unresolved_threads(&repo, pr);

    let cr = CodeRabbitStatus {
        review_state: cr_state,
        new_comments,
        actionable_comments: actionable,
        unresolved_threads: unresolved,
        walkthrough_clean,
    };

    let findings = fetch_findings(&repo, pr, &args.push_time);
    let (status, action) = decide(&ci, &cr);
    let summary = build_summary(&ci, &cr);

    CheckResult {
        status,
        action,
        ci,
        coderabbit: cr,
        summary,
        findings,
        rate_limit,
    }
}

fn resolve_repo_and_pr(args: &CliArgs) -> Result<(String, u64), Box<CheckResult>> {
    let repo_result = args.repo.clone().map(Ok).unwrap_or_else(auto_detect_repo);
    let pr_result = args.pr.map(Ok).unwrap_or_else(auto_detect_pr);

    let repo_err = repo_result.as_ref().err().cloned();
    let pr_err = pr_result.as_ref().err().cloned();
    let repo = repo_result.unwrap_or_default();
    let pr = pr_result.unwrap_or(0);

    if repo.is_empty() || pr == 0 || !is_valid_repo(&repo) {
        return Err(Box::new(build_init_error_result(&repo, pr, repo_err, pr_err)));
    }
    Ok((repo, pr))
}

fn build_init_error_result(
    repo: &str,
    pr: u64,
    repo_err: Option<String>,
    pr_err: Option<String>,
) -> CheckResult {
    let mut reasons = vec![];
    if repo.is_empty() {
        let detail = repo_err.unwrap_or_else(|| "不明".to_string());
        reasons.push(format!("リポジトリ取得失敗: {}", detail));
    } else if !is_valid_repo(repo) {
        reasons.push(format!("リポジトリ名が不正: {}", repo));
    }
    if pr == 0 {
        let detail = pr_err.unwrap_or_else(|| "不明".to_string());
        reasons.push(format!("PR番号取得失敗: {}", detail));
    }
    let summary = format!("初期化エラー: {}", reasons.join("; "));
    eprintln!("[check-ci-coderabbit] {}", summary);
    CheckResult {
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
        rate_limit: None,
    }
}

fn fetch_ci(branch: &str) -> CiStatus {
    if branch.is_empty() {
        return CiStatus {
            overall: "pending".to_string(),
            runs: vec![],
        };
    }
    match run_gh(&[
        "run",
        "list",
        "--branch",
        branch,
        "--limit",
        "5",
        "--json",
        "name,conclusion",
    ]) {
        Ok(ci_json) => parse_ci_runs(&ci_json),
        Err(e) => {
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
}

fn fetch_coderabbit_commit_state(repo: &str) -> String {
    let head_sha = get_head_sha().unwrap_or_default();
    if head_sha.is_empty() || !is_valid_sha(&head_sha) {
        return "not_found".to_string();
    }
    let statuses_json = run_gh(&[
        "api",
        &format!("repos/{}/commits/{}/statuses", repo, head_sha),
    ])
    .unwrap_or_else(|_| "[]".to_string());
    parse_coderabbit_status(&statuses_json)
}

fn fetch_issue_comments_json(repo: &str, pr: u64) -> String {
    run_gh(&[
        "api",
        &format!("repos/{}/issues/{}/comments", repo, pr),
    ])
    .unwrap_or_else(|_| "[]".to_string())
}

fn fetch_actionable_count(repo: &str, pr: u64, push_time: &str) -> Option<usize> {
    let reviews_json = run_gh(&[
        "api",
        &format!("repos/{}/pulls/{}/reviews", repo, pr),
    ])
    .unwrap_or_else(|_| "[]".to_string());
    parse_actionable_comments(&reviews_json, push_time)
}

fn fetch_unresolved_threads(repo: &str, pr: u64) -> Option<usize> {
    let (owner, name) = repo.split_once('/').unwrap_or(("", ""));
    if owner.is_empty() || name.is_empty() {
        return None;
    }
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
}

fn fetch_findings(repo: &str, pr: u64, push_time: &str) -> Vec<lib_report_formatter::Finding> {
    let pull_comments_json = run_gh(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/comments", repo, pr),
    ])
    .unwrap_or_else(|_| "[]".to_string());
    parse_findings(&pull_comments_json, push_time)
}

/// `--list-findings` モードの実行: PR インラインコメントを取得して
/// `ListedFinding` 配列にして返す。CI / rate-limit / status check は実行しない。
fn run_list_findings(args: CliArgs) -> Result<ListFindingsOutput, String> {
    let repo = args
        .repo
        .map(Ok)
        .unwrap_or_else(auto_detect_repo)
        .map_err(|e| format!("リポジトリ取得失敗: {}", e))?;
    if !is_valid_repo(&repo) {
        return Err(format!("リポジトリ名が不正: {}", repo));
    }
    let pr = args
        .pr
        .map(Ok)
        .unwrap_or_else(auto_detect_pr)
        .map_err(|e| format!("PR番号取得失敗: {}", e))?;

    let pull_comments_json = run_gh(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/comments", repo, pr),
    ])
    .map_err(|e| format!("pull comments 取得失敗: {}", e))?;

    Ok(ListFindingsOutput {
        findings: parse_listed_findings(&pull_comments_json, &args.push_time),
    })
}

fn print_json<T: serde::Serialize>(value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    println!("{}", json);
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[check-ci-coderabbit] エラー: {}", e);
            eprintln!("使い方: check-ci-coderabbit.exe --push-time <ISO8601> [--repo owner/repo] [--pr N]");
            eprintln!("       check-ci-coderabbit.exe --list-findings --pr N [--repo owner/repo]");
            std::process::exit(1);
        }
    };

    if args.list_findings {
        match run_list_findings(args) {
            Ok(output) => {
                print_json(&output);
            }
            Err(e) => {
                eprintln!("[check-ci-coderabbit] --list-findings エラー: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    let result = run_check(args);
    print_json(&result);
}

// ─── テスト ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_extracts_push_time() {
        // NOTE: parse_args reads std::env::args, so we test struct construction here.
        let args = CliArgs {
            push_time: "2026-04-01T12:00:00Z".to_string(),
            repo: Some("owner/repo".to_string()),
            pr: Some(42),
            list_findings: false,
        };
        assert_eq!(args.push_time, "2026-04-01T12:00:00Z");
        assert_eq!(args.repo, Some("owner/repo".to_string()));
        assert_eq!(args.pr, Some(42));
        assert!(!args.list_findings);
    }


}
