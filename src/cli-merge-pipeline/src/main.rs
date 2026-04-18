//! Merge Pipeline ランナー (スタンドアロン exe)
//!
//! pnpm merge-pr から呼び出され、PR のマージとローカル同期を実行します。
//! hooks-config.toml の [merge_pipeline] セクションから設定を読み込みます。
//!
//! 処理フロー:
//!   1. jj bookmark から現在の PR を自動検出
//!   2. [merge_pipeline.pre_steps] を順次実行（マージ前チェック）
//!   3. gh pr merge --squash を実行
//!   4. jj git fetch && jj new master でローカル同期
//!   5. [merge_pipeline.post_steps] を順次実行（学び提案等の拡張ポイント）
//!
//! 終了コード:
//!   0 - マージ成功 & ローカル同期完了
//!   1 - マージ失敗 / PR 検出失敗
//!   2 - 設定エラー

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ─── 設定 ───

/// hooks-config.toml のトップレベル構造
#[derive(Deserialize, Default)]
struct Config {
    merge_pipeline: Option<MergePipelineConfig>,
}

/// `[merge_pipeline]` セクションの設定
#[derive(Deserialize, Default)]
struct MergePipelineConfig {
    step_timeout: Option<u64>,
    default_branch: Option<String>,
    pre_steps: Option<Vec<PipelineStepConfig>>,
    post_steps: Option<Vec<PipelineStepConfig>>,
}

/// パイプラインの個別ステップ定義
#[derive(Deserialize, Clone)]
struct PipelineStepConfig {
    name: String,
    #[serde(rename = "type")]
    step_type: String,
    cmd: Option<String>,
    prompt: Option<String>,
}

/// デフォルトのブランチ名
const DEFAULT_BRANCH: &str = "master";

/// デフォルトのステップタイムアウト（秒）
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;

/// マージコマンドのタイムアウト（秒）
const DEFAULT_MERGE_TIMEOUT_SECS: u64 = 300;

// ─── ログ出力ヘルパー ───

fn log_step(name: &str, status: &str, message: &str) {
    if message.is_empty() {
        eprintln!("[merge-pipeline] [{}] {}", name, status);
    } else {
        eprintln!("[merge-pipeline] [{}] {} — {}", name, status, message);
    }
}

fn log_info(message: &str) {
    eprintln!("[merge-pipeline] {}", message);
}

// ─── パイプ排出 ───

/// サブプロセス出力の最大収集行数（メモリ保護）
const MAX_LINES: usize = 200;

fn drain_pipe(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut collected = Vec::with_capacity(MAX_LINES);
        let mut buf = Vec::new();
        let mut truncated = 0usize;

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < MAX_LINES {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    } else {
                        truncated += 1;
                    }
                }
                Err(_) => break,
            }
        }
        if truncated > 0 {
            collected.push(format!("... ({} lines truncated)", truncated));
        }
        collected.join("\n")
    })
}

// ─── コマンド実行 ───

fn run_cmd(name: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    let mut child = match Command::new("cmd")
        .args(["/c", cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain_pipe(child.stdout.take().unwrap());
    let stderr_handle = drain_pipe(child.stderr.take().unwrap());

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return (false, format!("Failed to wait for {}: {}", name, e)),
        }
    };

    if timed_out {
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();
        let combined = combine_output(&stdout, &stderr);
        let mut msg = format!("timed out after {}s", timeout_secs);
        if !combined.is_empty() {
            msg = format!("{}\n{}", msg, combined);
        }
        return (false, msg);
    }

    let success = child.wait().map(|s| s.success()).unwrap_or(false);

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let combined = combine_output(&stdout, &stderr);

    (success, combined)
}

fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

// ─── 設定ファイル読み込み ───

fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("hooks-config.toml の読み込みに失敗: {} ({})", path.display(), e))?;
    toml::from_str(&content)
        .map_err(|e| format!("hooks-config.toml のパースに失敗: {}", e))
}

// ─── PR 検出 (cli-pr-monitor から移植) ───

/// gh コマンドを実行し、失敗時は stderr をログ出力する
fn run_gh_logged(args: &[&str]) -> Option<String> {
    let output = match Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log_info(&format!("gh コマンド実行失敗: {} (args: {:?})", e, args));
            return None;
        }
    };

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            log_info(&format!("gh {:?} 失敗: {}", args, stderr));
        }
        None
    }
}

/// Bookmark 検索に使用する revset のリスト (近い順 = 優先順)。
///
/// `select_from_revsets` は先頭から順に試し、最初に (trunk 除外後の) bookmark が
/// 見つかった時点で後続の revset を検索しない ("@" で見つかれば "@--" は触らない)。
///
/// - `@`: 標準 `git` ブランチ運用、または bookmark が現在のコミット上にある場合
/// - `@-`: `jj new` で空 `@` を作った直後 (PR #53 で実測)
/// - `@--`: 連続 `jj new` や中間空コミット運用向けのフォールバック
const BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "@-", "@--"];

/// PR 検出から除外する trunk 系 bookmark。
/// cli-push-runner/push_jj_bookmark.rs と同じリストを採用。
const TRUNK_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop"];

fn is_trunk_bookmark(name: &str) -> bool {
    TRUNK_BOOKMARKS.contains(&name)
}

/// 現在の jj change 周辺に紐づく全ブックマーク名を取得する。
///
/// `BOOKMARK_SEARCH_REVSETS` の順で検索し、最初に非空の結果が得られた revset の
/// bookmark を返す。すべての revset で空なら空 Vec。
fn get_jj_bookmarks() -> Vec<String> {
    select_from_revsets(BOOKMARK_SEARCH_REVSETS, query_bookmarks_at)
}

/// 指定 revset を優先順に試し、最初に非空の bookmark リストを得た revset の結果を返す。
/// テスト用に `query` をクロージャで注入できる。
fn select_from_revsets<F>(revsets: &[&str], query: F) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    for (i, revset) in revsets.iter().enumerate() {
        let bookmarks = query(revset);
        if !bookmarks.is_empty() {
            if i > 0 {
                log_info(&format!(
                    "revset '{}' で bookmark を検出: {:?}",
                    revset, bookmarks
                ));
            }
            return bookmarks;
        }
    }
    Vec::new()
}

/// 指定 revset の bookmark 名を `jj log` で取得する (I/O)。
fn query_bookmarks_at(revset: &str) -> Vec<String> {
    let output = match Command::new("jj")
        .args([
            "log",
            "-r",
            revset,
            "--no-graph",
            "-T",
            "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if !stderr.is_empty() {
                log_info(&format!(
                    "jj bookmark 取得失敗 (revset={}): {}",
                    revset, stderr
                ));
            }
            return Vec::new();
        }
        Err(e) => {
            log_info(&format!("jj コマンド実行失敗: {}", e));
            return Vec::new();
        }
    };

    parse_bookmark_list_output(&String::from_utf8_lossy(&output.stdout))
}

/// `jj log` テンプレート出力 (カンマ区切り × 行) からユニークな bookmark 名を抽出する。
/// trunk 系 bookmark (master/main/trunk/develop) は PR 検索対象から除外する。
fn parse_bookmark_list_output(raw: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for line in raw.lines() {
        for name in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if is_trunk_bookmark(name) {
                continue;
            }
            let name = name.to_string();
            if !seen.contains(&name) {
                seen.push(name);
            }
        }
    }
    seen
}

/// 現在のブックマークから PR 番号を検出する
fn detect_pr_number() -> Option<u64> {
    // Strategy A: gh pr view (git ブランチが使える場合)
    let pr_number = run_gh_logged(&["pr", "view", "--json", "number", "-q", ".number"])
        .and_then(|s| s.parse::<u64>().ok());

    if pr_number.is_some() {
        return pr_number;
    }

    // Strategy B: jj bookmark → gh pr list --head (OPEN 優先、なければ全状態)
    let bookmarks = get_jj_bookmarks();
    for bookmark in &bookmarks {
        log_info(&format!("jj bookmark '{}' を使用して PR を検索", bookmark));

        // まず OPEN な PR を検索
        let pr_number = run_gh_logged(&[
            "pr", "list", "--head", bookmark, "--json", "number", "-q", ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return pr_number;
        }

        // OPEN がなければ全状態で検索（マージ済み PR のローカル同期用）
        let pr_number = run_gh_logged(&[
            "pr", "list", "--head", bookmark, "--state", "all",
            "--json", "number", "-q", ".[0].number",
        ])
        .and_then(|s| s.parse::<u64>().ok());

        if pr_number.is_some() {
            return pr_number;
        }
    }

    None
}

// ─── ステップ実行 ───

/// ステップリストを順次実行する。失敗時は Err(exit_code) を返す
fn run_steps(phase: &str, steps: &[PipelineStepConfig], timeout: u64) -> Result<(), i32> {
    if steps.is_empty() {
        return Ok(());
    }

    log_info(&format!("{} ({} ステップ)", phase, steps.len()));

    for (i, step) in steps.iter().enumerate() {
        let label = format!("{}/{} {}", i + 1, steps.len(), step.name);

        match step.step_type.as_str() {
            "command" => {
                let trimmed_cmd = step.cmd.as_deref().map(str::trim).filter(|c| !c.is_empty());
                let cmd = match trimmed_cmd {
                    Some(c) => c,
                    None => {
                        log_step(&label, "ERROR", "cmd が未定義または空です");
                        return Err(1);
                    }
                };

                log_step(&label, "RUN", cmd);
                let (success, output) = run_cmd(&step.name, cmd, timeout);

                if success {
                    log_step(&label, "PASS", "");
                } else {
                    log_step(&label, "FAIL", "");
                    if !output.is_empty() {
                        eprintln!("{}", output);
                    }
                    log_info(&format!(
                        "パイプライン中断: {} が失敗しました。問題を修正して pnpm merge-pr を再実行してください。",
                        step.name
                    ));
                    return Err(1);
                }
            }
            "ai" => {
                let prompt = step.prompt.as_deref().unwrap_or("(未定義)");
                log_step(
                    &label,
                    "SKIP",
                    &format!(
                        "AI ステップ (prompt: {}) — 将来実装予定。現在はスキップします。",
                        prompt
                    ),
                );
            }
            unknown => {
                log_step(
                    &label,
                    "ERROR",
                    &format!("未知のステップタイプ: {}", unknown),
                );
                return Err(1);
            }
        }
    }
    Ok(())
}

// ─── パイプライン実行 ───

fn run_pipeline() -> i32 {
    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("設定エラー: {}", e));
            return 2;
        }
    };

    let pipeline = config.merge_pipeline.unwrap_or_default();

    let pre_steps = pipeline.pre_steps.unwrap_or_default();
    let post_steps = pipeline.post_steps.unwrap_or_default();
    let timeout = pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
    let branch = pipeline
        .default_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BRANCH)
        .to_string();

    // PR 検出
    log_info("PR を検出中...");
    let pr_number = match detect_pr_number() {
        Some(n) => {
            log_info(&format!("PR #{} を検出しました", n));
            n
        }
        None => {
            log_info("エラー: 現在のブックマークに紐づく PR が見つかりません。");
            log_info("ヒント: PR が作成済みで、正しいブックマークにいることを確認してください。");
            return 1;
        }
    };

    // PR の状態を確認（マージ可能か）
    log_info("PR の状態を確認中...");
    let pr_state = run_gh_logged(&["pr", "view", &pr_number.to_string(), "--json", "state", "-q", ".state"]);
    match pr_state.as_deref() {
        Some("MERGED") => {
            log_info("この PR は既にマージ済みです。ローカル同期のみ実行します。");
            let rc = sync_local(&branch);
            if rc != 0 { return rc; }
            // マージ済みでも post_steps は実行する（学び提案等）
            if let Err(code) = run_steps("post-merge ステップ", &post_steps, timeout) {
                return code;
            }
            return 0;
        }
        Some("CLOSED") => {
            log_info("エラー: この PR はクローズされています。");
            return 1;
        }
        Some("OPEN") => {} // 正常 — 続行
        _ => {
            log_info("警告: PR の状態を取得できませんでした。マージを試行します。");
        }
    }

    // pre-merge ステップ実行
    if let Err(code) = run_steps("pre-merge ステップ", &pre_steps, timeout) {
        return code;
    }

    // マージ実行 (gh api を使用 — jj の detached HEAD でも動作する)
    let merge_cmd = format!(
        "gh api repos/{{owner}}/{{repo}}/pulls/{}/merge -X PUT -f merge_method=squash",
        pr_number
    );
    log_info(&format!("マージを実行します (squash): PR #{}", pr_number));

    let (success, output) = run_cmd("merge", &merge_cmd, DEFAULT_MERGE_TIMEOUT_SECS);

    if !success {
        log_info("マージ失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info("マージ完了");

    // リモートブランチを削除 (gh api)
    let head_branch = run_gh_logged(&[
        "pr", "view", &pr_number.to_string(),
        "--json", "headRefName", "-q", ".headRefName",
    ]);
    if let Some(ref branch_name) = head_branch {
        let encoded_branch = branch_name.replace('/', "%2F");
        let delete_cmd = format!(
            "gh api repos/{{owner}}/{{repo}}/git/refs/heads/{} -X DELETE",
            encoded_branch
        );
        let (del_ok, del_out) = run_cmd("delete-branch", &delete_cmd, 30);
        if del_ok {
            log_info(&format!("リモートブランチ '{}' を削除しました", branch_name));
        } else if del_out.contains("Reference does not exist") {
            log_info(&format!("リモートブランチ '{}' は既に削除済みです（GitHub による自動削除）", branch_name));
        } else {
            let msg = if del_out.is_empty() {
                "不明なエラー".to_string()
            } else {
                del_out
            };
            log_info(&format!("リモートブランチ '{}' の削除失敗: {}", branch_name, msg));
        }
    }

    // ローカル同期
    let rc = sync_local(&branch);
    if rc != 0 { return rc; }

    // post-merge ステップ実行（学び提案等）
    if let Err(code) = run_steps("post-merge ステップ", &post_steps, timeout) {
        return code;
    }

    0
}

/// jj git fetch → jj new <branch> でローカルを最新に同期する
fn sync_local(branch: &str) -> i32 {
    log_info("ローカル同期中: jj git fetch");
    let (success, output) = run_cmd("fetch", "jj git fetch", DEFAULT_STEP_TIMEOUT_SECS);
    if !success {
        log_info("jj git fetch 失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    let new_cmd = format!("jj new {}", branch);
    log_info(&format!("ローカル同期中: {}", new_cmd));
    let (success, output) = run_cmd("new-branch", &new_cmd, DEFAULT_STEP_TIMEOUT_SECS);
    if !success {
        log_info(&format!("{} 失敗:", new_cmd));
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info(&format!("ローカル同期完了。{} の最新状態で作業を開始できます。", branch));
    0
}

fn main() {
    std::process::exit(run_pipeline());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_merge_pipeline_with_pre_and_post_steps() {
        let toml_str = r#"
[merge_pipeline]
step_timeout = 60
default_branch = "main"

[[merge_pipeline.pre_steps]]
name = "ci_check"
type = "command"
cmd = "gh pr checks --required"

[[merge_pipeline.post_steps]]
name = "post_merge_learnings"
type = "ai"
prompt = "analyze_pr_learnings"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(pipeline.step_timeout.unwrap(), 60);
        assert_eq!(pipeline.default_branch.as_deref(), Some("main"));

        let pre = pipeline.pre_steps.unwrap();
        assert_eq!(pre.len(), 1);
        assert_eq!(pre[0].name, "ci_check");
        assert_eq!(pre[0].step_type, "command");

        let post = pipeline.post_steps.unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].name, "post_merge_learnings");
        assert_eq!(post[0].step_type, "ai");
        assert_eq!(post[0].prompt.as_deref(), Some("analyze_pr_learnings"));
    }

    #[test]
    fn config_defaults_when_empty() {
        let toml_str = r#"
[merge_pipeline]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let pipeline = config.merge_pipeline.unwrap();
        assert_eq!(
            pipeline.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS
        );
        assert!(pipeline.pre_steps.unwrap_or_default().is_empty());
        assert!(pipeline.post_steps.unwrap_or_default().is_empty());
        assert_eq!(
            pipeline.default_branch.unwrap_or_else(|| DEFAULT_BRANCH.to_string()),
            DEFAULT_BRANCH
        );
    }

    #[test]
    fn config_missing_merge_pipeline_section() {
        let toml_str = r#"
[push_pipeline]
step_timeout = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.merge_pipeline.is_none());
    }

    #[test]
    fn combine_output_both_present() {
        assert_eq!(combine_output("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_output_only_stdout() {
        assert_eq!(combine_output("out", ""), "out");
    }

    #[test]
    fn combine_output_only_stderr() {
        assert_eq!(combine_output("", "err"), "err");
    }

    #[test]
    fn combine_output_both_empty() {
        assert_eq!(combine_output("", ""), "");
    }

    // ─── bookmark 検出ロジック ───

    #[test]
    fn parse_bookmark_list_output_empty() {
        assert!(parse_bookmark_list_output("").is_empty());
        assert!(parse_bookmark_list_output("\n\n").is_empty());
    }

    #[test]
    fn parse_bookmark_list_output_single() {
        assert_eq!(parse_bookmark_list_output("feat/x\n"), vec!["feat/x"]);
    }

    #[test]
    fn parse_bookmark_list_output_csv_on_one_line() {
        assert_eq!(
            parse_bookmark_list_output("feat/a,feat/b\n"),
            vec!["feat/a", "feat/b"]
        );
    }

    #[test]
    fn parse_bookmark_list_output_multiple_lines() {
        // @ と @- の両方にヒットした場合 (将来 revset を両方カバーする場合向け)
        let raw = "feat/current\nfeat/parent\n";
        assert_eq!(
            parse_bookmark_list_output(raw),
            vec!["feat/current", "feat/parent"]
        );
    }

    #[test]
    fn parse_bookmark_list_output_deduplicates() {
        let raw = "feat/x,feat/x\nfeat/x\n";
        assert_eq!(parse_bookmark_list_output(raw), vec!["feat/x"]);
    }

    #[test]
    fn parse_bookmark_list_output_trims_whitespace() {
        assert_eq!(
            parse_bookmark_list_output("  feat/a ,  feat/b  \n"),
            vec!["feat/a", "feat/b"]
        );
    }

    #[test]
    fn parse_bookmark_list_output_excludes_trunk_bookmarks() {
        // fresh checkout で @- が master を指すケース (option B の注意点)
        assert!(parse_bookmark_list_output("master\n").is_empty());
        assert_eq!(
            parse_bookmark_list_output("master,feat/x\n"),
            vec!["feat/x"]
        );
    }

    #[test]
    fn is_trunk_bookmark_known_names_rejected() {
        assert!(is_trunk_bookmark("main"));
        assert!(is_trunk_bookmark("master"));
        assert!(is_trunk_bookmark("trunk"));
        assert!(is_trunk_bookmark("develop"));
        assert!(!is_trunk_bookmark("feat/x"));
        assert!(!is_trunk_bookmark("main-feature"));
    }

    #[test]
    fn select_from_revsets_returns_empty_when_all_revsets_empty() {
        let result = select_from_revsets(&["@", "@-"], |_| Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn select_from_revsets_prefers_current_over_parent() {
        // @ と @- の両方に bookmark があるとき、近い @ を優先する
        let result = select_from_revsets(&["@", "@-"], |r| match r {
            "@" => vec!["feat/current".to_string()],
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        });
        assert_eq!(result, vec!["feat/current"]);
    }

    #[test]
    fn select_from_revsets_falls_back_to_parent_when_current_empty() {
        // PR #53 で実測した「@ 空, @- に bookmark」ケース
        let result = select_from_revsets(&["@", "@-"], |r| match r {
            "@" => Vec::new(),
            "@-" => vec!["feat/parent".to_string()],
            _ => Vec::new(),
        });
        assert_eq!(result, vec!["feat/parent"]);
    }

    #[test]
    fn select_from_revsets_stops_at_first_hit() {
        // 優先度の低い revset は検索されない (副作用が発生しないことを確認)
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::<String>::new());
        let result = select_from_revsets(&["@", "@-", "@--"], |r| {
            calls.borrow_mut().push(r.to_string());
            if r == "@-" {
                vec!["feat/hit".to_string()]
            } else {
                Vec::new()
            }
        });
        assert_eq!(result, vec!["feat/hit"]);
        assert_eq!(*calls.borrow(), vec!["@".to_string(), "@-".to_string()]);
    }
}
