use std::process::Command;

use crate::log::{log_info, log_stage};

pub(super) fn advance_jj_bookmarks() -> Result<(), String> {
    let target = match determine_target_revision()? {
        Some(rev) => rev,
        None => return Ok(()), // root commit 等で有効な target がない
    };
    let bookmarks = get_bookmarks_in_range(&target)?;

    if bookmarks.is_empty() {
        return Ok(()); // 前進対象なし (新規ブランチ等)
    }

    for bookmark in &bookmarks {
        match set_bookmark(bookmark, &target) {
            Ok(()) => log_stage(
                "push",
                &format!("bookmark '{}' を {} に自動更新", bookmark, target),
            ),
            Err(e) => {
                log_info(&format!("bookmark '{}' の更新失敗 (続行): {}", bookmark, e));
            }
        }
    }
    Ok(())
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

fn run_jj(args: &[&str], error_prefix: &str) -> Result<String, String> {
    let output = Command::new("jj")
        .args(args)
        .output()
        .map_err(|e| format!("{}: {}", error_prefix, e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
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
}
