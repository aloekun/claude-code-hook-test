//! transcript の置き場所 (`~/.claude/projects/<project-id>/`) の解決。
//!
//! 読み出しと時刻 range filter は [`super::transcript`] が持つ。

use std::fs;
use std::path::{Path, PathBuf};

/// `cwd` パス → `~/.claude/projects/` の project ID 形式へ変換する。
///
/// Windows: `E:\work\claude-code-hook-test` → `e--work-claude-code-hook-test`
/// (lowercase、`:` `\` `/` をすべて `-` に置換)。
pub fn cwd_to_project_id(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .to_lowercase()
        .replace([':', '\\', '/'], "-")
}

/// `~/.claude/projects/<project-id>/` を返す。`USERPROFILE` 未設定なら `None`。
pub(crate) fn project_transcript_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let projects_root = PathBuf::from(home).join(".claude").join("projects");
    resolve_project_dir(&projects_root, cwd)
}

/// `projects_root` 配下から `cwd` に対応する project-id ディレクトリを探す。
///
/// # なぜ完全一致で `join` しないか (順位 469 完了基準)
///
/// [`cwd_to_project_id`] は比較用に `to_lowercase()` するが、実フォルダ名は元の `cwd` の
/// 大文字小文字をそのまま保存している (例: `C--Users-owner-…-improve`)。Windows は
/// ファイルシステムが case-insensitive なので `join` + `is_dir()` でも偶然一致するが、
/// Linux では一致せずセッションが無言で拾えなくなる。`read_dir` で実在するフォルダ名を
/// 列挙し、lowercase 比較で対応するものを探すことで OS を問わず解決する。
fn resolve_project_dir(projects_root: &Path, cwd: &Path) -> Option<PathBuf> {
    let project_id = cwd_to_project_id(cwd);
    fs::read_dir(projects_root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_lowercase() == project_id)
        })
}

/// このリポジトリの**全 workspace** の transcript ディレクトリと、その workspace root。
///
/// # なぜ cwd 由来の 1 つでは足りないか (順位 469)
///
/// [ADR-045](../../../../docs/adr/adr-045-jj-workspace-parallel-sessions.md) の並列 workspace
/// 運用では、workspace ごとに別の project-id フォルダができる。実装をある workspace で行い
/// 別の workspace から `pnpm merge-pr` すると、**実装セッションが分析入力から丸ごと落ちる**。
///
/// 2026-08-18 の実測: PR #417 は `improve` workspace で実装され (編集 31 件)、main から
/// マージされたため実装セッションが欠落した。**しかも feedback レポートは正常に生成され、
/// 欠落を示す痕跡が何も残らない**。
///
/// # 広げるだけにしない
///
/// フォルダを増やすと無関係なセッションを引き込む危険が裏表で生じる。返り値に workspace
/// root を添えるのは、呼び手が **`cwd` がその root 配下にあること**を必須条件として課せる
/// ようにするため ([ADR-064](../../../../docs/adr/adr-064-monitor-success-positive-evidence.md)
/// の陽性証拠要求)。
///
/// workspace を列挙できない場合 (jj 不在など) は `cwd` 由来の 1 つへフォールバックする。
pub fn workspace_transcript_dirs(cwd: &Path) -> Vec<(PathBuf, PathBuf)> {
    let roots = lib_jj_helpers::list_workspace_roots();
    let roots = if roots.is_empty() {
        vec![cwd.to_path_buf()]
    } else {
        roots
    };
    let mut dirs: Vec<(PathBuf, PathBuf)> = roots
        .into_iter()
        .filter_map(|root| project_transcript_dir(&root).map(|dir| (dir, root)))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "feedback-filter-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn project_id_windows_drive() {
        let p = Path::new("E:\\work\\claude-code-hook-test");
        assert_eq!(cwd_to_project_id(p), "e--work-claude-code-hook-test");
    }

    #[test]
    fn project_id_unix_path() {
        let p = Path::new("/home/user/project");
        assert_eq!(cwd_to_project_id(p), "-home-user-project");
    }

    /// **順位 469 完了基準**: 実フォルダ名が大文字小文字を保存していても
    /// (Linux の case-sensitive filesystem を模した fixture) 解決できること。
    #[test]
    fn resolve_project_dir_matches_case_insensitively() {
        let root = unique_temp_dir("case-insensitive-root");
        let actual_dir_name = "C--Users-owner-Improve";
        fs::create_dir_all(root.join(actual_dir_name)).unwrap();

        let cwd = Path::new("C:\\Users\\owner\\Improve");
        let resolved = resolve_project_dir(&root, cwd);

        assert_eq!(
            resolved,
            Some(root.join(actual_dir_name)),
            "cwd 由来の lowercase project-id と実フォルダ名の大文字小文字が異なっても一致するべき"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_project_dir_returns_none_when_no_match() {
        let root = unique_temp_dir("case-insensitive-no-match");
        fs::create_dir_all(root.join("C--Users-owner-Other")).unwrap();

        let cwd = Path::new("C:\\Users\\owner\\Improve");
        let resolved = resolve_project_dir(&root, cwd);

        assert_eq!(resolved, None, "対応するフォルダが無ければ None を返すべき");

        let _ = fs::remove_dir_all(&root);
    }
}
