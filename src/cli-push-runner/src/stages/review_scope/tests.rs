//! review_scope の単体テスト (ADR-089)。補助関数は他の test module と共有しない (ADR-084)。

use super::*;

fn approved(commit: &str, base: &str) -> Approved {
    Approved { commit: commit.to_string(), base: base.to_string(), at: "2026-10-25T00:00:00Z".to_string() }
}

fn diff_config(incremental: Option<&str>) -> DiffConfig {
    DiffConfig {
        command: "jj diff --git -r {{PR_RANGE}}".to_string(),
        incremental_command: incremental.map(str::to_string),
        output_path: ".takt/review-diff.txt".to_string(),
        timeout: None,
        default_branch: None,
    }
}

#[test]
fn the_approved_commit_is_the_start_when_the_base_is_unchanged_and_readable_and_remotely_pushed() {
    let record = approved("abc", "base1");
    assert_eq!(
        choose_since(true, Some(&record), Some("base1"), |c| Some(c.to_string()), |_| true),
        Some("abc".to_string())
    );
}

#[test]
fn the_resolved_canonical_id_is_returned_instead_of_the_raw_stored_string() {
    // NOTE: 生文字列は後段で diff コマンドのテンプレートに展開されシェル実行に渡るため、
    // NOTE: jj が解決した commit ID を使う (承認記録の偽装対策)。
    let record = approved("HEAD~0", "base1");
    assert_eq!(
        choose_since(true, Some(&record), Some("base1"), |_| Some("canonical-id".to_string()), |_| true),
        Some("canonical-id".to_string())
    );
}

#[test]
fn the_whole_pr_is_reviewed_when_any_condition_fails() {
    let record = approved("abc", "base1");
    let resolves = |c: &str| Some(c.to_string());
    assert_eq!(choose_since(false, Some(&record), Some("base1"), resolves, |_| true), None, "無効化");
    assert_eq!(choose_since(true, None, Some("base1"), resolves, |_| true), None, "記録が無い");
    assert_eq!(choose_since(true, Some(&record), Some("base2"), resolves, |_| true), None, "基点が変わった (rebase)");
    assert_eq!(choose_since(true, Some(&record), None, resolves, |_| true), None, "基点を読めない");
    assert_eq!(choose_since(true, Some(&record), Some("base1"), |_| None, |_| true), None, "承認した commit を読めない");
}

#[test]
fn a_forged_record_pointing_at_a_commit_never_pushed_to_remote_is_rejected() {
    let record = approved("forged", "base1");
    assert_eq!(
        choose_since(true, Some(&record), Some("base1"), |c| Some(c.to_string()), |_| false),
        None,
        "remote 追跡 ref と一致しない記録は PR 全体レビューに倒す"
    );
}

#[test]
fn a_record_changed_while_takt_ran_is_detected() {
    let record = r#"{"feat/x":{"commit":"abc"}}"#;
    assert!(!approvals_changed(Some(record), Some(record)), "変わっていない");
    assert!(!approvals_changed(None, None), "前後とも無い");
    assert!(approvals_changed(None, Some(record)), "takt の実行中に書かれた");
    assert!(approvals_changed(Some(record), Some(r#"{"feat/x":{"commit":"will-be-pushed"}}"#)), "書き換えられた");
    assert!(approvals_changed(Some(record), None), "消された");
}

#[test]
fn each_scope_builds_its_own_diff_command() {
    let config = diff_config(Some("jj diff --git --from {{FROM}} --to @"));
    let full = ReviewScope::Full { pr_range: "master@origin..@".to_string() };
    let since = ReviewScope::Since { from: "abc123".to_string(), template: "jj diff --git --from {{FROM}} --to @".to_string() };
    assert_eq!(full.command(&config), "jj diff --git -r master@origin..@");
    assert_eq!(since.command(&config), "jj diff --git --from abc123 --to @");
    assert_eq!((full.telemetry_label(), since.telemetry_label()), ("full", "incremental"));
}

#[test]
fn the_scope_note_names_the_start_commit() {
    let note = scope_note("abc123");
    assert!(note.contains("`abc123`") && note.contains("ADR-089"), "{note}");
}

#[test]
fn the_base_is_the_left_side_of_the_pr_range() {
    assert_eq!(base_of("master@origin..@"), "master@origin");
    assert_eq!(base_of("trunk()..@"), "trunk()");
}

#[test]
fn only_the_same_commit_as_the_remote_tip_matches() {
    assert!(tip_matches(Some("abc"), "abc"));
    assert!(!tip_matches(Some("abc"), "forged"), "remote と違う commit");
    assert!(!tip_matches(None, "abc"), "remote 追跡 ref を読めない");
}
