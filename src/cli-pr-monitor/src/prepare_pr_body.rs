//! `--prepare-pr-body` / `--prepare-pr-body-cleanup` サブコマンド。
//!
//! 旧 `scripts/prepare-pr-body.ps1` の Rust 移植 (WP-14)。`gh pr create --body-file`
//! に渡す PR body を stdin から受け取り、リポジトリルート (= CWD) の
//! `.tmp-pr-body.md` に BOM なし UTF-8 で書き出してそのパスを stdout に返す。
//! cleanup モードは同ファイルを削除する。prepare-pr スキルが
//! 「本文生成 → prepare-pr-body → create-pr → cleanup」の順で呼ぶ (ADR-028)。
//!
//! ルート基準を CWD にするのは他の cli-pr-monitor 操作 (create-pr / monitor が
//! jj / gh を CWD で実行) と同じ前提。normal と cleanup は同じ基準を使うため、
//! `pnpm prepare-pr-body` / `pnpm prepare-pr-body:cleanup` は常に同じファイルを指す。

use std::io::{self, Read};
use std::path::{Path, PathBuf};

const BODY_FILE_NAME: &str = ".tmp-pr-body.md";

pub(crate) fn run_prepare_pr_body() -> i32 {
    let base = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("prepare-pr-body: current dir の解決に失敗: {e}");
            return 1;
        }
    };

    let mut body = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut body) {
        eprintln!("prepare-pr-body: stdin の読み取りに失敗: {e}");
        return 1;
    }

    match write_body(&base, &body) {
        Ok(path) => {
            println!("{}", path.display());
            0
        }
        Err(msg) => {
            eprintln!("{msg}");
            1
        }
    }
}

pub(crate) fn run_prepare_pr_body_cleanup() -> i32 {
    let base = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("prepare-pr-body: current dir の解決に失敗: {e}");
            return 1;
        }
    };
    match cleanup_body(&base) {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(msg) => {
            eprintln!("{msg}");
            1
        }
    }
}

/// `base/.tmp-pr-body.md` に `body` を BOM なし UTF-8 で書き出し、書き出し先の
/// 絶対パスを返す。body が空 / 空白のみなら `Err` (旧 ps1 の exit 1 経路)。
///
/// 既にあるのが通常のファイルでなければ (シンボリックリンク・ディレクトリなど) 書かずに `Err`。
/// `std::fs::write` はリンクをたどってリンク先を上書きするため、許可リストで確認なしに
/// 実行する前提 (ADR-028 追記 2026-10-28) が崩れる (PR #596 の CodeRabbit 指摘)。通常の
/// ファイルは消してから「既にあれば作らない」形で作るので、確かめてから作るまでの間に
/// リンクを置かれても作成が失敗する。
fn write_body(base: &Path, body: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    if body.trim().is_empty() {
        return Err(
            "prepare-pr-body: stdin is empty or whitespace-only. Pipe PR body content into this command."
                .to_string(),
        );
    }
    let path = base.join(BODY_FILE_NAME);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if !meta.is_file() => {
            return Err(format!(
                "prepare-pr-body: {} が通常のファイルでない (シンボリックリンクなど)。リンク先を上書きしないよう書き出さない",
                path.display()
            ));
        }
        Ok(_) => std::fs::remove_file(&path)
            .map_err(|e| format!("prepare-pr-body: 前回のファイルを消せなかった ({}): {e}", path.display()))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("prepare-pr-body: {} を確かめられなかった: {e}", path.display())),
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .map_err(|e| format!("prepare-pr-body: 書き出しに失敗: {e}"))?;
    Ok(path)
}

/// `base/.tmp-pr-body.md` を削除する。存在すれば削除、無ければ no-op。
/// いずれも旧 ps1 と同じ通知文字列を返す。リンクはたどらずにリンクそのものを消す
/// (`exists()` はリンクをたどるので、リンク先の無いリンクが消えずに残っていた)。
fn cleanup_body(base: &Path) -> Result<String, String> {
    let path = base.join(BODY_FILE_NAME);
    if std::fs::symlink_metadata(&path).is_ok() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("prepare-pr-body: 削除に失敗 ({}): {e}", path.display()))?;
        Ok(format!("cleaned: {}", path.display()))
    } else {
        Ok(format!("nothing to clean: {} not found", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_body_writes_without_bom_and_returns_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_body(dir.path(), "## Summary\n- x\n").unwrap();
        assert_eq!(path, dir.path().join(BODY_FILE_NAME));
        let bytes = std::fs::read(&path).unwrap();
        assert_ne!(&bytes[..bytes.len().min(3)], &[0xEF, 0xBB, 0xBF], "BOM を付けない");
        assert_eq!(String::from_utf8(bytes).unwrap(), "## Summary\n- x\n");
    }

    #[test]
    fn write_body_preserves_utf8_multibyte() {
        let dir = tempfile::tempdir().unwrap();
        let body = "# 概要\n日本語の本文\n";
        let path = write_body(dir.path(), body).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }

    #[test]
    fn write_body_rejects_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_body(dir.path(), "").is_err());
    }

    #[test]
    fn write_body_rejects_whitespace_only() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_body(dir.path(), "  \n\t \n").is_err());
        assert!(
            !dir.path().join(BODY_FILE_NAME).exists(),
            "空白のみ入力ではファイルを作らない"
        );
    }

    #[test]
    fn write_body_replaces_a_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        write_body(dir.path(), "old body that is longer").unwrap();
        let path = write_body(dir.path(), "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn write_body_rejects_a_directory_in_place_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(BODY_FILE_NAME)).unwrap();
        let error = write_body(dir.path(), "body").unwrap_err();
        assert!(error.contains("通常のファイルでない"), "{error}");
        assert!(dir.path().join(BODY_FILE_NAME).is_dir(), "ディレクトリに触らない");
    }

    /// リンク先を上書きしない。Windows ではシンボリックリンクの作成に権限が要るので、
    /// Linux の CI で確かめる (同じ判定はディレクトリのテストが両 OS で通す)。
    #[cfg(unix)]
    #[test]
    fn write_body_rejects_a_symlink_and_keeps_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, "keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join(BODY_FILE_NAME)).unwrap();
        let error = write_body(dir.path(), "body").unwrap_err();
        assert!(error.contains("通常のファイルでない"), "{error}");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_removes_a_dangling_symlink_itself() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join(BODY_FILE_NAME);
        std::os::unix::fs::symlink(dir.path().join("missing"), &link).unwrap();
        let msg = cleanup_body(dir.path()).unwrap();
        assert!(msg.starts_with("cleaned: "), "msg={msg}");
        assert!(std::fs::symlink_metadata(&link).is_err());
    }

    #[test]
    fn cleanup_removes_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        write_body(dir.path(), "body").unwrap();
        let msg = cleanup_body(dir.path()).unwrap();
        assert!(msg.starts_with("cleaned: "), "msg={msg}");
        assert!(!dir.path().join(BODY_FILE_NAME).exists());
    }

    #[test]
    fn cleanup_noop_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let msg = cleanup_body(dir.path()).unwrap();
        assert!(msg.starts_with("nothing to clean: "), "msg={msg}");
    }
}
