//! jj workspace layout の解釈 — gh 用 `GIT_DIR` 導出とメイン workspace root 解決 (ADR-045)。
//!
//! secondary workspace (`jj workspace add`) は colocated 化されず `.git` を持たないため、
//! gh がリポジトリを解決できない / untracked な状態ファイルが workspace ごとに分裂する。
//! どちらも `.jj` の on-disk layout を辿って解決する。

/// [`resolve_git_dir`] の結果。
pub enum GitDirResolution {
    /// cwd に `.git` が存在する (colocated) — 注入不要
    NotNeeded,
    /// 非 colocated jj workspace — 導出した git dir
    Resolved(std::path::PathBuf),
    /// 導出失敗 (jj リポジトリ外 / layout 不整合 / fs エラー)
    Unresolved(String),
}

/// workspace root から gh 用の git dir を導出する (I/O は fs 読み取りのみ)。
///
/// jj の secondary workspace (`jj workspace add` で作成) は colocated 化されず
/// `.git` を持たないため、gh がリポジトリを解決できない (ADR-045)。
/// jj の on-disk layout を辿って main リポジトリの git dir を求める:
///
/// 1. `<root>/.git` があれば [`GitDirResolution::NotNeeded`]
/// 2. `<root>/.jj/repo` がファイルなら内容が main repo store へのパス
///    (相対なら `<root>/.jj/` 基準)。ディレクトリなら自身が main workspace
/// 3. `<store>/store/git_target` の内容 (相対なら `<store>/store/` 基準) が
///    colocated git dir。git_target が無ければ jj 内部 store の `store/git`
pub fn resolve_git_dir(workspace_root: &std::path::Path) -> GitDirResolution {
    if workspace_root.join(".git").exists() {
        return GitDirResolution::NotNeeded;
    }

    let repo_entry = workspace_root.join(".jj").join("repo");
    let repo_store = if repo_entry.is_file() {
        match std::fs::read_to_string(&repo_entry) {
            Ok(content) => resolve_relative_to(content.trim(), &workspace_root.join(".jj")),
            Err(e) => return GitDirResolution::Unresolved(format!(".jj/repo 読み取り失敗: {}", e)),
        }
    } else if repo_entry.is_dir() {
        repo_entry
    } else {
        return GitDirResolution::Unresolved(
            ".jj/repo が見つかりません (jj リポジトリ外?)".to_string(),
        );
    };

    let store = repo_store.join("store");
    let git_target = store.join("git_target");
    let git_dir = if git_target.is_file() {
        match std::fs::read_to_string(&git_target) {
            Ok(content) => resolve_relative_to(content.trim(), &store),
            Err(e) => {
                return GitDirResolution::Unresolved(format!("git_target 読み取り失敗: {}", e))
            }
        }
    } else {
        store.join("git")
    };

    match git_dir.canonicalize() {
        Ok(p) => GitDirResolution::Resolved(strip_windows_verbatim_prefix(&p)),
        Err(e) => GitDirResolution::Unresolved(format!(
            "導出した git dir が存在しません ({}): {}",
            git_dir.display(),
            e
        )),
    }
}

/// secondary jj workspace から canonical な (メイン) workspace root を解決する (ADR-045 状態分裂対策)。
///
/// `.claude/weekly-review-last-run.json` のような gitignore 済み untracked 状態ファイルは
/// workspace ごとに独立し (per-checkout materialize)、secondary workspace には存在しない。状態を
/// 1 か所 (メイン workspace) に集約するため、`.jj` の on-disk layout からメイン root を導出する。
/// [`resolve_git_dir`] と同じ layout 解釈 (相対パス基準、verbatim prefix 剥がし) を共有する:
///
/// 1. `<root>/.jj/repo` がディレクトリ → この root 自身がメイン (colocated) workspace →
///    `Some(root)` をそのまま返す
/// 2. `<root>/.jj/repo` がファイル → 内容が main repo store への (相対なら `<root>/.jj/` 基準の)
///    パス (`<main>/.jj/repo`)。その 2 階層上がメイン workspace root
/// 3. `.jj/repo` 不在 / 読み取り失敗 / 導出パス不存在 → `None` (caller は現 root に fail-open)
///
/// `GIT_DIR` を扱う [`resolve_git_dir`] と違い最終 store ではなく **workspace root** を返す点、
/// および colocated root を `Resolved` ではなく入力そのまま返す点で用途が異なる。
pub fn resolve_main_workspace_root(workspace_root: &std::path::Path) -> Option<std::path::PathBuf> {
    let repo_entry = workspace_root.join(".jj").join("repo");
    if repo_entry.is_dir() {
        return Some(workspace_root.to_path_buf());
    }
    if !repo_entry.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&repo_entry).ok()?;
    let store = resolve_relative_to(content.trim(), &workspace_root.join(".jj"));
    let main_root = store.parent()?.parent()?;
    match main_root.canonicalize() {
        Ok(p) => Some(strip_windows_verbatim_prefix(&p)),
        Err(_) => None,
    }
}

/// パス文字列を解決する: 絶対ならそのまま、相対なら `base` 基準で連結。
fn resolve_relative_to(path_str: &str, base: &std::path::Path) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(path_str);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

/// Windows の `canonicalize` が付ける verbatim prefix (`\\?\`) を剥がす。
/// git / gh は素のパスで動作し、`\\?\` 付きは外部ツールで問題を起こしやすい。
///
/// ネットワーク共有上のリポジトリでは canonicalize が UNC verbatim 形式
/// (`\\?\UNC\server\share\...`) を返す。ここを一律に剥がすと `UNC\server\share\...`
/// という**存在しないパス**になるため、UNC 形式は `\\server\share\...` へ復元する。
fn strip_windows_verbatim_prefix(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{}", rest));
    }
    match s.strip_prefix(r"\\?\") {
        Some(stripped) => std::path::PathBuf::from(stripped),
        None => p.to_path_buf(),
    }
}

/// 非 colocated jj workspace で `GIT_DIR` を自動注入する (ADR-045 恒久対策候補 1)。
///
/// exe の main() 冒頭で 1 回呼ぶ。プロセス env に設定するため、以降に spawn する
/// gh 子プロセス全体へ伝播する。jj 自身は `GIT_DIR` を無視するため jj 操作には
/// 影響しない (ADR-045 で確認済み)。
///
/// - 既に `GIT_DIR` が設定済み → 尊重して no-op (手動指定・CI 環境を壊さない)
/// - cwd に `.git` がある colocated 環境 → no-op
/// - 導出失敗 → warning ログのみで続行 (fail-soft — colocated では本機能自体が
///   不要であり、失敗時の挙動は従来と同じ「gh が repo 解決に失敗」に留まるため)
pub fn inject_git_dir_for_gh(log_info: fn(&str)) {
    if std::env::var_os("GIT_DIR").is_some() {
        return;
    }
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(_) => return,
    };
    match resolve_git_dir(&cwd) {
        GitDirResolution::NotNeeded => {}
        GitDirResolution::Resolved(git_dir) => {
            std::env::set_var("GIT_DIR", &git_dir);
            log_info(&format!(
                "[env] GIT_DIR 自動注入 (非 colocated jj workspace): {}",
                git_dir.display()
            ));
        }
        GitDirResolution::Unresolved(reason) => {
            log_info(&format!(
                "[env] GIT_DIR 導出失敗 (gh の repo 解決は失敗する可能性): {}",
                reason
            ));
        }
    }
}

/// このリポジトリの全 workspace root を絶対パスで返す (ADR-045 の並列 workspace 運用)。
///
/// `jj workspace list -T 'self.root()'` の出力をそのまま使う。**パスの導出を自前で
/// 組み立てない** — `.jj/repo/workspace_store/index` は内部形式で、jj のバージョン間で
/// 変わりうるため。
///
/// `--ignore-working-copy` で working copy の暗黙 snapshot を回避する (read-only を
/// 意図した helper が snapshot を誘発しないよう、`discover.rs` の
/// `run_jj_workspace_list` と同じ safeguard を適用)。
///
/// 失敗時 (jj 不在 / リポジトリ外 / 非ゼロ終了) は空 `Vec`。呼び手は「列挙できなければ
/// 従来どおり cwd 由来の 1 つだけを使う」ようにフォールバックできる。
pub fn list_workspace_roots() -> Vec<std::path::PathBuf> {
    let output = match std::process::Command::new("jj")
        .args([
            "workspace",
            "list",
            "--ignore-working-copy",
            "-T",
            r#"self.root() ++ "\n""#,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

/// `path` が `root` と同じか、その配下にあるか。**大文字小文字を無視して比較する。**
///
/// # なぜ前方一致か / なぜ case を無視するか (2026-08-18 実測)
///
/// transcript の全 89,625 エントリの `cwd` を集計した結果:
///
/// - サブディレクトリで起動したセッションが実在する (main workspace だけで **26 種類**、
///   `src/...` や `.takt/...` 等)。完全一致にすると 800 件超を落とす
/// - 同一 workspace でもドライブレターの case が揺れる
///   (`C:\Users\...` が 68,312 件、`c:\Users\...` が 5,653 件)。case を見ると取りこぼす
pub fn is_inside_workspace(path: &str, root: &std::path::Path) -> bool {
    let normalize = |s: &str| s.to_lowercase().replace('\\', "/");
    let root = normalize(&root.to_string_lossy());
    let path = normalize(path);
    let Some(rest) = path.strip_prefix(&root) else {
        return false;
    };
    rest.is_empty() || rest.starts_with('/')
}

#[cfg(test)]
mod tests {
    /// verbatim prefix 剥がしは文字列変換のみなので、実 Windows パス無しで固定できる。
    mod verbatim_prefix {
        use super::super::*;
        use std::path::PathBuf;

        #[test]
        fn strips_drive_verbatim_prefix() {
            assert_eq!(
                strip_windows_verbatim_prefix(&PathBuf::from(r"\\?\C:\repo\.git")),
                PathBuf::from(r"C:\repo\.git")
            );
        }

        #[test]
        fn restores_unc_share_path() {
            assert_eq!(
                strip_windows_verbatim_prefix(&PathBuf::from(r"\\?\UNC\server\share\repo\.git")),
                PathBuf::from(r"\\server\share\repo\.git"),
                "UNC verbatim を一律に剥がすと存在しないパス (UNC\\server\\...) になる"
            );
        }

        #[test]
        fn leaves_plain_path_untouched() {
            assert_eq!(
                strip_windows_verbatim_prefix(&PathBuf::from(r"C:\repo\.git")),
                PathBuf::from(r"C:\repo\.git")
            );
            assert_eq!(
                strip_windows_verbatim_prefix(&PathBuf::from("/home/user/repo/.git")),
                PathBuf::from("/home/user/repo/.git")
            );
        }
    }

    /// tempdir に jj の on-disk layout を模擬構築する (jj バイナリ不要の unit test 用)。
    /// 実レイアウトは 2026-07-03 に実機確認: secondary の `.jj/repo` はファイルで
    /// main store への相対パス、colocated main の `store/git_target` は `../../../.git`。
    mod git_dir {
        use super::super::*;
        use std::fs;

        fn make_colocated_main(root: &std::path::Path) {
            fs::create_dir_all(root.join(".git")).unwrap();
            fs::create_dir_all(root.join(".jj/repo/store")).unwrap();
            fs::write(root.join(".jj/repo/store/git_target"), "../../../.git").unwrap();
        }

        fn make_secondary_workspace(ws: &std::path::Path, main_store_rel: &str) {
            fs::create_dir_all(ws.join(".jj")).unwrap();
            fs::write(ws.join(".jj/repo"), main_store_rel).unwrap();
        }

        #[test]
        fn colocated_root_is_not_needed() {
            let tmp = tempfile::tempdir().unwrap();
            make_colocated_main(tmp.path());
            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::NotNeeded
            ));
        }

        #[test]
        fn secondary_workspace_resolves_to_main_git_dir() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            make_secondary_workspace(&ws, "../../main/.jj/repo");

            match resolve_git_dir(&ws) {
                GitDirResolution::Resolved(p) => {
                    let expected = main.join(".git").canonicalize().unwrap();
                    assert_eq!(p.canonicalize().unwrap(), expected);
                    assert!(
                        !p.to_string_lossy().starts_with(r"\\?\"),
                        "verbatim prefix は剥がされていること: {:?}",
                        p
                    );
                }
                other => panic!("Resolved を期待: {:?}", debug_name(&other)),
            }
        }

        #[test]
        fn secondary_workspace_with_absolute_store_path_resolves() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            let abs = main.join(".jj").join("repo");
            make_secondary_workspace(&ws, &abs.to_string_lossy());

            assert!(matches!(
                resolve_git_dir(&ws),
                GitDirResolution::Resolved(_)
            ));
        }

        #[test]
        fn main_workspace_without_git_target_falls_back_to_internal_store() {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join(".jj/repo/store/git")).unwrap();

            match resolve_git_dir(tmp.path()) {
                GitDirResolution::Resolved(p) => {
                    assert!(p.ends_with("git"), "内部 git store を指すこと: {:?}", p);
                }
                other => panic!("Resolved を期待: {:?}", debug_name(&other)),
            }
        }

        #[test]
        fn non_jj_directory_is_unresolved() {
            let tmp = tempfile::tempdir().unwrap();
            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::Unresolved(_)
            ));
        }

        #[test]
        fn dangling_git_target_is_unresolved() {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join(".jj/repo/store")).unwrap();
            fs::write(
                tmp.path().join(".jj/repo/store/git_target"),
                "../../../no-such-dir/.git",
            )
            .unwrap();

            assert!(matches!(
                resolve_git_dir(tmp.path()),
                GitDirResolution::Unresolved(_)
            ));
        }

        fn debug_name(r: &GitDirResolution) -> &'static str {
            match r {
                GitDirResolution::NotNeeded => "NotNeeded",
                GitDirResolution::Resolved(_) => "Resolved",
                GitDirResolution::Unresolved(_) => "Unresolved",
            }
        }

        /// 実 jj で colocated repo + secondary workspace を組み、実レイアウトとの
        /// 齟齬 (jj バージョン更新による layout 変更) を検出する統合テスト。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn real_jj_secondary_workspace_resolves_to_main_git() {
            use std::process::Command as StdCommand;

            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            fs::create_dir_all(&main).unwrap();

            let init_ok = StdCommand::new("jj")
                .args(["git", "init", "--colocate"])
                .current_dir(&main)
                .status()
                .expect("jj git init 実行失敗")
                .success();
            assert!(init_ok, "jj git init --colocate が失敗");

            let ws = tmp.path().join("ws");
            let add_ok = StdCommand::new("jj")
                .args(["workspace", "add", ws.to_string_lossy().as_ref()])
                .current_dir(&main)
                .status()
                .expect("jj workspace add 実行失敗")
                .success();
            assert!(add_ok, "jj workspace add が失敗");

            assert!(
                !ws.join(".git").exists(),
                "secondary workspace は .git を持たない前提 (持つなら本機能は不要になる)"
            );

            match resolve_git_dir(&ws) {
                GitDirResolution::Resolved(p) => {
                    let expected = main.join(".git").canonicalize().unwrap();
                    assert_eq!(p.canonicalize().unwrap(), expected);
                }
                other => panic!("Resolved を期待: {}", debug_name(&other)),
            }
        }
    }

    /// [`resolve_main_workspace_root`] の layout 解釈テスト。fixture は `git_dir` と同型。
    mod main_workspace_root {
        use super::super::*;
        use std::fs;

        fn make_colocated_main(root: &std::path::Path) {
            fs::create_dir_all(root.join(".git")).unwrap();
            fs::create_dir_all(root.join(".jj/repo/store")).unwrap();
        }

        fn make_secondary_workspace(ws: &std::path::Path, main_store: &str) {
            fs::create_dir_all(ws.join(".jj")).unwrap();
            fs::write(ws.join(".jj/repo"), main_store).unwrap();
        }

        #[test]
        fn colocated_main_returns_itself() {
            let tmp = tempfile::tempdir().unwrap();
            make_colocated_main(tmp.path());
            let resolved = resolve_main_workspace_root(tmp.path())
                .expect("colocated main (.jj/repo がディレクトリ) は自身を返す");
            assert_eq!(resolved.as_path(), tmp.path());
        }

        #[test]
        fn secondary_workspace_resolves_to_main_root() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            make_secondary_workspace(&ws, "../../main/.jj/repo");

            let resolved = resolve_main_workspace_root(&ws)
                .expect("secondary の .jj/repo ファイルからメイン root を導出する");
            assert_eq!(
                resolved.canonicalize().unwrap(),
                main.canonicalize().unwrap(),
                "メイン workspace root (store の 2 階層上) を返すこと"
            );
            assert!(
                !resolved.to_string_lossy().starts_with(r"\\?\"),
                "verbatim prefix は剥がされていること: {:?}",
                resolved
            );
        }

        #[test]
        fn secondary_workspace_with_absolute_store_path_resolves() {
            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            let ws = tmp.path().join("ws");
            make_colocated_main(&main);
            let abs = main.join(".jj").join("repo");
            make_secondary_workspace(&ws, &abs.to_string_lossy());

            let resolved = resolve_main_workspace_root(&ws)
                .expect("絶対パス store でもメイン root を導出する");
            assert_eq!(
                resolved.canonicalize().unwrap(),
                main.canonicalize().unwrap()
            );
        }

        #[test]
        fn non_jj_directory_is_none() {
            let tmp = tempfile::tempdir().unwrap();
            assert!(
                resolve_main_workspace_root(tmp.path()).is_none(),
                ".jj 不在は None (caller は現 root に fail-open)"
            );
        }

        /// 実 jj で colocated main + secondary workspace を組み、実レイアウトとの齟齬を検出する。
        #[test]
        #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
        fn real_jj_secondary_workspace_resolves_to_main_root() {
            use std::process::Command as StdCommand;

            let tmp = tempfile::tempdir().unwrap();
            let main = tmp.path().join("main");
            fs::create_dir_all(&main).unwrap();

            let init_ok = StdCommand::new("jj")
                .args(["git", "init", "--colocate"])
                .current_dir(&main)
                .status()
                .expect("jj git init 実行失敗")
                .success();
            assert!(init_ok, "jj git init --colocate が失敗");

            let ws = tmp.path().join("ws");
            let add_ok = StdCommand::new("jj")
                .args(["workspace", "add", ws.to_string_lossy().as_ref()])
                .current_dir(&main)
                .status()
                .expect("jj workspace add 実行失敗")
                .success();
            assert!(add_ok, "jj workspace add が失敗");

            let resolved = resolve_main_workspace_root(&ws)
                .expect("実 jj secondary workspace からメイン root を導出する");
            assert_eq!(
                resolved.canonicalize().unwrap(),
                main.canonicalize().unwrap(),
                "secondary はメイン workspace root を返す"
            );

            let main_resolved =
                resolve_main_workspace_root(&main).expect("colocated main は自身を返す");
            assert_eq!(
                main_resolved.canonicalize().unwrap(),
                main.canonicalize().unwrap(),
                "colocated main は自身の root を返す"
            );
        }
    }

    mod inside_workspace {
        use super::super::is_inside_workspace;
        use std::path::Path;

        #[test]
        fn accepts_the_root_itself() {
            let root = Path::new(r"C:\Users\owner\work\repo");
            assert!(is_inside_workspace(r"C:\Users\owner\work\repo", root));
        }

        /// サブディレクトリで起動したセッションを落とさない (実測で main だけで 26 種類)。
        #[test]
        fn accepts_subdirectories() {
            let root = Path::new(r"C:\Users\owner\work\repo");
            assert!(is_inside_workspace(r"C:\Users\owner\work\repo\src", root));
            assert!(is_inside_workspace(
                r"C:\Users\owner\work\repo\.takt\runs\x",
                root
            ));
        }

        /// ドライブレターの case 揺れを吸収する (実測で `C:` と `c:` が混在)。
        #[test]
        fn ignores_case_differences() {
            let root = Path::new(r"C:\Users\owner\work\repo");
            assert!(is_inside_workspace(r"c:\users\owner\work\repo\src", root));
        }

        /// **名前が前方一致するだけの別 workspace を受理しない。**
        ///
        /// `repo` と `repo-improve` は文字列としては前方一致するが別の workspace。
        /// 区切りまで見ないと、secondary workspace のセッションを main のものとして
        /// 取り込んでしまう。
        #[test]
        fn rejects_sibling_workspace_with_a_shared_prefix() {
            let root = Path::new(r"C:\Users\owner\work\repo");
            assert!(!is_inside_workspace(
                r"C:\Users\owner\work\repo-improve",
                root
            ));
            assert!(!is_inside_workspace(
                r"C:\Users\owner\work\repo-improve\src",
                root
            ));
        }

        #[test]
        fn rejects_unrelated_paths() {
            let root = Path::new(r"C:\Users\owner\work\repo");
            assert!(!is_inside_workspace(r"C:\Users\owner\work\other", root));
            assert!(!is_inside_workspace("", root));
        }
    }
}
