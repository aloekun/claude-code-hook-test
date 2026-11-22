//! push 済みのコミットへの編集を、Edit / Write の前で止める (順位 611)。
//!
//! `pnpm push` の後も、作業コピー (`@`) は push したコミットのまま残る。そのまま編集すると、jj は変更を
//! push 済みのコミットに取り込み、次の push で PR のコミットが書き換わる (2026-11-13 の #636 で、
//! レビュースレッドの返信で示したコミットが PR から消えかけた)。[ADR-045] の「ファイル編集を始める前に
//! `jj new` する」は文章の規約なので、push の直後に続けて作業する場面で漏れた。
//!
//! # 判定
//!
//! 編集するファイルのパスから `.jj` を遡ってワークスペースを見つけ、`@` の change を remote の
//! bookmark が指しているかを jj に 1 回だけ問い合わせる。change で比べるので、push の直後
//! (`@` = push したコミット) だけでなく、push 後に書き換えてまだ push していない状態も止める。
//! `jj new` すれば `@` は別の change になり、止まらない。
//!
//! push 済みのコミットを意図して直すときは、`jj new -m "wip: <内容>"` → 編集 → `jj squash -u` で
//! 取り込む。`jj squash` は Bash で走るので、この guard は止めない。案内する `jj new` に `-m` を
//! 付けるのは、`jj-message-required` preset が `-m` の無い `jj new` を止めるため (PR #660 の
//! CodeRabbit の指摘)。
//!
//! `--ignore-working-copy` で問い合わせる。snapshot を取らないので速く (実測 約 60ms)、
//! 編集前の検査が作業コピーの状態を変えない。snapshot していない編集があっても、`@` の change は
//! 変わらないので判定は同じになる。
//!
//! # 方式の選択 (2026-11-22 実測、jj 0.42)
//!
//! `immutable_heads()` に `remote_bookmarks(remote=origin)` を足す案も実測した。push した時点で jj が
//! 空の子コミットを作るので漏れは無いが、push 済みの PR ブランチの `jj rebase -b` / `jj squash` の
//! たびに `--ignore-immutable` が要り、設定が追跡されない `.jj/repo/config.toml` に入るので clone や
//! cloud ごとに配る仕組みも要る。ユーザー判断で hook を選んだ。Bash 経由の編集 (`sed -i` など) は
//! 止めない。
//!
//! # fail-open
//!
//! jj が無い・失敗した・時間切れのときは編集を通す。品質ゲート ([ADR-043]) ではなく作業の流れを守る
//! 仕組みで、jj の不調ですべての編集が止まる方が害が大きいため。
//!
//! [ADR-045]: ../../../docs/adr/adr-045-jj-workspace-parallel-sessions.md
//! [ADR-043]: ../../../docs/adr/adr-043-security-gates-fail-closed.md

use std::path::{Path, PathBuf};

use crate::config::{PushedCommitGuardConfig, PUSHED_COMMIT_GUARD_DEFAULT_REMOTE, PUSHED_COMMIT_GUARD_JJ_TIMEOUT_SECS};

/// jj の出力の 1 行を区切る文字。change id と bookmark 名には現れない。
const FIELD_SEPARATOR: char = '\t';
/// `@` の行に付ける印。
const WORKING_COPY_MARK: &str = "@";

/// `@` と、`remote` の bookmark が指す (trunk ではない) コミットを 1 行ずつ出す jj の引数。
///
/// 1 行は `change id \t @の印 \t remote bookmark (空白区切り)`。push 後に `@` を書き換えると、
/// remote bookmark は書き換え前のコミット (同じ change) を指したまま残るので、その行も出る。
fn query_args(root: &Path, remote: &str) -> Vec<String> {
    let revset = format!("@ | (remote_bookmarks(remote=exact:\"{remote}\") & mutable())");
    let template = format!(
        r#"change_id ++ "{FIELD_SEPARATOR}" ++ if(current_working_copy, "{WORKING_COPY_MARK}") ++ "{FIELD_SEPARATOR}" ++ remote_bookmarks ++ "\n""#
    );
    [
        "-R",
        &root.to_string_lossy(),
        "--ignore-working-copy",
        "--no-pager",
        "--color=never",
        "log",
        "--no-graph",
        "-r",
        &revset,
        "-T",
        &template,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// [`query_args`] の出力から、`@` と同じ change を指す `remote` の bookmark を返す。
///
/// テンプレートの `remote_bookmarks` は remote を絞らずに出す。colocate のリポジトリでは
/// ローカルの bookmark ごとに `<name>@git` が付くので、`@<remote>` で終わる名前だけを数える。
/// 数えないと、bookmark を付けただけの未 push の `@` まで止める (2026-11-22、本 guard の
/// 最初の push で実際に止まった)。
///
/// `@` の行が無い・形が崩れている、のときは空 (fail-open。モジュールの doc を参照)。
pub(crate) fn pushed_bookmarks_of_working_copy(output: &str, remote: &str) -> Vec<String> {
    let remote_suffix = format!("@{remote}");
    let rows: Vec<[&str; 3]> = output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, FIELD_SEPARATOR);
            Some([fields.next()?, fields.next()?, fields.next()?])
        })
        .collect();
    let Some(working_copy_change) = rows
        .iter()
        .find(|[_, mark, _]| *mark == WORKING_COPY_MARK)
        .map(|[change, _, _]| *change)
    else {
        return Vec::new();
    };
    let mut bookmarks: Vec<String> = rows
        .iter()
        .filter(|[change, _, _]| *change == working_copy_change)
        .flat_map(|[_, _, names]| names.split_whitespace())
        .filter(|name| name.ends_with(&remote_suffix))
        .map(str::to_string)
        .collect();
    bookmarks.sort();
    bookmarks.dedup();
    bookmarks
}

/// `file_path` を含む jj のワークスペースの root (`.jj` のあるディレクトリ)。無ければ None。
///
/// 新しく作るファイルはまだ無いので、パスの文字列だけで親を遡る。
pub(crate) fn find_workspace_root(file_path: &Path) -> Option<PathBuf> {
    file_path
        .ancestors()
        .skip(1)
        .find(|dir| dir.join(".jj").is_dir())
        .map(Path::to_path_buf)
}

/// guard を止めるときの理由。
pub(crate) fn build_message(bookmarks: &[String]) -> String {
    let names = bookmarks.iter().map(|b| format!("`{b}`")).collect::<Vec<_>>().join(", ");
    format!(
        "**push 済みのコミットへの編集がブロックされました**\n\n\
         作業コピー (@) は push 済みのコミット ({names}) です。このまま編集すると、jj は変更をそのコミットに\n\
         取り込み、次の push で PR のコミットが書き換わります (#636 で、レビューの返信で示したコミットが\n\
         PR から消えかけました)。\n\n\
         **代わりに:** `{SUGGESTED_JJ_NEW}` してから編集してください。続きの作業は新しいコミットとして push します。\n\n\
         push 済みのコミットを意図して直すときは、`{SUGGESTED_JJ_NEW}` → 編集 → `jj squash -u` で取り込みます\n\
         (ADR-045 § ファイル編集を始める前に jj new する)。"
    )
}

/// 案内する `jj new`。`jj-message-required` preset に止められないよう `-m` を付ける。
const SUGGESTED_JJ_NEW: &str = r#"jj new -m "wip: <内容>""#;

/// 編集を止めるなら理由を返す。設定で有効にしていなければ何もしない (ADR-039、既定 OFF)。
pub(crate) fn check_pushed_commit(file_path: &str, config: &PushedCommitGuardConfig) -> Option<String> {
    check_pushed_commit_with(file_path, config, run_jj)
}

/// [`check_pushed_commit`] の本体。`jj` は引数を受け取り、成功したときの stdout を返す。
pub(crate) fn check_pushed_commit_with(
    file_path: &str,
    config: &PushedCommitGuardConfig,
    jj: impl Fn(&[String]) -> Option<String>,
) -> Option<String> {
    if !config.enabled.unwrap_or(false) || file_path.is_empty() {
        return None;
    }
    let root = find_workspace_root(Path::new(file_path))?;
    let remote = config.remote.as_deref().unwrap_or(PUSHED_COMMIT_GUARD_DEFAULT_REMOTE);
    let output = jj(&query_args(&root, remote))?;
    let bookmarks = pushed_bookmarks_of_working_copy(&output, remote);
    (!bookmarks.is_empty()).then(|| build_message(&bookmarks))
}

fn run_jj(args: &[String]) -> Option<String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let capture = lib_subprocess::run_cmd_direct_capture("jj", &args, PUSHED_COMMIT_GUARD_JJ_TIMEOUT_SECS);
    capture.ok.then_some(capture.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> PushedCommitGuardConfig {
        PushedCommitGuardConfig { enabled: Some(true), remote: None }
    }

    fn row(change: &str, working_copy: bool, bookmarks: &str) -> String {
        let mark = if working_copy { WORKING_COPY_MARK } else { "" };
        format!("{change}{FIELD_SEPARATOR}{mark}{FIELD_SEPARATOR}{bookmarks}\n")
    }

    #[test]
    fn the_working_copy_at_a_remote_bookmark_is_pushed() {
        let out = row("kkqv", true, "feat@origin");
        assert_eq!(pushed_bookmarks_of_working_copy(&out, "origin"), vec!["feat@origin"]);
    }

    /// push 後に `@` を書き換えると、remote bookmark は書き換え前の同じ change のコミットに残る。
    #[test]
    fn a_rewritten_working_copy_of_the_same_change_is_pushed() {
        let out = row("kkqv", true, "") + &row("kkqv", false, "feat@origin") + &row("zzzz", false, "other@origin");
        assert_eq!(pushed_bookmarks_of_working_copy(&out, "origin"), vec!["feat@origin"]);
    }

    #[test]
    fn a_new_change_on_top_of_the_pushed_commit_is_not_pushed() {
        let out = row("mxvq", true, "") + &row("kkqv", false, "feat@origin");
        assert!(pushed_bookmarks_of_working_copy(&out, "origin").is_empty());
    }

    #[test]
    fn several_bookmarks_on_the_change_are_all_named_once() {
        let out = row("kkqv", true, "b@origin a@origin") + &row("kkqv", false, "a@origin");
        assert_eq!(pushed_bookmarks_of_working_copy(&out, "origin"), vec!["a@origin", "b@origin"]);
    }

    /// `@` の行が無い・崩れた出力は止めない (fail-open)。
    #[test]
    fn output_without_the_working_copy_row_is_not_pushed() {
        assert!(pushed_bookmarks_of_working_copy("", "origin").is_empty());
        assert!(pushed_bookmarks_of_working_copy("garbage\n", "origin").is_empty());
        assert!(pushed_bookmarks_of_working_copy(&row("kkqv", false, "feat@origin"), "origin").is_empty());
    }

    /// colocate のリポジトリでは、未 push のローカル bookmark にも `@git` が付く。設定した remote 以外は
    /// 数えない (2026-11-22、本 guard の最初の push で、未 push の `@` を止めた誤検知)。
    #[test]
    fn bookmarks_of_other_remotes_are_not_pushed() {
        let out = row("kkqv", true, "feat@git");
        assert!(pushed_bookmarks_of_working_copy(&out, "origin").is_empty());
        let both = row("kkqv", true, "feat@git feat@origin");
        assert_eq!(pushed_bookmarks_of_working_copy(&both, "origin"), vec!["feat@origin"]);
        assert!(pushed_bookmarks_of_working_copy(&both, "upstream").is_empty());
    }

    #[test]
    fn finds_the_workspace_root_from_a_file_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".jj")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let file = dir.path().join("src").join("new").join("x.rs");
        assert_eq!(find_workspace_root(&file).as_deref(), Some(dir.path()));
    }

    #[test]
    fn a_file_outside_any_workspace_has_no_root() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_workspace_root(&dir.path().join("a.txt")), None);
    }

    fn workspace_file() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".jj")).unwrap();
        let file = dir.path().join("a.rs").to_string_lossy().into_owned();
        (dir, file)
    }

    #[test]
    fn blocks_with_the_bookmark_name_when_the_working_copy_is_pushed() {
        let (_dir, file) = workspace_file();
        let message = check_pushed_commit_with(&file, &enabled(), |_| Some(row("kkqv", true, "feat@origin"))).unwrap();
        assert!(message.contains("`feat@origin`"));
        assert!(message.contains(&format!("`{SUGGESTED_JJ_NEW}`")));
    }

    /// 案内に書いた `jj ...` のコマンドは、このリポジトリで有効な jj の preset に止められない。
    /// 案内どおりに打つと別の preset に止められる、を防ぐ (PR #660 の CodeRabbit の指摘:
    /// `-m` の無い `jj new` を案内していた)。
    #[test]
    fn the_commands_in_the_message_pass_the_jj_presets() {
        use crate::blocked_patterns::{build_blocked_patterns, validate_command};
        use crate::config::{Config, PreToolValidateConfig};
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(
                    ["jj-immutable", "jj-main-guard", "jj-message-required"].iter().map(|s| s.to_string()).collect(),
                ),
                extra_protected_files: None,
                todo_staleness: None,
                allowed_commands: None,
                pushed_commit_guard: None,
            }),
        };
        let patterns = build_blocked_patterns(&config);
        let message = build_message(&["feat@origin".to_string()]);
        let commands: Vec<&str> = message.split('`').skip(1).step_by(2).filter(|span| span.starts_with("jj ")).collect();
        assert!(commands.contains(&SUGGESTED_JJ_NEW), "{commands:?}");
        assert!(commands.contains(&"jj squash -u"), "{commands:?}");
        for command in commands {
            assert!(validate_command(command, &patterns).is_none(), "案内のコマンドが preset に止められる: {command}");
        }
    }

    #[test]
    fn does_nothing_unless_enabled() {
        let (_dir, file) = workspace_file();
        let pushed = |_: &[String]| Some(row("kkqv", true, "feat@origin"));
        for config in [
            PushedCommitGuardConfig { enabled: None, remote: None },
            PushedCommitGuardConfig { enabled: Some(false), remote: None },
        ] {
            assert_eq!(check_pushed_commit_with(&file, &config, pushed), None);
        }
    }

    /// jj が失敗したら止めない (fail-open)。
    #[test]
    fn a_failed_jj_query_lets_the_edit_through() {
        let (_dir, file) = workspace_file();
        assert_eq!(check_pushed_commit_with(&file, &enabled(), |_| None), None);
    }

    #[test]
    fn a_file_outside_any_workspace_does_not_query_jj() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt").to_string_lossy().into_owned();
        let called = std::cell::Cell::new(false);
        let result = check_pushed_commit_with(&file, &enabled(), |_| {
            called.set(true);
            Some(row("kkqv", true, "feat@origin"))
        });
        assert_eq!(result, None);
        assert!(!called.get());
    }

    #[test]
    fn queries_the_configured_remote_in_the_files_workspace() {
        let (dir, file) = workspace_file();
        let config = PushedCommitGuardConfig { enabled: Some(true), remote: Some("upstream".to_string()) };
        let seen = std::cell::RefCell::new(Vec::new());
        check_pushed_commit_with(&file, &config, |args| {
            *seen.borrow_mut() = args.to_vec();
            None
        });
        let args = seen.into_inner();
        assert_eq!(args[..2], ["-R".to_string(), dir.path().to_string_lossy().into_owned()]);
        assert!(args.contains(&"--ignore-working-copy".to_string()));
        assert!(args.iter().any(|a| a.contains(r#"remote_bookmarks(remote=exact:"upstream")"#)));
    }

    /// 実際の jj で、push 直後・push 後の書き換え・`jj new` の後を確かめる (`cargo test -- --ignored`)。
    #[test]
    #[ignore = "jj を起動する統合テスト"]
    fn real_jj_blocks_only_while_the_working_copy_is_the_pushed_change() {
        const JJ_SETUP_TIMEOUT_SECS: u64 = 30;
        let base = tempfile::tempdir().unwrap();
        let remote = base.path().join("remote");
        let repo = base.path().join("repo");
        let jj = |args: &[&str]| {
            let capture = lib_subprocess::run_cmd_direct_capture("jj", args, JJ_SETUP_TIMEOUT_SECS);
            assert!(capture.ok, "jj {args:?} failed: {}", capture.stderr);
        };
        let repo_s = repo.to_string_lossy().into_owned();
        let remote_git = remote.join(".git").to_string_lossy().into_owned();
        jj(&["git", "init", "--colocate", &remote.to_string_lossy()]);
        jj(&["git", "init", "--colocate", &repo_s]);
        jj(&["-R", &repo_s, "config", "set", "--repo", "user.name", "t"]);
        jj(&["-R", &repo_s, "config", "set", "--repo", "user.email", "t@example.com"]);
        jj(&["-R", &repo_s, "git", "remote", "add", "origin", &remote_git]);
        let file = repo.join("a.txt");
        std::fs::write(&file, "1\n").unwrap();
        jj(&["-R", &repo_s, "describe", "-m", "feat"]);
        jj(&["-R", &repo_s, "bookmark", "create", "feat", "-r", "@"]);
        let file_s = file.to_string_lossy().into_owned();
        assert_eq!(check_pushed_commit(&file_s, &enabled()), None, "bookmark はあるが未 push (colocate の @git だけ)");
        jj(&["-R", &repo_s, "git", "push", "-b", "feat"]);

        let after_push = check_pushed_commit(&file_s, &enabled());
        assert!(after_push.as_deref().is_some_and(|m| m.contains("`feat@origin`")), "push 直後: {after_push:?}");

        std::fs::write(&file, "2\n").unwrap();
        jj(&["-R", &repo_s, "status"]);
        let after_rewrite = check_pushed_commit(&file_s, &enabled());
        assert!(after_rewrite.is_some(), "push 後に書き換えた @");

        jj(&["-R", &repo_s, "new"]);
        assert_eq!(check_pushed_commit(&file_s, &enabled()), None, "jj new の後");
    }
}
