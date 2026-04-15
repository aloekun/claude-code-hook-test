use std::path::PathBuf;

use crate::config::DEFAULT_STEP_TIMEOUT_SECS;
use crate::log::log_info;
use crate::runner::{run_cmd_direct, run_gh_quiet};
use crate::stages::monitor::start_monitoring;
use crate::util::{
    get_jj_bookmarks, get_pr_info, parse_pr_number_from_url, utc_now_iso8601, PrInfo,
};

// ─── --body -> --body-file 変換 (issue #1) ───

/// Drop 時に自動削除される一時ファイル
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// --body 引数に改行が含まれる場合、一時ファイルに書き出して --body-file に差し替える。
fn convert_body_to_file(args: &[String]) -> (Vec<String>, Option<TempFile>) {
    let mut result = Vec::with_capacity(args.len());
    let mut i = 0;
    let mut temp_guard: Option<TempFile> = None;

    while i < args.len() {
        if args[i] == "--body" && i + 1 < args.len() {
            let body = &args[i + 1];
            if body.contains('\n') || body.contains("\\n") {
                let filename = format!(
                    "gh-pr-body-{}-{}.md",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );
                let path = std::env::temp_dir().join(filename);
                let resolved = body.replace("\\n", "\n");
                match std::fs::write(&path, &resolved) {
                    Ok(()) => {
                        log_info(&format!(
                            "--body に改行を検出 → --body-file に変換 ({})",
                            path.display()
                        ));
                        result.push("--body-file".to_string());
                        result.push(path.to_string_lossy().to_string());
                        temp_guard = Some(TempFile(path));
                    }
                    Err(e) => {
                        log_info(&format!(
                            "警告: body ファイル書き出し失敗: {}。--body をそのまま使用",
                            e
                        ));
                        result.push(args[i].clone());
                        result.push(args[i + 1].clone());
                    }
                }
                i += 2;
                continue;
            }
        }
        result.push(args[i].clone());
        i += 1;
    }

    (result, temp_guard)
}

// ─── --head 補完 ───

/// `--head` / `--head=<non-empty>` / `-H` のいずれかが有効な値付きで引数リストに含まれるか判定する。
/// `--head=` (空値) は無効扱いとし、自動補完の対象にする。
fn has_head_flag(args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--head" || a == "-H" {
            if let Some(v) = args.get(i + 1) {
                if !v.is_empty() && !v.starts_with('-') {
                    return true;
                }
            }
        } else if let Some(v) = a.strip_prefix("--head=") {
            if !v.is_empty() {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// `--head` 系フラグが未指定のとき、bookmarks の先頭を `--head <bookmark>` として追記する。
/// 既に指定済みの場合は args をそのまま返す。bookmarks が空のときも変更なし。
/// 値欠落の `--head` / `-H` / `--head=` は無効指定として除去してから補完する。
fn ensure_head_arg(args: Vec<String>, bookmarks: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--head=" {
            i += 1;
            continue;
        }
        if a == "--head" || a == "-H" {
            match args.get(i + 1) {
                Some(v) if !v.is_empty() && !v.starts_with('-') => {
                    normalized.push(a.clone());
                    normalized.push(v.clone());
                    i += 2;
                }
                _ => {
                    i += 1;
                }
            }
            continue;
        }
        normalized.push(a.clone());
        i += 1;
    }

    if has_head_flag(&normalized) {
        return normalized;
    }
    if let Some(bookmark) = bookmarks.first() {
        normalized.push("--head".to_string());
        normalized.push(bookmark.clone());
    }
    normalized
}

// ─── PR 作成モード ───

pub(crate) fn run_create_pr(gh_args: &[String]) -> i32 {
    log_info("PR 作成モード");

    // --body に改行が含まれる場合、--body-file に自動変換
    let (mut final_args, _body_tempfile) = convert_body_to_file(gh_args);

    // jj 環境対応: --head 未指定時に jj bookmark から自動補完
    // gh pr create は git の current branch を検出するが、jj 環境では
    // "not on any branch" エラーになるため、明示的に --head を指定する
    if !has_head_flag(&final_args) {
        let bookmarks = get_jj_bookmarks();
        final_args = ensure_head_arg(final_args, &bookmarks);
        if let Some(bookmark) = bookmarks.first() {
            log_info(&format!("jj bookmark '{}' を --head に自動補完", bookmark));
        }
    }

    log_info(&format!(
        "実行: gh pr create {}",
        final_args
            .iter()
            .map(|a| {
                if a.contains(' ') {
                    format!("\"{}\"", a)
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    ));

    let (success, output) = run_cmd_direct(
        "gh",
        &["pr", "create"],
        &final_args,
        DEFAULT_STEP_TIMEOUT_SECS,
    );

    if !success {
        log_info("PR 作成失敗:");
        if !output.is_empty() {
            eprintln!("{}", output);
        }
        return 1;
    }

    log_info("PR 作成完了");
    // PR URL を表示 (Claude が読める stdout に出力)
    if !output.is_empty() {
        println!("{}", output);
    }

    // PR 情報取得: gh pr create の出力から PR 番号を直接パース
    let pr_number_from_url = parse_pr_number_from_url(&output);
    let push_time = utc_now_iso8601();

    let pr_info = if pr_number_from_url.is_some() {
        log_info(&format!("PR URL から番号を取得: {:?}", pr_number_from_url));
        let repo = run_gh_quiet(&[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ]);
        PrInfo {
            pr_number: pr_number_from_url,
            repo,
        }
    } else {
        log_info("PR URL からの番号取得失敗、gh コマンドで検索");
        get_pr_info()
    };

    start_monitoring(&pr_info, &push_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // --- convert_body_to_file ---

    #[test]
    fn body_without_newline_unchanged() {
        let args = vec![
            "--title".into(),
            "test".into(),
            "--body".into(),
            "simple body".into(),
        ];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result, args);
        assert!(temp.is_none());
    }

    #[test]
    fn body_with_literal_newline_converted() {
        let args = vec![
            "--title".into(),
            "test".into(),
            "--body".into(),
            "line1\\nline2".into(),
        ];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result[0], "--title");
        assert_eq!(result[1], "test");
        assert_eq!(result[2], "--body-file");
        assert!(temp.is_some());
        let content = std::fs::read_to_string(&temp.as_ref().unwrap().0).unwrap();
        assert!(content.contains("line1\nline2"));
    }

    #[test]
    fn body_with_real_newline_converted() {
        let args = vec!["--body".into(), "line1\nline2".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result[0], "--body-file");
        assert!(temp.is_some());
    }

    #[test]
    fn no_body_arg_unchanged() {
        let args = vec!["--title".into(), "test".into()];
        let (result, temp) = convert_body_to_file(&args);
        assert_eq!(result, args);
        assert!(temp.is_none());
    }

    // --- ensure_head_arg ---

    #[test]
    fn ensure_head_arg_empty_bookmarks_unchanged() {
        let args = strs(&["--title", "test"]);
        let result = ensure_head_arg(args.clone(), &[]);
        assert_eq!(result, args);
    }

    #[test]
    fn ensure_head_arg_long_flag_present_unchanged() {
        let args = strs(&["--title", "test", "--head", "main"]);
        let result = ensure_head_arg(args.clone(), &["other".to_string()]);
        assert_eq!(result, args);
    }

    #[test]
    fn ensure_head_arg_long_eq_flag_present_unchanged() {
        let args = strs(&["--head=feature/xyz"]);
        let result = ensure_head_arg(args.clone(), &["other".to_string()]);
        assert_eq!(result, args);
    }

    #[test]
    fn ensure_head_arg_short_flag_present_unchanged() {
        let args = strs(&["-H", "feature/branch"]);
        let result = ensure_head_arg(args.clone(), &["other".to_string()]);
        assert_eq!(result, args);
    }

    #[test]
    fn ensure_head_arg_no_head_single_bookmark_appended() {
        let args = strs(&["--title", "test"]);
        let bookmarks = vec!["my-feature".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--title", "test", "--head", "my-feature"]));
    }

    #[test]
    fn ensure_head_arg_no_head_multiple_bookmarks_uses_first() {
        let args = strs(&["--title", "test"]);
        let bookmarks = vec!["first".to_string(), "second".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--title", "test", "--head", "first"]));
    }

    #[test]
    fn ensure_head_arg_empty_eq_value_triggers_completion() {
        let args = strs(&["--head="]);
        let bookmarks = vec!["my-feature".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--head", "my-feature"]));
    }

    #[test]
    fn has_head_flag_empty_eq_returns_false() {
        assert!(!has_head_flag(&strs(&["--head="])));
    }

    #[test]
    fn has_head_flag_nonempty_eq_returns_true() {
        assert!(has_head_flag(&strs(&["--head=feature"])));
    }

    #[test]
    fn has_head_flag_long_with_value_returns_true() {
        assert!(has_head_flag(&strs(&["--head", "feature"])));
    }

    #[test]
    fn has_head_flag_long_without_value_returns_false() {
        assert!(!has_head_flag(&strs(&["--head"])));
    }

    #[test]
    fn ensure_head_arg_bare_head_stripped_and_bookmark_appended() {
        let args = strs(&["--title", "test", "--head"]);
        let bookmarks = vec!["my-feature".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--title", "test", "--head", "my-feature"]));
    }

    #[test]
    fn ensure_head_arg_bare_short_h_stripped_and_bookmark_appended() {
        let args = strs(&["--title", "test", "-H"]);
        let bookmarks = vec!["my-feature".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--title", "test", "--head", "my-feature"]));
    }

    #[test]
    fn ensure_head_arg_head_followed_by_flag_stripped() {
        let args = strs(&["--head", "--title", "test"]);
        let bookmarks = vec!["my-feature".to_string()];
        let result = ensure_head_arg(args, &bookmarks);
        assert_eq!(result, strs(&["--title", "test", "--head", "my-feature"]));
    }
}
