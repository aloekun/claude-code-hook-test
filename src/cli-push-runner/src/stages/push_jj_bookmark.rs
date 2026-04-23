use lib_jj_helpers::is_trunk_bookmark;
use std::process::Command;

use crate::log::{log_info, log_stage};

pub(super) fn advance_jj_bookmarks() -> Result<(), String> {
    let target = match determine_target_revision()? {
        Some(rev) => rev,
        None => return Ok(()), // root commit 等で有効な target がない
    };
    let bookmarks = get_bookmarks_in_range(&target)?;

    if bookmarks.is_empty() {
        // Fallback: takt fix が @ を amend すると旧 commit が obsolete になり、
        // revset ベースの検索では発見できない。`jj bookmark list` は obsolete
        // commit 上の bookmark も返すため、こちらで再探索する。
        return advance_bookmarks_via_list(&target);
    }

    apply_bookmarks(&bookmarks, &target, "");
    Ok(())
}

fn apply_bookmarks(bookmarks: &[String], target: &str, label: &str) {
    for bookmark in bookmarks {
        match set_bookmark(bookmark, target) {
            Ok(()) => log_stage(
                "push",
                &format!("bookmark '{}' を {} に自動更新{}", bookmark, target, label),
            ),
            Err(e) => {
                log_info(&format!(
                    "bookmark '{}' の更新失敗{} (続行): {}",
                    bookmark, label, e
                ));
            }
        }
    }
}

/// `jj bookmark list` の出力から非 trunk ローカル bookmark を取得し、target に前進させる。
/// revset ベースの `get_bookmarks_in_range` が空を返した場合のフォールバック。
///
/// 安全策: 非 trunk bookmark が 1 つだけの場合のみ前進させる。
/// 複数ある場合は無関係な bookmark を誤って移動するリスクがあるためスキップする。
fn advance_bookmarks_via_list(target: &str) -> Result<(), String> {
    let bookmarks = get_local_bookmarks_from_list()?;
    dispatch_bookmark_advance(&bookmarks, target, |b, t| {
        apply_bookmarks(b, t, " (fallback)")
    });
    Ok(())
}

fn dispatch_bookmark_advance(
    bookmarks: &[String],
    target: &str,
    apply: impl FnOnce(&[String], &str),
) {
    match bookmarks.len() {
        0 => {
            log_info("ローカル bookmark が見つかりません (新規ブランチ等)");
        }
        1 => {
            log_info(&format!(
                "fallback: bookmark '{}' を {} に前進させます",
                bookmarks[0], target
            ));
            apply(bookmarks, target);
        }
        _ => {
            // 複数の非 trunk bookmark がある場合、無関係な bookmark を
            // 誤って移動するリスクがあるためスキップする
            log_info(&format!(
                "複数の bookmark ({}) が存在するため fallback 更新をスキップします: {}",
                bookmarks.len(),
                bookmarks.join(", ")
            ));
        }
    }
}

fn determine_target_revision() -> Result<Option<String>, String> {
    let output = run_jj_log("@", "if(empty, \"empty\", \"content\")")?;
    if output.trim() == "empty" {
        match run_jj_log("@-", "commit_id") {
            Ok(_) => Ok(Some("@-".to_string())),
            Err(_) => {
                log_info("@ が root commit のため bookmark 自動更新をスキップします");
                Ok(None)
            }
        }
    } else {
        Ok(Some("@".to_string()))
    }
}

fn get_bookmarks_in_range(target: &str) -> Result<Vec<String>, String> {
    let revsets = [
        format!("(trunk()..{}) & bookmarks()", target),
        format!("(main..{}) & bookmarks()", target),
        format!("(master..{}) & bookmarks()", target),
    ];

    for revset in &revsets {
        let template = "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"";
        match run_jj_log(revset, template) {
            Ok(output) => {
                let bookmarks = parse_bookmarks_from_template(&output);
                if !bookmarks.is_empty() {
                    return Ok(dedup(bookmarks));
                }
            }
            Err(_) => continue,
        }
    }

    // (push 自体は続行するので Err を返さず警告に留める)
    log_info("trunk/main/master bookmark が見つからず、bookmark 自動更新をスキップします");
    Ok(Vec::new())
}

fn parse_bookmarks_from_template(raw: &str) -> Vec<String> {
    raw.lines()
        .flat_map(|line| line.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

const JJ_TIMEOUT_SECS: u64 = 30;

fn run_jj(args: &[&str], error_prefix: &str) -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {}", error_prefix, e))?;

    let stdout_handle =
        crate::runner::drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        crate::runner::drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let status = crate::runner::wait_with_timeout(error_prefix, &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("{}: {}", error_prefix, e))?;

    let stdout_text = stdout_handle.join().unwrap_or_default();
    let stderr_text = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!(
            "{}: タイムアウト ({}s)",
            error_prefix, JJ_TIMEOUT_SECS
        )),
        Some(s) if s.success() => Ok(stdout_text),
        Some(_) => Err(stderr_text.trim().to_string()),
    }
}

fn set_bookmark(name: &str, target: &str) -> Result<(), String> {
    run_jj(
        &["bookmark", "set", "-r", target, "--", name],
        "jj bookmark set 実行失敗",
    )?;
    Ok(())
}

fn run_jj_log(revset: &str, template: &str) -> Result<String, String> {
    run_jj(
        &["log", "-r", revset, "--no-graph", "-T", template],
        "jj log 実行失敗",
    )
}

/// `jj bookmark list` の出力をパースし、非 trunk のローカル bookmark 名を返す。
/// 出力形式: "name: commit_id description\n  @origin: commit_id\n"
/// インデントで始まる行はリモート追跡情報なのでスキップする。
fn get_local_bookmarks_from_list() -> Result<Vec<String>, String> {
    let output = run_jj(&["bookmark", "list"], "jj bookmark list 実行失敗")?;
    Ok(dedup(parse_bookmark_list_output(&output)))
}

fn parse_bookmark_list_output(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dedup ---

    #[test]
    fn dedup_preserves_order_and_removes_duplicates() {
        let input = vec![
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
            "b".to_string(),
        ];
        assert_eq!(dedup(input), vec!["a", "b", "c"]);
    }

    #[test]
    fn dedup_empty_returns_empty() {
        assert_eq!(dedup(Vec::new()), Vec::<String>::new());
    }

    #[test]
    fn dedup_single_unchanged() {
        assert_eq!(dedup(vec!["x".to_string()]), vec!["x"]);
    }

    // --- parse_bookmarks_from_template ---

    #[test]
    fn parse_bookmarks_empty_string_returns_empty() {
        assert_eq!(parse_bookmarks_from_template(""), Vec::<String>::new());
    }

    #[test]
    fn parse_bookmarks_single_name() {
        assert_eq!(
            parse_bookmarks_from_template("main\n"),
            vec!["main".to_string()]
        );
    }

    #[test]
    fn parse_bookmarks_comma_separated_single_line() {
        assert_eq!(
            parse_bookmarks_from_template("feat/foo,feat/bar,fix/baz\n"),
            vec!["feat/foo", "feat/bar", "fix/baz"]
        );
    }

    #[test]
    fn parse_bookmarks_multi_line_output() {
        let raw = "feat/a,feat/b\nfeat/c\n";
        assert_eq!(
            parse_bookmarks_from_template(raw),
            vec!["feat/a", "feat/b", "feat/c"]
        );
    }

    #[test]
    fn parse_bookmarks_strips_leading_trailing_whitespace() {
        assert_eq!(
            parse_bookmarks_from_template("  main , dev  \n"),
            vec!["main", "dev"]
        );
    }

    #[test]
    fn parse_bookmarks_filters_whitespace_only_entries() {
        assert_eq!(
            parse_bookmarks_from_template(",  ,feat/x,\n"),
            vec!["feat/x".to_string()]
        );
    }

    #[test]
    fn parse_bookmarks_with_duplicates_returned_as_is() {
        assert_eq!(
            parse_bookmarks_from_template("a,a,b\n"),
            vec!["a", "a", "b"]
        );
    }

    // --- is_trunk_bookmark / TRUNK_BOOKMARKS ---
    //
    // lib-jj-helpers に集約済 (ADR-024 本採用、PR-C で移設)。
    // cli-push-runner 側からは lib_jj_helpers::is_trunk_bookmark を呼び出す。

    // --- parse_bookmark_list_output ---

    #[test]
    fn parse_bookmark_list_typical_output() {
        let output = "\
feat/xyz: abc1234 add feature
  @origin: abc1234 add feature
main: def5678 initial
  @origin: def5678 initial
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }

    #[test]
    fn parse_bookmark_list_multiple_feature_bookmarks() {
        let output = "\
feat/a: 111 desc
feat/b: 222 desc
main: 333 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/a", "feat/b"]);
    }

    #[test]
    fn parse_bookmark_list_empty_output() {
        assert_eq!(parse_bookmark_list_output(""), Vec::<String>::new());
    }

    #[test]
    fn parse_bookmark_list_only_trunk() {
        let output = "main: abc123 desc\nmaster: def456 desc\n";
        assert_eq!(parse_bookmark_list_output(output), Vec::<String>::new());
    }

    // --- dispatch_bookmark_advance ---

    #[test]
    fn dispatch_zero_bookmarks_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(&[], "abc123", |_, _| called.set(true));
        assert!(!called.get());
    }

    #[test]
    fn dispatch_one_bookmark_calls_apply_with_correct_args() {
        let captured = std::cell::RefCell::new(None::<(Vec<String>, String)>);
        dispatch_bookmark_advance(&["feat/xyz".to_string()], "abc123", |b, t| {
            *captured.borrow_mut() = Some((b.to_vec(), t.to_string()))
        });
        assert_eq!(
            *captured.borrow(),
            Some((vec!["feat/xyz".to_string()], "abc123".to_string()))
        );
    }

    #[test]
    fn dispatch_multiple_bookmarks_does_not_call_apply() {
        let called = std::cell::Cell::new(false);
        dispatch_bookmark_advance(
            &["feat/a".to_string(), "feat/b".to_string()],
            "abc123",
            |_, _| called.set(true),
        );
        assert!(!called.get());
    }

    #[test]
    fn parse_bookmark_list_skips_indented_remote_lines() {
        let output = "\
feat/xyz: abc1234 desc
  @origin: abc1234 desc
  @upstream: abc1234 desc
";
        assert_eq!(parse_bookmark_list_output(output), vec!["feat/xyz"]);
    }
}
