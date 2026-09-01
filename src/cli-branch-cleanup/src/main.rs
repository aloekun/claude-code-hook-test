//! cli-branch-cleanup — 決着済み PR のブランチを lease 付きで削除する (順位 467 D-1 / 機3)。
//!
//! `.github/workflows/nightly-todo.yml` の掃除ループに shell で書かれていた
//! 観測 → 削除 → 分類を移送した exe。**判定は [`classify`] の純関数**にあり、
//! 本ファイルは I/O (git 呼び出し) と表示だけを持つ。
//!
//! # 責務分離
//!
//! 削除**対象の列挙**は `cli-stale-branch-scan` が担う (提案)。本 exe は**実行**side で、
//! 標準入力から受け取ったブランチ名だけを処理する ([ADR-022](../../../docs/adr/adr-022-automation-responsibility-separation.md)
//! の責務分離: scan = 提案、cleanup = 外部可視の実行)。
//!
//! # 使い方
//!
//! ```text
//! cli-stale-branch-scan ... --deletable-only \
//!   | cli-branch-cleanup --repo <owner/repo> --work-dir <dir> [--dry-run]
//! ```
//!
//! App token は環境変数 `APP_TOKEN` から取る。**token を出力へ出さない** — git の
//! 出力は必ず [`redact`] を通す。
//!
//! # `--work-dir` が要る理由
//!
//! **`git push` はリポジトリの外では動かない** (`fatal: not a git repository`、exit 128)。
//! 夜間 workflow の既定 cwd は checkout 先ではないため、移送前の shell は使い捨ての空
//! リポジトリを `git init` して `git -C` で push していた。本 exe も同じことをする —
//! `--work-dir` に空リポジトリを作り、そこから push する。
//!
//! **既存の checkout を使わない。** `actions/checkout` が仕込む extraheader の資格情報が、
//! URL に埋めた App token より優先されうる。空リポジトリなら継承する config が無い。
//!
//! # 終了コード
//!
//! - 0 — 全ブランチが削除 / skip で片付いた
//! - 1 — ref が動いた (中止) / 観測・再確認の失敗が 1 件でもあった
//! - 2 — 引数エラー / `APP_TOKEN` 不在

mod classify;

use std::io::Read;
use std::process::{Command, Stdio};

use classify::{classify, observe_from_output, DeleteAttempt, Outcome, RefObservation};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const GIT_TIMEOUT_SECS: u64 = 120;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("[branch-cleanup] 引数エラー: {message}");
            eprintln!(
                "usage: cli-branch-cleanup --repo <owner/repo> --work-dir <dir> [--dry-run]"
            );
            return std::process::ExitCode::from(EXIT_USAGE as u8);
        }
    };
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("[branch-cleanup] 標準入力を読めません");
        return std::process::ExitCode::from(EXIT_USAGE as u8);
    }
    let branches: Vec<&str> = input
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if branches.is_empty() {
        eprintln!("[branch-cleanup] 掃除対象はありません");
        return std::process::ExitCode::SUCCESS;
    }
    if cli.dry_run {
        for branch in &branches {
            eprintln!("[branch-cleanup] dry-run のため削除しません: {branch}");
        }
        return std::process::ExitCode::SUCCESS;
    }

    let Ok(token) = std::env::var("APP_TOKEN") else {
        eprintln!("[branch-cleanup] APP_TOKEN が設定されていません");
        return std::process::ExitCode::from(EXIT_USAGE as u8);
    };
    if token.trim().is_empty() {
        eprintln!("[branch-cleanup] APP_TOKEN が空です");
        return std::process::ExitCode::from(EXIT_USAGE as u8);
    }

    delete_all(&cli, &token, &branches)
}

/// 先頭から順にブランチを処理し、**異常を検知した時点で以降のブランチには一切触れず
/// red で終える** (旧 shell の `exit 1` と同じ fail-fast、意味論は移送で変えない)。
fn delete_all(cli: &Cli, token: &str, branches: &[&str]) -> std::process::ExitCode {
    let push_url = format!("https://x-access-token:{token}@github.com/{}.git", cli.repo);
    if let Err(detail) = init_work_repo(token, &cli.work_dir) {
        eprintln!("[branch-cleanup] push 用の空リポジトリを作れません: {detail}");
        return std::process::ExitCode::from(EXIT_FAILURE as u8);
    }
    if run_branches(branches, |branch| {
        process_branch(&push_url, token, &cli.work_dir, branch)
    }) {
        std::process::ExitCode::from(EXIT_FAILURE as u8)
    } else {
        std::process::ExitCode::SUCCESS
    }
}

/// 各ブランチを処理し、**最初の失敗で打ち切る**。失敗したら `true`。
///
/// 打ち切りは移送前の shell (`set -euo pipefail` + `exit 1`) と同じ意味論である。
/// 続行すると、失効した token やネットワーク断のような**残り全件でも同じく失敗する
/// 原因**に対して削除 push を投げ続けることになる。
///
/// 処理の実体を引数で受けるのは、この打ち切り自体を I/O 無しでテストするため
/// (F5 と同じ注入の seam)。
fn run_branches(branches: &[&str], mut process: impl FnMut(&str) -> Outcome) -> bool {
    for branch in branches {
        let outcome = process(branch);
        eprintln!("[branch-cleanup] {}", outcome.message(branch));
        if outcome.is_failure() {
            return true;
        }
    }
    false
}

struct Cli {
    repo: String,
    work_dir: String,
    dry_run: bool,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut repo = None;
    let mut work_dir = None;
    let mut dry_run = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" => {
                i += 1;
                repo = Some(args.get(i).ok_or("--repo には引数が必要です")?.clone());
            }
            "--work-dir" => {
                i += 1;
                work_dir = Some(args.get(i).ok_or("--work-dir には引数が必要です")?.clone());
            }
            "--dry-run" => dry_run = true,
            other => return Err(format!("不明な引数: {other}")),
        }
        i += 1;
    }
    let repo = repo.ok_or("--repo は必須です")?;
    if repo.trim().is_empty() {
        return Err("--repo が空です".to_string());
    }
    let work_dir = work_dir.ok_or("--work-dir は必須です")?;
    if work_dir.trim().is_empty() {
        return Err("--work-dir が空です".to_string());
    }
    Ok(Cli {
        repo,
        work_dir,
        dry_run,
    })
}

/// push 元になる**空リポジトリ**を用意する。
///
/// `git init` は既存ディレクトリに対しても安全 (再初期化) なので、run をまたいで
/// 同じ `--work-dir` を渡してよい。
fn init_work_repo(token: &str, work_dir: &str) -> Result<(), String> {
    if let Err(e) = std::fs::create_dir_all(work_dir) {
        return Err(redact(&format!("ディレクトリを作れません: {e}"), token));
    }
    run_git(token, &["init", "-q", work_dir]).map(|_| ())
}

/// 1 ブランチ分の観測 → 削除 → (失敗時のみ) 再確認 を行い、分類を返す。
///
/// **段の呼び分けは [`classify`] の契約に合わせる** — 呼ばなかった段は `None` を渡す。
fn process_branch(push_url: &str, token: &str, work_dir: &str, branch: &str) -> Outcome {
    let observation = observe_ref(push_url, token, branch);
    let RefObservation::Present(_) = observation else {
        return classify(&observation, None, None);
    };
    let delete = delete_ref(push_url, token, work_dir, branch, &observation);
    let DeleteAttempt::Failed(_) = delete else {
        return classify(&observation, Some(&delete), None);
    };
    let recheck = observe_ref(push_url, token, branch);
    classify(&observation, Some(&delete), Some(&recheck))
}

fn observe_ref(push_url: &str, token: &str, branch: &str) -> RefObservation {
    let output = run_git(
        token,
        &[
            "ls-remote",
            "--heads",
            push_url,
            &format!("refs/heads/{branch}"),
        ],
    );
    match output {
        Ok(stdout) => observe_from_output(stdout.lines().next().unwrap_or("")),
        Err(detail) => RefObservation::Failed(detail),
    }
}

/// lease 付きの削除 push。**空リポジトリ (`work_dir`) から実行する** — job の既定 cwd は
/// リポジトリではなく、そこから push すると `fatal: not a git repository` で死ぬ。
fn delete_ref(
    push_url: &str,
    token: &str,
    work_dir: &str,
    branch: &str,
    observed: &RefObservation,
) -> DeleteAttempt {
    let RefObservation::Present(sha) = observed else {
        return DeleteAttempt::Failed("観測できていない ref を削除しようとしました".to_string());
    };
    let lease = format!("--force-with-lease=refs/heads/{branch}:{sha}");
    match run_git(
        token,
        &[
            "-C",
            work_dir,
            "push",
            &lease,
            push_url,
            "--delete",
            &format!("refs/heads/{branch}"),
        ],
    ) {
        Ok(_) => DeleteAttempt::Succeeded,
        Err(detail) => DeleteAttempt::Failed(detail),
    }
}

/// git を起動し、成功なら stdout、失敗なら **redact 済み**の診断を返す。
///
/// stdout と stderr は分離して読む — 成功時の戻り値に stderr の警告が混ざると、
/// `ls-remote` の出力解釈がずれる (`cli-pr-monitor` の `interpret_capture` と同じ理由)。
fn run_git(token: &str, args: &[&str]) -> Result<String, String> {
    let mut child = match Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return Err(redact(&format!("git の起動に失敗: {e}"), token)),
    };
    let stdout_handle = lib_subprocess::drain_pipe_unlimited(
        child.stdout.take().expect("stdout must be piped"),
    );
    let stderr_handle = lib_subprocess::drain_pipe_unlimited(
        child.stderr.take().expect("stderr must be piped"),
    );
    let status = lib_subprocess::wait_with_timeout_basic("git", &mut child, GIT_TIMEOUT_SECS);
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    match status {
        Ok(Some(status)) if status.success() => Ok(stdout),
        Ok(Some(_)) => Err(redact(&format!("{stdout}{stderr}"), token)),
        Ok(None) => Err(redact(
            &format!("git がタイムアウトしました ({GIT_TIMEOUT_SECS}s): {stderr}"),
            token,
        )),
        Err(e) => Err(redact(&format!("git の実行に失敗: {e}"), token)),
    }
}

/// 出力から token を伏せる。**token は URL に埋めて渡すため、生のまま出すと漏れる**。
fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        // NOTE: 空文字の replace は全文字間へ挿入され、診断が読めなくなる。
        return text.trim().to_string();
    }
    text.replace(token, "***").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<String> {
        let mut all = vec![
            "cli-branch-cleanup".to_string(),
            "--repo".to_string(),
            "owner/repo".to_string(),
            "--work-dir".to_string(),
            "/tmp/cleanup-repo".to_string(),
        ];
        all.extend(extra.iter().map(|s| s.to_string()));
        all
    }

    #[test]
    fn repo_is_required() {
        let args = vec!["cli-branch-cleanup".to_string()];
        assert!(parse_args(&args).is_err());
    }

    /// **`--work-dir` は省略できない。** push はリポジトリの外では動かないため、
    /// 省略を許すと job の既定 cwd で `fatal: not a git repository` に落ちる。
    #[test]
    fn work_dir_is_required() {
        let args = vec![
            "cli-branch-cleanup".to_string(),
            "--repo".to_string(),
            "owner/repo".to_string(),
        ];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn dry_run_is_optional() {
        let cli = parse_args(&args(&[])).expect("parse");
        assert_eq!(cli.repo, "owner/repo");
        assert_eq!(cli.work_dir, "/tmp/cleanup-repo");
        assert!(!cli.dry_run);
    }

    #[test]
    fn dry_run_flag_is_parsed() {
        assert!(parse_args(&args(&["--dry-run"])).expect("parse").dry_run);
    }

    #[test]
    fn unknown_flags_are_usage_errors() {
        let args = vec![
            "cli-branch-cleanup".to_string(),
            "--force".to_string(),
        ];
        assert!(parse_args(&args).is_err());
    }

    /// **最初の失敗で打ち切る** (移送前の shell の `set -e` + `exit 1` と同じ)。
    /// 続けると、失効した token のような全件共通の原因に対して削除 push を投げ続ける。
    #[test]
    fn the_loop_stops_at_the_first_failure() {
        let mut seen = Vec::new();
        let failed = run_branches(&["a", "b", "c"], |branch| {
            seen.push(branch.to_string());
            if branch == "b" {
                Outcome::Failed("could not read Username".to_string())
            } else {
                Outcome::Deleted
            }
        });
        assert!(failed);
        assert_eq!(seen, vec!["a".to_string(), "b".to_string()], "c は触らない");
    }

    /// skip は失敗ではないので打ち切らない (既に消えた ref で掃除を止めない)。
    #[test]
    fn skipped_branches_do_not_stop_the_loop() {
        let mut seen = Vec::new();
        let failed = run_branches(&["a", "b"], |branch| {
            seen.push(branch.to_string());
            Outcome::SkippedAlreadyGone
        });
        assert!(!failed);
        assert_eq!(seen.len(), 2);
    }

    /// **token を出力へ出さない。** URL に埋め込んで git へ渡すため、診断をそのまま
    /// 出すと GitHub Actions のログへ漏れる。
    #[test]
    fn the_token_is_redacted_from_diagnostics() {
        let token = "ghs_secret_value";
        let raw = format!("remote: https://x-access-token:{token}@github.com/o/r.git failed");
        let masked = redact(&raw, token);
        assert!(!masked.contains(token), "{masked}");
        assert!(masked.contains("***"), "{masked}");
    }

    /// 空の token で redact しても全文が伏せられない (置換対象が空文字列にならないこと)。
    #[test]
    fn redacting_with_an_empty_token_keeps_the_text() {
        let masked = redact("plain diagnostic", "");
        assert!(masked.contains("plain diagnostic"), "{masked}");
    }
}
