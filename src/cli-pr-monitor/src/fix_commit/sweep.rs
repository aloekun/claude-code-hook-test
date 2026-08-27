use crate::log::log_info;
use crate::runner::{run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

/// BLOCK / FAIL で push を中止したとき、ローカルに残った自動生成 fix commit を
/// 列挙して警告する (順位 387、ADR-022「自動化コンポーネントは自分の副作用を後始末する」)。
///
/// **ロールバック (自動 abandon) は採らない。** scope guard の BLOCK は prompt
/// injection の疑い (ADR-054) であり、**fix commit そのものが調査対象の証拠**。
/// 自動で消すと「何が書き換えられようとしたか」を人間が確認できなくなる
/// (ADR-068 の human routing とも整合しない)。gate FAIL 側も、なぜテストが落ちる
/// fix が生成されたかの調査に commit が要る点は同じなので、扱いを揃える。
/// 残置の危険 (#366: 気づかず次作業に混入) は、残っている事実と change_id を
/// **明示的に列挙する**ことで塞ぐ — 台帳の対処案 (a) rollback / (b) 警告 のうち
/// (b) を証拠保全の理由で選んだ形。
///
/// fail-open: 列挙自体が失敗しても push 中止という安全側の決定は変わらないため、
/// warn ログのみで続行する。ただし**沈黙はしない** — 列挙できなかった場合も
/// 「残っている可能性」と手動確認の revset / 手順を出す (CodeRabbit #431 Major:
/// 旧実装は列挙失敗時に無警告で return しており、jj 不調時に本関数の目的
/// = 残置の可視化 が失われていた)。
pub(crate) fn warn_unpushed_fix_commits(default_branch: &str) {
    let commits = match list_unpushed_fix_commits(default_branch) {
        Ok(commits) => commits,
        Err(e) => {
            // **列挙できないときこそ黙ってはいけない** (CodeRabbit #431 Major)。
            // 旧実装は「警告なしで続行」で終わっており、jj 不調時にだけ順位 387 が
            // 塞いだはずの「気づかず次作業に混入する」経路が復活していた。
            // 件数は出せなくても、残っている可能性と手動確認の手順は出せる。
            log_info(&format!(
                "[action] 残置 fix commit の列挙に失敗しました (jj 不調): {}",
                e
            ));
            log_info(&format!(
                "[action] 未 push の自動生成 fix commit がローカルに残っている可能性があります。\
                 `jj log -r '{}..@'` で確認し、不要な `fix(review):` commit は \
                 `jj abandon <change_id>` で破棄してください",
                default_branch
            ));
            return;
        }
    };
    if commits.is_empty() {
        return;
    }
    log_info(&format!(
        "[action] 未 push の自動生成 fix commit がローカルに残っています ({} 件): {}",
        commits.len(),
        commits.join(" / ")
    ));
    log_info(
        "[action] BLOCK/FAIL は自動ロールバックしません (fix commit は調査対象の証拠、ADR-054)。\
         内容を確認し、不要なら `jj abandon <change_id>` で破棄してください",
    );
}

/// `default_branch..@` に残る `fix(review):` commit を "change_id 説明" 形式で列挙する。
/// (I/O は本関数に閉じ、警告文の組み立てと分離して統合テスト可能にする)
pub(crate) fn list_unpushed_fix_commits(default_branch: &str) -> Result<Vec<String>, String> {
    let revset = format!(
        "description(substring:\"fix(review):\") & ({}..@)",
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
            "change_id.short() ++ \" \" ++ description.first_line() ++ \"\\n\"",
        ],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        return Err(out);
    }
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
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
///
/// **本関数は空 commit の後始末専用**で、非空の fix commit を残置警告するのは
/// [`warn_unpushed_fix_commits`] (順位 387) の役割。
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

#[cfg(test)]
mod tests {
    //! jj integration test の不変式パターン (PR #194 T2-#3 codified、
    //! `~/.claude/rules/common/testing.md` § "jj 操作コードの integration test pattern" と対):
    //!
    //!   - NG: `count_empty_in_pr_range(repo_dir) == 0` 等の count-based assert
    //!     → jj は abandon 後に空 WC を自動生成するため、count は意図通り減らず false failure を起こす
    //!   - OK: `assert_descriptions_absent_in_pr_range(repo_dir, default_branch, &[target_desc])` の description-based assert
    //!     → 明示的に投入した description は auto-generated WC と区別できる
    //!   - sentinel 事前投入: 「mutation が発生していない」を assert する場合、本来残るべき commit を
    //!     `assert_descriptions_present_in_pr_range` で生存確認すると no-op vs no-mutation の偽陽性を防げる

    use super::*;

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
                !log_str.contains(d),
                "{:?} が abandon されている前提だが残存: {:?}",
                d,
                log_str
            );
        }
    }

    fn count_empty_in_pr_range(repo_dir: &std::path::Path, default_branch: &str) -> usize {
        let revset = format!("empty() & ({}..@)", default_branch);
        let out = std::process::Command::new("jj")
            .args([
                "log",
                "-r",
                &revset,
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

    /// Companion helper group (`assert_descriptions_absent/present_in_pr_range` /
    /// `count_empty_in_pr_range`) の signature 整合を compile-time で強制する
    /// witness test (順位 193、PR #196 T2-1)。関数ポインタへの cast は型検査のみで
    /// 実行時コストは無いが、signature drift が起きると本 test が compile error で
    /// 落ちる。新しい `*_in_pr_range` helper を group に追加する際は、ここに同型の
    /// cast を 1 行追加すること。
    #[test]
    fn companion_helpers_share_pr_range_signature() {
        let _: fn(&std::path::Path, &str, &[&str]) = assert_descriptions_absent_in_pr_range;
        let _: fn(&std::path::Path, &str, &[&str]) = assert_descriptions_present_in_pr_range;
        let _: fn(&std::path::Path, &str) -> usize = count_empty_in_pr_range;
    }

    /// 統合: PR 範囲 (`<default_branch>..@`) に空 commit が無いとき sweep は no-op (非空 commit を保持)。
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

    /// 統合: PR 範囲 (`<default_branch>..@`) の複数空 commit を sweep が全て abandon する。
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
            count_empty_in_pr_range(repo_dir, "master") >= 2,
            "前提: sweep 前に空 commit が 2 件以上"
        );

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("master");

        assert_descriptions_absent_in_pr_range(
            repo_dir,
            "master",
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
        assert_descriptions_absent_in_pr_range(repo_dir, "master", &["fix(review): empty matched"]);
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

        assert!(
            count_empty_in_pr_range(repo_dir, "main") >= 1,
            "前提: sweep 前に 'main' 範囲で空 commit が 1 件以上 (helper の default_branch 引数が main で機能していること)"
        );

        let _guard = enter_repo(repo_dir);
        sweep_empty_commits_in_pr_range("main");

        assert_descriptions_absent_in_pr_range(
            repo_dir,
            "main",
            &["fix(review): empty under main"],
        );
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
}

#[cfg(test)]
mod leftover_tests {
    use super::*;
    use std::env;
    use std::path::Path;
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

    /// 順位 387 の回帰テスト: BLOCK/FAIL 後に残る fix(review) commit を、警告経路が
    /// 実際に列挙できること (残置が「無警告で埋もれる」退行の検知)。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn lists_leftover_fix_commits_in_pr_range() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        std::fs::create_dir_all(&repo).unwrap();
        jj(&repo, &["git", "init", "--colocate"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        jj(&repo, &["describe", "-m", "chore: base"]);
        jj(&repo, &["bookmark", "create", "master", "-r", "@"]);
        // 自動 fix 経路が作る形: feature commit + fix(review) commit
        jj(&repo, &["new", "-m", "feat: work"]);
        std::fs::write(repo.join("f.txt"), "x\n").unwrap();
        jj(&repo, &["new", "-m", "fix(review): apply CodeRabbit fixes for #999"]);
        std::fs::write(repo.join("g.txt"), "y\n").unwrap();

        let original = env::current_dir().expect("cwd");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore { original };
        env::set_current_dir(&repo).expect("cd");

        let commits = list_unpushed_fix_commits("master").expect("列挙は成功するはず");
        assert_eq!(commits.len(), 1, "fix(review) commit がちょうど 1 件列挙される");
        assert!(
            commits[0].contains("fix(review): apply CodeRabbit fixes for #999"),
            "説明の 1 行目が含まれる: {:?}",
            commits
        );

        // 対照: fix(review) を含まないリポジトリ状態では空 (誤検知しない)
        jj(&repo, &["abandon", "-r", "description(substring:\"fix(review):\")"]);
        let commits = list_unpushed_fix_commits("master").expect("列挙は成功するはず");
        assert!(commits.is_empty(), "fix(review) が無ければ空: {:?}", commits);
    }

    /// CodeRabbit #431 Major の regression guard: **列挙に失敗しても沈黙しない**。
    ///
    /// 旧実装は jj 失敗時に「警告なしで続行」で return しており、jj 不調のときだけ
    /// 順位 387 の目的 (残置の可視化) が失われていた。存在しない revset を渡して
    /// 列挙を失敗させ、`list_unpushed_fix_commits` が `Err` を返すこと
    /// (= fallback 警告の分岐に入ること) を固定する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn enumeration_failure_is_reported_as_error_not_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        std::fs::create_dir_all(&repo).unwrap();
        jj(&repo, &["git", "init", "--colocate"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        jj(&repo, &["describe", "-m", "chore: base"]);

        let original = env::current_dir().expect("cwd");
        struct CwdRestore {
            original: std::path::PathBuf,
        }
        impl Drop for CwdRestore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
            }
        }
        let _guard = CwdRestore { original };
        env::set_current_dir(&repo).expect("cd");

        // 存在しない bookmark を default_branch に渡すと revset 解決が失敗する。
        let result = list_unpushed_fix_commits("no-such-branch-xyz");
        assert!(
            result.is_err(),
            "解決できない revset は Err を返すこと (空 Vec に潰すと警告が出ない): {:?}",
            result
        );

        // 対照: 正常な branch 名なら Ok (fix commit 無しなので空)。
        jj(&repo, &["bookmark", "create", "master", "-r", "@"]);
        let result = list_unpushed_fix_commits("master").expect("正常時は Ok");
        assert!(result.is_empty());
    }
}
