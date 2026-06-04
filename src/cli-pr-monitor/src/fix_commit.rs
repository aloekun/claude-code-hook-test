//! 分離型 fix commit の pre-create と description 生成。
//!
//! ADR-022 例外条項 (2026-04-20): 自動生成された修正を独立した child commit として
//! 分離する場合に限り、その child commit への description 付与を許可する。
//! 元 commit (= 人間が意図を込めた初回 PR commit) の description は改変しない。
//!
//! pre-takt で `jj new -m "..."` により空 child を作成し、takt が `@` を amend する
//! ことで fix 内容が自動的に child commit へ入る仕組み。

use lib_report_formatter::Finding;

use crate::log::log_info;
use crate::runner::{capture_commit_id, diff_at_is_empty, run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

/// 分離型 fix commit の状態。
///
/// pre-takt で作成を試み、成否を型で表現する。
/// post-takt の分岐 (re-push / abandon / 放置) で消費される。
#[derive(Debug, Clone)]
pub(crate) enum FixCommitState {
    /// 分離を行わなかった (findings なし、takt 未構成、または作成失敗)
    None,
    /// fix commit を pre-create 済み
    Created { commit_id: String },
}

impl FixCommitState {
    pub(crate) fn is_created(&self) -> bool {
        matches!(self, Self::Created { .. })
    }
}

/// fix commit の description を生成する。
///
/// ADR-022 例外の「新規 child commit への自己記述」として、
/// - header ラベル: commit 種別を示す
/// - findings summary: 何を問題と捉え、どれを修正したかの文脈
///
/// の 2 段構成で返す。findings が空なら header のみ返す。
pub(crate) fn build_fix_commit_description(pr_number: Option<u64>, findings: &[Finding]) -> String {
    let header = match pr_number {
        Some(n) => format!("fix(review): apply CodeRabbit fixes for #{}", n),
        None => "fix(review): apply CodeRabbit fixes".to_string(),
    };

    if findings.is_empty() {
        return header;
    }

    let mut body = String::with_capacity(256);
    body.push_str(&header);
    body.push_str("\n\nResolved findings:\n");
    for f in findings {
        let issue_oneline = sanitize_to_oneline(&f.issue);
        body.push_str(&format!(
            "- [{}] {}:{} {}\n",
            f.severity, f.file, f.line, issue_oneline
        ));
    }
    body.trim_end().to_string()
}

/// CodeRabbit の `issue` フィールドは複数行になることがあるため、
/// `build_fix_commit_description` のリスト項目に埋める前に単行化する。
fn sanitize_to_oneline(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// pre-takt で fix commit を新規作成する (`jj new -m "..."`)。
///
/// 成功時: `FixCommitState::Created { commit_id }` を返す。@ は空 child を指す状態。
/// 失敗時: `FixCommitState::None` を返す (fallback = 分離なしで元の flow へフォールバック)。
///
/// `jj new` が成功したが `capture_commit_id` で commit id を追跡できない場合は、
/// 作成済みの空 child が orphan にならないよう即座に abandon を試みる
/// (fail-safe: 追跡不能 child を remote に残さない)。
pub(crate) fn create_fix_commit(pr_number: Option<u64>, findings: &[Finding]) -> FixCommitState {
    let desc = build_fix_commit_description(pr_number, findings);
    let (ok, output) = run_cmd_direct("jj", &["new", "-m", &desc], &[], JJ_CMD_TIMEOUT_SECS);
    if !ok {
        log_info(&format!(
            "[action] fix commit 分離 skip: jj new 失敗: {}",
            output
        ));
        return FixCommitState::None;
    }
    match capture_commit_id() {
        Some(cid) => {
            log_info(&format!("[state] fix commit pre-created: {}", cid));
            FixCommitState::Created { commit_id: cid }
        }
        None => {
            log_info(
                "[state] fix commit 作成後の commit id capture 失敗 (orphan child を cleanup)",
            );
            try_abandon_empty_fix_commit("create_fix_commit id capture 失敗:", None);
            FixCommitState::None
        }
    }
}

/// 空 fix commit を安全に abandon する。
///
/// `commit_id` が `Some(expected)` のとき: 現在の `@` が `expected` と一致する場合のみ
/// abandon を実行する。不一致または capture 失敗時は `[warn]` を出してスキップする。
/// `commit_id` が `None` のとき: 従来通り diff チェックのみで判定する。
///
/// diff あり判定失敗時は abandon をスキップ (fail-safe: 誤 abandon 防止)。
///
/// abandon 成功後は `reparent_at_to_pr_tip` で `@` を PR tip 直下に戻す
/// (task 6: cleanup 後の @ 孤児化を解消)。
pub(crate) fn try_abandon_empty_fix_commit(context: &str, commit_id: Option<&str>) {
    if let Some(expected) = commit_id {
        match capture_commit_id().as_deref() {
            Some(current) if current == expected => {}
            Some(current) => {
                log_info(&format!(
                    "[warn] {} expected={}, current={} abandon を見送り",
                    context, expected, current
                ));
                return;
            }
            None => {
                log_info(&format!(
                    "[warn] {} expected={}, current=<capture失敗> abandon を見送り",
                    context, expected
                ));
                return;
            }
        }
    }

    if diff_at_is_empty() {
        let label = commit_id.map_or_else(String::new, |id| format!(" ({})", id));
        log_info(&format!(
            "[action] {} 空 fix commit を abandon{}",
            context, label
        ));
        let (ok, out) = run_cmd_direct("jj", &["abandon"], &[], JJ_CMD_TIMEOUT_SECS);
        if !ok {
            log_info(&format!(
                "[action] jj abandon 失敗 (手動片付け推奨): {}",
                out
            ));
            return;
        }
        reparent_at_to_pr_tip(context);
    } else {
        log_info(&format!(
            "[warn] {} fix commit に diff あり、abandon を見送り",
            context
        ));
    }
}

/// `@` を PR tip (単一 local bookmark の指す commit) 直下に再配置する。
///
/// `jj abandon` 直後の `@` は stale な空 commit の上に残ることがあり
/// (task 6 背景: PR #64 で 3 回発生)、次の `jj new` がそこに積まれる。
/// これを解消するため、bookmark が指す PR tip を解決して `jj new -r <tip>` で
/// `@` を PR tip の直接子に戻す。
///
/// 以下のケースは fail-safe でスキップする:
/// - PR tip 解決失敗 (bookmark なし / 複数 bookmark で曖昧 / 取得失敗)
/// - 既に `@-` が PR tip と一致 (redundant な空 commit を作らない)
/// - `jj new -r <tip>` 自体の失敗 (ログのみで処理を継続)
fn reparent_at_to_pr_tip(context: &str) {
    let pr_tip = match crate::stages::push_jj_bookmark::resolve_pr_tip_commit_id() {
        Some(id) => id,
        None => {
            log_info(&format!(
                "[state] {} PR tip bookmark を特定できず re-parent スキップ",
                context
            ));
            return;
        }
    };

    if parent_commit_id_is(&pr_tip) {
        log_info(&format!(
            "[state] {} @ は既に PR tip ({}) 直下、re-parent 不要",
            context, pr_tip
        ));
        return;
    }

    let (ok, out) = run_cmd_direct("jj", &["new", "-r", &pr_tip], &[], JJ_CMD_TIMEOUT_SECS);
    if ok {
        log_info(&format!(
            "[action] {} @ を PR tip ({}) 直下に re-parent",
            context, pr_tip
        ));
    } else {
        log_info(&format!(
            "[action] {} @ の re-parent 失敗 (手動対応): {}",
            context, out
        ));
    }
}

/// `default_branch..@` 範囲の `fix(review):` 空 commit を sweep して全て abandon する (順位 155、PR #174 T1-#1)。
///
/// 既存 `try_abandon_empty_fix_commit` が tracked な単一 fix commit (= 直近 `create_fix_commit`
/// の戻り値) のみを対象とするのに対し、本関数は PR 範囲全体を sweep して
/// **untracked な空 commit** を網羅的に拾う。PR #174 で観測した `kqvluqyv` 事例
/// (過去 fix loop で取りこぼされた granduncle 位置の空 commit が後続 push で PR diff 汚染) の
/// 構造的予防層。
///
/// 実装: jj revset `empty() & description("fix(review):") & (default_branch..@)` で範囲内の
/// fix(review): 空 commit を 1 step で列挙し、change_id ベースで順次 `jj abandon` する。
/// change_id は jj の永続識別子のため、複数 abandon で graph が rebase されても残りの id 参照は invariant。
/// description フィルタにより `create_fix_commit` 由来のコミットのみを対象とし、他の空コミットは除外する。
///
/// fail-open: jj log / abandon の失敗時は warn ログのみで cleanup を継続する
/// (push を block すると fix loop 全体が止まるため、ローカル副作用は次回再走で吸収する方針)。
pub(crate) fn sweep_empty_commits_in_pr_range(default_branch: &str) {
    let revset = format!(
        "empty() & description(substring:\"fix(review):\") & ({}..@)",
        default_branch
    );
    let (ok, out) = run_cmd_direct(
        "jj",
        &[
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-T",
            "change_id ++ \"\\n\"",
        ],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        log_info(&format!(
            "[warn] sweep_empty_commits: jj log 失敗 (sweep skip): {}",
            out.trim()
        ));
        return;
    }

    let change_ids = parse_empty_change_ids(&out);
    if change_ids.is_empty() {
        return;
    }
    log_info(&format!(
        "[action] sweep_empty_commits: {}..@ 範囲に fix(review): 空 commit {} 件を検出 → abandon",
        default_branch,
        change_ids.len()
    ));
    for cid in &change_ids {
        let (ok, out) = run_cmd_direct("jj", &["abandon", cid], &[], JJ_CMD_TIMEOUT_SECS);
        if !ok {
            log_info(&format!(
                "[warn] sweep_empty_commits: jj abandon {} 失敗 (継続): {}",
                cid,
                out.trim()
            ));
            continue;
        }
        log_info(&format!("[action] sweep_empty_commits: abandoned {}", cid));
    }
}

/// `jj log` 出力 (1 行 1 change_id) を parse する純関数。空行と前後空白を除去する。
fn parse_empty_change_ids(log_output: &str) -> Vec<String> {
    log_output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `@-` (親 commit) の id が `expected` と一致するか判定する。
/// 取得失敗時は `false` (= 不一致扱いで reparent を試行) を返す。
fn parent_commit_id_is(expected: &str) -> bool {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["log", "-r", "@-", "--no-graph", "-T", "commit_id"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    ok && out.trim() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: &str, file: &str, line: &str, issue: &str) -> Finding {
        Finding {
            severity: severity.to_string(),
            file: file.to_string(),
            line: line.to_string(),
            issue: issue.to_string(),
            suggestion: String::new(),
            source: "CodeRabbit".to_string(),
        }
    }

    #[test]
    fn description_without_findings_is_header_only() {
        let desc = build_fix_commit_description(Some(42), &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes for #42");
    }

    #[test]
    fn description_without_pr_number_falls_back_to_generic_header() {
        let desc = build_fix_commit_description(None, &[]);
        assert_eq!(desc, "fix(review): apply CodeRabbit fixes");
    }

    #[test]
    fn description_with_findings_includes_summary_block() {
        let fs = vec![
            finding("Major", "src/foo.rs", "12", "null pointer"),
            finding("Minor", "src/bar.rs", "34", "unused variable"),
        ];
        let desc = build_fix_commit_description(Some(42), &fs);
        assert!(
            desc.starts_with("fix(review): apply CodeRabbit fixes for #42\n\nResolved findings:\n")
        );
        assert!(desc.contains("- [Major] src/foo.rs:12 null pointer"));
        assert!(desc.contains("- [Minor] src/bar.rs:34 unused variable"));
        assert!(!desc.ends_with('\n'));
    }

    #[test]
    fn description_with_findings_without_pr_number() {
        let fs = vec![finding("Major", "a.rs", "1", "issue")];
        let desc = build_fix_commit_description(None, &fs);
        assert!(desc.starts_with("fix(review): apply CodeRabbit fixes\n\n"));
        assert!(desc.contains("- [Major] a.rs:1 issue"));
    }

    #[test]
    fn description_sanitizes_multiline_issue_into_single_line() {
        let fs = vec![finding(
            "Major",
            "src/foo.rs",
            "10",
            "first line\nsecond line\r\nthird  line",
        )];
        let desc = build_fix_commit_description(Some(1), &fs);
        assert!(
            desc.contains("- [Major] src/foo.rs:10 first line second line third line"),
            "multi-line issue が単行化されていない: {:?}",
            desc
        );
        let bullet_lines: Vec<_> = desc.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(bullet_lines.len(), 1, "bullet は 1 行のみ: {:?}", desc);
    }

    #[test]
    fn sanitize_to_oneline_preserves_single_spacing_and_trims() {
        assert_eq!(sanitize_to_oneline("a  b\nc\td"), "a b c d");
        assert_eq!(sanitize_to_oneline("   leading   "), "leading");
        assert_eq!(sanitize_to_oneline(""), "");
    }

    /// 統合: `create_fix_commit` の fail-safe cleanup 動作を確認する。
    ///
    /// `capture_commit_id` 失敗を直接 inject できないため、代わりに
    /// `try_abandon_empty_fix_commit(_, None)` を直接呼んで「空 child が cleanup される」
    /// 挙動 (= None 分岐が依拠する唯一の副作用) が jj で動くことを確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_empty_fix_commit_without_id_drops_orphan_child() {
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        assert!(StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init 失敗")
            .success());

        std::fs::write(repo_dir.join("a.txt"), "x\n").expect("write failed");
        let original_msg = "feat: original";
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", original_msg])
            .current_dir(repo_dir)
            .status()
            .expect("describe")
            .success());

        assert!(StdCommand::new("jj")
            .args(["new", "-m", "fix(review): orphan test"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new")
            .success());

        let original_cwd = env::current_dir().expect("cwd");
        env::set_current_dir(repo_dir).expect("cd");
        // panic-safe cwd restore
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        try_abandon_empty_fix_commit("test:", None);

        let log_out = StdCommand::new("jj")
            .args([
                "log",
                "-r",
                "::@",
                "--no-graph",
                "-T",
                "description ++ \"\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log");
        let log_str = String::from_utf8_lossy(&log_out.stdout);
        assert!(
            !log_str.contains("fix(review): orphan test"),
            "orphan child が abandon されていない: {:?}",
            log_str
        );
        assert!(
            log_str.contains(original_msg),
            "元 commit が残っていること: {:?}",
            log_str
        );
    }

    #[test]
    fn parse_empty_change_ids_handles_empty_input() {
        assert!(parse_empty_change_ids("").is_empty());
    }

    #[test]
    fn parse_empty_change_ids_extracts_single_id() {
        let out = "abc123def\n";
        assert_eq!(parse_empty_change_ids(out), vec!["abc123def".to_string()]);
    }

    #[test]
    fn parse_empty_change_ids_extracts_multiple_ids() {
        let out = "abc\ndef\nghi\n";
        assert_eq!(
            parse_empty_change_ids(out),
            vec!["abc".to_string(), "def".to_string(), "ghi".to_string()]
        );
    }

    #[test]
    fn parse_empty_change_ids_skips_blank_lines_and_whitespace() {
        let out = "  abc  \n\n   \ndef\n\n";
        assert_eq!(
            parse_empty_change_ids(out),
            vec!["abc".to_string(), "def".to_string()]
        );
    }

    fn setup_jj_repo_with_master_at_base(base_msg: &str) -> tempfile::TempDir {
        use std::process::Command as StdCommand;
        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();
        assert!(StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init")
            .success());
        std::fs::write(repo_dir.join("base.txt"), "content\n").expect("write base");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", base_msg])
            .current_dir(repo_dir)
            .status()
            .expect("describe base")
            .success());
        assert!(StdCommand::new("jj")
            .args(["bookmark", "create", "master", "-r", "@"])
            .current_dir(repo_dir)
            .status()
            .expect("bookmark master")
            .success());
        temp
    }

    struct CwdGuard {
        original: std::path::PathBuf,
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    fn enter_repo(repo_dir: &std::path::Path) -> CwdGuard {
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(repo_dir).expect("cd");
        CwdGuard { original }
    }

    /// `jj new -m <description>` で空 commit を作成する test helper。
    /// integration test での空 commit 列挙を 1 行で書けるようにする。
    fn build_jj_empty_with_description(repo_dir: &std::path::Path, description: &str) {
        let status = std::process::Command::new("jj")
            .args(["new", "-m", description])
            .current_dir(repo_dir)
            .status()
            .expect("jj new");
        assert!(status.success(), "jj new failed for: {}", description);
    }

    /// `master` bookmark を `branch_name` にリネームする test helper。
    /// alternative default_branch test の前処理として利用。
    fn rename_master_bookmark(repo_dir: &std::path::Path, branch_name: &str) {
        let status = std::process::Command::new("jj")
            .args(["bookmark", "rename", "master", branch_name])
            .current_dir(repo_dir)
            .status()
            .expect("jj bookmark rename");
        assert!(status.success(), "rename master -> {}", branch_name);
    }

    /// 指定 description を持つ commit が PR 範囲に残存していることを assert する。
    /// (negative case 用 = abandon されてはいけない sentinel commit の生存確認)
    fn assert_descriptions_present_in_pr_range(
        repo_dir: &std::path::Path,
        default_branch: &str,
        descriptions: &[&str],
    ) {
        let revset = format!("{}..@", default_branch);
        let out = std::process::Command::new("jj")
            .args([
                "log",
                "-r",
                &revset,
                "--no-graph",
                "-T",
                "description ++ \"\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log");
        let log_str = String::from_utf8_lossy(&out.stdout);
        for d in descriptions {
            assert!(
                log_str.contains(d),
                "{:?} が残存している前提だが消えている: {:?}",
                d,
                log_str
            );
        }
    }

    fn assert_descriptions_absent_in_pr_range(
        repo_dir: &std::path::Path,
        descriptions: &[&str],
    ) {
        let out = std::process::Command::new("jj")
            .args([
                "log",
                "-r",
                "master..@",
                "--no-graph",
                "-T",
                "description ++ \"\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log master..@");
        let log_str = String::from_utf8_lossy(&out.stdout);
        for d in descriptions {
            assert!(
                !log_str.contains(d),
                "{:?} が abandon されている前提だが残存: {:?}",
                d,
                log_str
            );
        }
    }

    fn count_empty_in_pr_range(repo_dir: &std::path::Path) -> usize {
        let out = std::process::Command::new("jj")
            .args([
                "log",
                "-r",
                "empty() & (master..@)",
                "--no-graph",
                "-T",
                "change_id ++ \"\\n\"",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("jj log");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }

    /// 統合: `master..@` 範囲に空 commit が無いとき sweep は no-op (非空 commit を保持)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_sweep_empty_commits_no_op_when_no_empty_in_range() {
        let temp = setup_jj_repo_with_master_at_base("feat: real change");
        let repo_dir = temp.path();
        let _guard = enter_repo(repo_dir);

        sweep_empty_commits_in_pr_range("master");

        let log_out = std::process::Command::new("jj")
            .args(["log", "-r", "::@", "--no-graph", "-T", "description"])
            .current_dir(repo_dir)
            .output()
            .expect("jj log");
        let log_str = String::from_utf8_lossy(&log_out.stdout);
        assert!(
            log_str.contains("feat: real change"),
            "non-empty commit が保持されていること: {:?}",
            log_str
        );
    }

    /// 統合: `master..@` 範囲の複数空 commit を sweep が全て abandon する。
    /// PR #174 `kqvluqyv` 事例の最小再現。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_sweep_empty_commits_abandons_multiple_in_range() {
        use std::process::Command as StdCommand;
        let temp = setup_jj_repo_with_master_at_base("feat: base");
        let repo_dir = temp.path();

        for label in &["fix(review): empty 1", "fix(review): empty 2"] {
            build_jj_empty_with_description(repo_dir, label);
        }
        assert!(
            count_empty_in_pr_range(repo_dir) >= 2,
            "前提: sweep 前に空 commit が 2 件以上"
        );

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("master");

        assert_descriptions_absent_in_pr_range(
            repo_dir,
            &["fix(review): empty 1", "fix(review): empty 2"],
        );

        let master_out = StdCommand::new("jj")
            .args(["log", "-r", "master", "--no-graph", "-T", "description"])
            .current_dir(repo_dir)
            .output()
            .expect("jj log master");
        let master_desc = String::from_utf8_lossy(&master_out.stdout);
        assert!(
            master_desc.contains("feat: base"),
            "master commit (非空) は abandon されない: {:?}",
            master_desc
        );
    }

    /// 統合 (PR #194 T2-#2 variant 1): non-`fix(review):` 系の空 commit (`feat:` / `docs:` / `chore:` 等)
    /// は sweep 対象外であることを assert (description filter の negative case)。
    /// fix(review): empty が混在しても誤 abandon されないことが保証される。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_sweep_skips_non_fix_review_empty_commits() {
        let temp = setup_jj_repo_with_master_at_base("feat: base");
        let repo_dir = temp.path();

        build_jj_empty_with_description(repo_dir, "feat: empty 1");
        build_jj_empty_with_description(repo_dir, "docs: empty 2");
        build_jj_empty_with_description(repo_dir, "chore: empty 3");
        build_jj_empty_with_description(repo_dir, "fix(review): empty matched");

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("master");

        assert_descriptions_present_in_pr_range(
            repo_dir,
            "master",
            &["feat: empty 1", "docs: empty 2", "chore: empty 3"],
        );
        assert_descriptions_absent_in_pr_range(repo_dir, &["fix(review): empty matched"]);
    }

    /// 統合 (PR #194 T2-#2 variant 2): default_branch を `main` 等の alternative 名で
    /// 指定したとき、revset がパラメータ化されて該当範囲のみ対象になることを assert。
    /// `SweepConfig.default_branch` 設定可能化の dogfood。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_sweep_respects_alternative_default_branch() {
        let temp = setup_jj_repo_with_master_at_base("feat: base");
        let repo_dir = temp.path();
        rename_master_bookmark(repo_dir, "main");

        build_jj_empty_with_description(repo_dir, "fix(review): empty under main");

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("main");

        assert_descriptions_absent_in_pr_range(repo_dir, &["fix(review): empty under main"]);
    }

    /// 統合 (PR #194 T2-#2 variant 3): `fix(review):` 空 commit が 0 件のとき、
    /// 他 description の空 commit が範囲内に存在しても sweep が abandon を 1 件も発行しない
    /// (description filter の早期 return path)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_sweep_no_op_when_only_non_fix_review_empties_present() {
        let temp = setup_jj_repo_with_master_at_base("feat: base");
        let repo_dir = temp.path();

        build_jj_empty_with_description(repo_dir, "feat: only feat empty");
        build_jj_empty_with_description(repo_dir, "docs: only docs empty");

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("master");

        assert_descriptions_present_in_pr_range(
            repo_dir,
            "master",
            &["feat: only feat empty", "docs: only docs empty"],
        );
    }

    #[test]
    fn fix_commit_state_is_created_truth_table() {
        assert!(!FixCommitState::None.is_created());
        assert!(FixCommitState::Created {
            commit_id: "abc".into()
        }
        .is_created());
    }

    /// 統合: task 6 の再現 — `pnpm push` 後の空 WC の上に fix commit が
    /// 作られた状態で cleanup すると、`@` が stale な空 commit に残らず、
    /// PR tip (bookmark の指す commit) 直下に自動で re-parent されることを確認する。
    ///
    /// 検証対象シナリオ (PR #64 で 3 回発生):
    /// - `C1 (bookmark) ← C1' (empty, from pnpm push) ← Y (fix commit, @)`
    /// - takt が NoChange で Y を abandon した後、従来は `@- == C1'` に残っていた
    /// - 修正後は `@- == C1` (PR tip) に戻る
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_reparents_at_to_pr_tip_after_cleanup() {
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        assert!(StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init 失敗")
            .success());

        // 1. C1: 実コンテンツを持つ commit (PR 本体に相当)
        std::fs::write(repo_dir.join("a.txt"), "content\n").expect("write a.txt 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "feat: PR body"])
            .current_dir(repo_dir)
            .status()
            .expect("describe C1 失敗")
            .success());
        let c1_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log C1");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert!(!c1_id.is_empty());

        // 2. bookmark feat/task6 を C1 に作成 (PR tip として resolve される対象)
        assert!(StdCommand::new("jj")
            .args(["bookmark", "create", "feat/task6", "-r", "@"])
            .current_dir(repo_dir)
            .status()
            .expect("bookmark create 失敗")
            .success());

        // 3. C1': `pnpm push` 相当で @ を空 child に移す
        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new (C1') 失敗")
            .success());

        // 4. cwd を tempdir に切り替え (cli-pr-monitor helpers は cwd 依存)
        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        // 5. Y: fix commit を pre-create (cli-pr-monitor の pre-takt 相当)
        let fix_state = create_fix_commit(Some(64), &[]);
        let fix_cid = match &fix_state {
            FixCommitState::Created { commit_id } => commit_id.clone(),
            _ => panic!("create_fix_commit 失敗: {:?}", fix_state),
        };

        // 6. takt no-op: ファイル変更なし → @ は空 Y のまま

        // 7. cleanup 実行: abandon + reparent
        try_abandon_empty_fix_commit("test:", Some(&fix_cid));

        // 8. 検証: @- は PR tip (C1) と一致する。stale な C1' 上に残っていない。
        let parent_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log @-");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            parent_id, c1_id,
            "@- が PR tip (bookmark feat/task6) の指す commit と一致すること: got={:?}",
            parent_id
        );

        // 9. @ は空 WC (新規作成されたもの)
        assert!(diff_at_is_empty(), "reparent 後の @ は空 WC");

        // 10. bookmark は C1 から動いていない (reparent は bookmark を触らない)
        let bookmark_tip = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "feat/task6", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log bookmark");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            bookmark_tip, c1_id,
            "bookmark が動かされていないこと: got={:?}",
            bookmark_tip
        );
    }

    /// 統合: bookmark が複数ある場合 (stacked PR 等) は reparent をスキップし、
    /// `jj abandon` のデフォルト配置 (親の上に新規 WC) に任せる fail-safe 挙動を確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_try_abandon_skips_reparent_with_multiple_bookmarks() {
        use std::env;
        use std::process::Command as StdCommand;

        let temp = tempfile::tempdir().expect("tempdir 作成失敗");
        let repo_dir = temp.path();

        assert!(StdCommand::new("jj")
            .args(["git", "init"])
            .current_dir(repo_dir)
            .status()
            .expect("jj git init 失敗")
            .success());

        std::fs::write(repo_dir.join("a.txt"), "content\n").expect("write 失敗");
        assert!(StdCommand::new("jj")
            .args(["describe", "-m", "feat: base"])
            .current_dir(repo_dir)
            .status()
            .expect("describe 失敗")
            .success());

        // 複数の非 trunk bookmark を作成 (ambiguous な状態)
        for name in &["feat/stack-a", "feat/stack-b"] {
            assert!(StdCommand::new("jj")
                .args(["bookmark", "create", name, "-r", "@"])
                .current_dir(repo_dir)
                .status()
                .expect("bookmark create 失敗")
                .success());
        }

        // 空 child (pnpm push 相当) を作成
        assert!(StdCommand::new("jj")
            .args(["new"])
            .current_dir(repo_dir)
            .status()
            .expect("jj new 失敗")
            .success());
        let c1_prime_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        let original_cwd = env::current_dir().expect("cwd 取得失敗");
        env::set_current_dir(repo_dir).expect("cd 失敗");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore {
            original: original_cwd,
        };

        let fix_state = create_fix_commit(Some(1), &[]);
        let fix_cid = match &fix_state {
            FixCommitState::Created { commit_id } => commit_id.clone(),
            _ => panic!("create_fix_commit 失敗"),
        };

        try_abandon_empty_fix_commit("test:", Some(&fix_cid));

        // 複数 bookmark なので reparent スキップ。@- は stale な C1' (fix の元親) のまま
        // = jj abandon のデフォルト配置に委ねられる。
        let parent_id = {
            let out = StdCommand::new("jj")
                .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
                .current_dir(repo_dir)
                .output()
                .expect("jj log @-");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            parent_id, c1_prime_id,
            "複数 bookmark 時は reparent スキップ、@- は C1' のまま: got={:?}",
            parent_id
        );
    }
}
