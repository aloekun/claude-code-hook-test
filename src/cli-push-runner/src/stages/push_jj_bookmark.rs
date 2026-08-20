use lib_jj_helpers::{classify_advance_target, is_trunk_bookmark, AdvanceTarget, ADVANCE_TARGET_REVSET};
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

/// advance の移動先: **@ から祖先方向で最も近い、説明のあるコミット** (順位 386)。
///
/// 旧規則 (「@ が非空なら @、空なら @-」) は description を見ないため、監視・自動 fix
/// 経路が積んだ説明なしコミットへ bookmark を移し、push が `Won't push commit ...
/// since it has no description` で失敗した (#370 実観測)。移動先の選定基準を
/// jj の push 拒否条件と同じ軸 (description) に揃える。revset と分類の設計根拠は
/// `lib_jj_helpers::ADVANCE_TARGET_REVSET` の doc を参照。
///
/// I/O は本 crate の timeout 付き wrapper (`run_jj_log`) を使い、分類だけを
/// lib と共有する (subprocess 規律は crate ごと、意味論は 1 箇所)。
fn determine_target_revision() -> Result<Option<String>, String> {
    let raw = run_jj_log(ADVANCE_TARGET_REVSET, "commit_id ++ \"\\n\"")?;
    let ids: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    match classify_advance_target(ids) {
        AdvanceTarget::Commit(id) => Ok(Some(id)),
        AdvanceTarget::Ambiguous(ids) => {
            log_info(&format!(
                "advance 先の候補が複数 (マージ祖先) のため bookmark 自動更新をスキップします: {:?}",
                ids
            ));
            Ok(None)
        }
        AdvanceTarget::None => {
            log_info("説明のあるコミットが @ の祖先に無いため bookmark 自動更新をスキップします");
            Ok(None)
        }
    }
}

/// `@` が空かを返す (bookmark_check の押し止め判定用)。
///
/// 経緯 (T8 / PR #279): かつて advance も「`@` が空なら `@-`」の規則にこの判定を
/// 使っており、bookmark_check と共有することで規則の二重定義を防いでいた。
/// 順位 386 で advance の移動先は description 基準
/// ([`determine_target_revision`]) に変わったため、本判定を使うのは
/// bookmark_check の「空 `@` を push させない」ゲート (PR #280 のレビューバイパス
/// 対策) のみになった。
pub(super) fn working_copy_is_empty() -> Result<bool, String> {
    let output = run_jj_log("@", "if(empty, \"empty\", \"content\")")?;
    Ok(output.trim() == "empty")
}

/// 前進対象の bookmark を返す: **target と同一コミットを指すものだけ** (順位 376)。
///
/// 旧実装は `(trunk()..target) & bookmarks()` で **PR 範囲全体の bookmark** を集めて
/// すべて target へ移していた。スタック push (feat/pr1 ← feat/pr2) では
/// **レビュー済みの feat/pr1 まで feat/pr2 の tip へ動かしてしまい**、
/// 別 PR の範囲に未レビュー変更が混入する (2026-08-06 実観測、gate が止めなければ
/// silent。Severity High)。
///
/// 前進の目的は「takt fix や `jj new` で **自分の** bookmark が置き去りになるのを
/// 直す」ことなので、対象は target が指すコミットの bookmark で必要十分。
/// 使い捨て jj リポジトリでの実測 (jj 0.42):
///
/// | 構成 | 旧 `(trunk()..target)` | 本実装 (target 直接) |
/// |---|---|---|
/// | スタック (pr1=@-, pr2=@) | `feat/pr1, feat/pr2, master` | `feat/pr2` のみ |
/// | 単一ブランチ (solo=@-, @ 空) | `feat/solo, master` | `feat/solo` |
///
/// 単一ブランチ運用の挙動は変わらない (target は既に「@ が空なら @-」で解決済みのため、
/// 置き去りの bookmark はちょうど target 上にある)。
///
/// **trunk 系は本関数が名前で除外する** ([`is_trunk_bookmark`])。旧実装は
/// `trunk()..target` の revset で範囲から外していたが、target を直接照会する形では
/// 範囲による除外が効かない — target が trunk bookmark を指す構成 (bookmark 未作成で
/// 作業を始めた `@`、feature 作成直後で master と同一コミット等) では `master` が
/// 前進対象に入ってしまう (jj 0.42 実測。CodeRabbit #432 指摘)。
///
/// 対象 commit は target と同一なので `jj bookmark set` 自体は実質 no-op だが、
/// 「bookmark 'master' を … に自動更新」というログが出て、trunk を動かしたように
/// 読める。除外層を revset から名前へ移したことを実装で明示する。
fn get_bookmarks_in_range(target: &str) -> Result<Vec<String>, String> {
    let template = "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"";
    match run_jj_log(target, template) {
        Ok(output) => Ok(dedup(
            parse_bookmarks_from_template(&output)
                .into_iter()
                .filter(|b| !is_trunk_bookmark(b))
                .collect(),
        )),
        Err(e) => {
            // (push 自体は続行するので Err を返さず警告に留める)
            log_info(&format!(
                "bookmark の照会に失敗し、bookmark 自動更新をスキップします: {}",
                e
            ));
            Ok(Vec::new())
        }
    }
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
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status = lib_subprocess::wait_with_timeout_basic(error_prefix, &mut child, JJ_TIMEOUT_SECS)
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

/// `@` に説明 (description) があるかを返す (bookmark_check の案内分岐用、順位 386)。
///
/// advance が説明なしコミットへ bookmark を移さなくなった結果、「`@` は非空だが
/// 説明なし」の状態では bookmark が `@` に来ない。この状態を bookmark 不在と
/// 区別しないと `jj bookmark create -r @` (説明なしコミットへの bookmark 付与 =
/// push 不能な bookmark を作る操作) へ誤誘導する — T8 が空コミットで塞いだのと
/// 同型の穴が説明なしコミットで開くため、専用の判定を設ける。
pub(super) fn head_has_description() -> Result<bool, String> {
    let output = run_jj_log("@", "if(description, \"described\", \"descless\")")?;
    Ok(output.trim() == "described")
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

    /// 順位 376 の実 jj 回帰テスト (両方向、2026-08-06 実観測の incident 再現)。
    ///
    /// スタック push (feat/pr1 ← feat/pr2) で、**レビュー済みの feat/pr1 を
    /// feat/pr2 の tip へ前進させない**こと。旧実装は `(trunk()..target) & bookmarks()`
    /// で PR 範囲全体を集めていたため、feat/pr1 が feat/pr2 の tip を指し、別 PR の
    /// 範囲に未レビュー変更が混入していた (gate が止めなければ silent、Severity High)。
    mod advance_scope {
        use super::super::*;
        use std::path::{Path, PathBuf};
        use std::process::Command as StdCommand;

        fn jj(dir: &Path, args: &[&str]) {
            assert!(
                StdCommand::new("jj")
                    .args(args)
                    .current_dir(dir)
                    .status()
                    .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e))
                    .success(),
                "jj {:?} が失敗",
                args
            );
        }

        fn jj_stdout(dir: &Path, args: &[&str]) -> String {
            let out = StdCommand::new("jj")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap_or_else(|e| panic!("jj {:?} 実行失敗: {}", args, e));
            assert!(out.status.success(), "jj {:?} が失敗", args);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        struct CwdRestore {
            original: PathBuf,
        }

        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }

        fn enter(dir: &Path) -> CwdRestore {
            let original = std::env::current_dir().expect("cwd");
            std::env::set_current_dir(dir).expect("cd");
            CwdRestore { original }
        }

        fn commit_of(dir: &Path, revset: &str) -> String {
            jj_stdout(dir, &["log", "-r", revset, "--no-graph", "-T", "commit_id"])
        }

        fn init_base(repo: &Path) {
            std::fs::create_dir_all(repo).unwrap();
            jj(repo, &["git", "init", "--colocate"]);
            std::fs::write(repo.join("a.txt"), "base\n").unwrap();
            jj(repo, &["describe", "-m", "chore: base"]);
            jj(repo, &["bookmark", "create", "master", "-r", "@"]);
        }

        /// スタック構成では **@ の bookmark だけ**が前進対象になること。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn stacked_bookmarks_do_not_advance_ancestors() {
            let tmp = tempfile::tempdir().unwrap();
            let repo = tmp.path().join("r");
            init_base(&repo);
            jj(&repo, &["new", "-m", "feat: pr1"]);
            std::fs::write(repo.join("b.txt"), "one\n").unwrap();
            jj(&repo, &["bookmark", "create", "feat/pr1", "-r", "@"]);
            jj(&repo, &["new", "-m", "feat: pr2"]);
            std::fs::write(repo.join("c.txt"), "two\n").unwrap();
            jj(&repo, &["bookmark", "create", "feat/pr2", "-r", "@"]);

            let pr1_before = commit_of(&repo, "feat/pr1");
            let _guard = enter(&repo);

            let bookmarks = get_bookmarks_in_range("@").expect("照会は成功するはず");
            assert_eq!(
                bookmarks,
                vec!["feat/pr2".to_string()],
                "スタックの祖先 bookmark (feat/pr1) を前進対象に含めてはならない"
            );

            advance_jj_bookmarks().expect("advance は成功するはず");
            assert_eq!(
                commit_of(&repo, "feat/pr1"),
                pr1_before,
                "レビュー済み feat/pr1 が feat/pr2 の tip へ動いてはならない (順位 376)"
            );
        }

        /// CodeRabbit #432 の regression guard: **trunk bookmark を前進対象に含めない**。
        ///
        /// 旧実装は `trunk()..target` の revset で trunk を範囲から外していたが、
        /// target を直接照会する形では範囲による除外が効かない。target が trunk を指す
        /// 構成 (bookmark 未作成で作業を始めた `@`、feature 作成直後で master と同一
        /// コミット等) では `master` が対象に入り、「bookmark 'master' を … に自動更新」
        /// というログが出る (同一 commit なので実害は無いが trunk を動かしたように読める)。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn trunk_bookmark_is_excluded_from_advance_targets() {
            let tmp = tempfile::tempdir().unwrap();
            let repo = tmp.path().join("r");
            init_base(&repo);
            let _guard = enter(&repo);

            assert!(
                get_bookmarks_in_range("@").expect("照会は成功するはず").is_empty(),
                "@ が master を指す構成で master を前進対象にしてはならない"
            );
        }

        /// master と feature が同一コミットにある構成 (bookmark 作成直後) でも、
        /// feature だけが対象になること。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn only_non_trunk_bookmarks_advance_when_sharing_a_commit_with_trunk() {
            let tmp = tempfile::tempdir().unwrap();
            let repo = tmp.path().join("r");
            init_base(&repo);
            jj(&repo, &["new", "-m", "feat: work"]);
            std::fs::write(repo.join("b.txt"), "one\n").unwrap();
            jj(&repo, &["bookmark", "create", "feat/x", "-r", "@"]);
            jj(&repo, &["bookmark", "set", "master", "-r", "@", "--allow-backwards"]);

            let _guard = enter(&repo);
            assert_eq!(
                get_bookmarks_in_range("@").expect("照会は成功するはず"),
                vec!["feat/x".to_string()],
                "trunk と同一コミットでも feature bookmark だけを前進させること"
            );
        }

        /// 対比: 単一ブランチ構成では従来どおり前進すること (効きすぎ防止)。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn single_branch_bookmark_still_advances() {
            let tmp = tempfile::tempdir().unwrap();
            let repo = tmp.path().join("r");
            init_base(&repo);
            jj(&repo, &["new", "-m", "feat: work"]);
            std::fs::write(repo.join("b.txt"), "one\n").unwrap();
            jj(&repo, &["bookmark", "create", "feat/solo", "-r", "@"]);
            // takt fix / 監視経路が @ を進めた状態 (bookmark が置き去り)
            jj(&repo, &["new", "-m", "feat: more work"]);
            std::fs::write(repo.join("c.txt"), "two\n").unwrap();

            let head = commit_of(&repo, "@");
            let solo_before = commit_of(&repo, "feat/solo");
            assert_ne!(solo_before, head, "前提: bookmark は置き去りになっている");

            let _guard = enter(&repo);
            advance_jj_bookmarks().expect("advance は成功するはず");

            assert_eq!(
                commit_of(&repo, "feat/solo"),
                head,
                "単一ブランチ運用では従来どおり @ へ前進すること"
            );
        }
    }
}
