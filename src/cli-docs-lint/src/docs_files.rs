//! docs_files — `docs/` 配下の TODO 系ファイルを列挙する共有層。
//!
//! # 由来
//!
//! 順位 table ファイルの name prefix (`todo-summary`) が **3 箇所で独立に定義**され、
//! 列挙処理も 4 箇所へ写経されていた (`entry_pairing` の summary / detail、
//! `priority_inversion` の summary、`preamble` の todo)。定義が割れると
//! 「validator A は `todo-summary3.md` を読むが B は読まない」という**片側だけの
//! 追従漏れ**が起きる — 台帳分割はこの先も繰り返す操作なので、そのたびに全 validator
//! を手で揃える運用は破れる (defect-convergence-plan.md § Phase F の F1)。
//!
//! **列挙を 1 箇所に集めることで、追従漏れの起きる場所そのものを無くす**
//! ([ADR-042](../../../docs/adr/adr-042-rule-vs-mechanism-boundary.md): 人間に同期の
//! 義務を課すのではなく、同期が要らない形にする)。
//!
//! # fail-closed
//!
//! 走査中の entry エラーは**握り潰さず伝播する**。統合前の `priority_inversion` /
//! `preamble` は `read_dir(..).flatten()` で entry エラーを黙って捨てており、
//! 読めないファイルが 1 件あると検査対象から静かに外れて false-green になっていた
//! ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。

use std::fs;
use std::path::{Path, PathBuf};

/// 順位 table ファイルの name prefix。
///
/// 分割された index の全 part (`todo-summary.md` / `todo-summary2.md` / 将来の
/// `todo-summary3.md`) にマッチする。**この 1 箇所が唯一の定義**であり、
/// `entry_pairing` / `priority_inversion` / `preamble` はここを参照する。
pub const SUMMARY_FILE_PREFIX: &str = "todo-summary";

/// ファイル名が順位 table ファイルか (I/O なし)。
pub fn is_summary_file_name(name: &str) -> bool {
    name.starts_with(SUMMARY_FILE_PREFIX) && name.ends_with(".md")
}

/// ファイル名が TODO 系 markdown か (I/O なし)。順位 table (`todo-summary*.md`) も含む。
pub fn is_todo_file_name(name: &str) -> bool {
    name.starts_with("todo") && name.ends_with(".md")
}

/// `docs_dir` 直下のファイルのうち `accept` が真を返すものを name 順に列挙する。
///
/// name 順にするのは violation の出力順を安定させるため (分割 part をまたぐ検査で
/// 報告順が環境依存になると diff が読めない)。
pub fn list_docs_files(
    docs_dir: &Path,
    accept: impl Fn(&str) -> bool,
) -> Result<Vec<PathBuf>, String> {
    let dir = fs::read_dir(docs_dir)
        .map_err(|e| format!("docs ディレクトリを読めません ({}): {e}", docs_dir.display()))?;
    let mut paths = Vec::new();
    for entry in dir {
        let entry = entry.map_err(|e| format!("docs ディレクトリの走査に失敗しました: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if accept(name) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// `docs_dir` 直下の順位 table ファイル (`todo-summary*.md`) を name 順に列挙する。
pub fn list_summary_files(docs_dir: &Path) -> Result<Vec<PathBuf>, String> {
    list_docs_files(docs_dir, is_summary_file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn summary_prefix_matches_every_split_part() {
        assert!(is_summary_file_name("todo-summary.md"));
        assert!(is_summary_file_name("todo-summary2.md"));
        assert!(is_summary_file_name("todo-summary3.md"));
        assert!(!is_summary_file_name("todo.md"));
        assert!(!is_summary_file_name("todo-summary.md.bak"));
    }

    #[test]
    fn todo_predicate_includes_summary_parts() {
        assert!(is_todo_file_name("todo.md"));
        assert!(is_todo_file_name("todo25.md"));
        assert!(is_todo_file_name("todo-summary2.md"));
        assert!(!is_todo_file_name("adr-033.md"));
    }

    #[test]
    fn listing_is_name_sorted_and_filtered() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for name in ["todo-summary3.md", "todo-summary.md", "todo1.md", "notes.md"] {
            std::fs::write(tmp.path().join(name), "").expect("write");
        }
        assert_eq!(
            names(&list_summary_files(tmp.path()).unwrap()),
            vec!["todo-summary.md", "todo-summary3.md"]
        );
    }

    /// ディレクトリを開けない場合は Err (fail-closed)。空 Vec で素通りしない。
    #[test]
    fn an_unreadable_docs_dir_is_an_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("no-such-dir");
        let err = list_summary_files(&missing).unwrap_err();
        assert!(err.contains("docs ディレクトリを読めません"), "{err}");
    }

    /// サブディレクトリが名前条件に合致しても列挙しない (`todo-summary` という名の dir 等)。
    #[test]
    fn directories_are_not_listed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tmp.path().join("todo-summary9.md")).expect("mkdir");
        std::fs::write(tmp.path().join("todo-summary.md"), "").expect("write");
        assert_eq!(names(&list_summary_files(tmp.path()).unwrap()), vec!["todo-summary.md"]);
    }
}
