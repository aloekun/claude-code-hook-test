//! hook が受け取るファイルパスを、config の glob と同じ土俵 (リポジトリ相対) へ揃える層。
//!
//! # なぜ必要か
//!
//! `.claude/hooks-config.toml` の `paths` や `.claude/custom-lint-rules.toml` の `paths` は
//! **リポジトリ相対の glob** (`docs/**/*.md` / `.takt/workflows/*.yaml`) で書く。一方
//! Claude Code が PostToolUse hook に渡す `tool_input.file_path` は **絶対パス**
//! (`C:/Users/.../repo/docs/guide.md`) である。
//!
//! 両者をそのまま照合すると `docs/` 始まりの glob は `C:/` 始まりの文字列に一致せず、
//! **フィルタが常に不一致 = 検査が無言で no-op になる**。2026-08-13 の調査で、
//! `file_size_check` (50KB 閾値) と `paths` 付き custom rule の両方がこの理由で一度も
//! 発火していなかったことが実測で判明した (docs 配下に 50KB 超が 6 件蓄積していた)。
//!
//! # 正規化の方針
//!
//! 現在の作業ディレクトリ (hook はリポジトリ root で起動される) を prefix として剥がす。
//! 剥がせない場合は**元の文字列をそのまま返す** — 判定を「マッチしない」側へ倒すことで、
//! リポジトリ外のファイルを誤って検査対象に含めない (fail-safe)。

/// 区切りを `/` に揃え、可能なら現在の作業ディレクトリからの相対パスへ変換する。
///
/// Windows ではドライブレターの大小が経路によって揺れる (`C:` / `c:`) ため、prefix 比較は
/// 大小無視で行う。返す値は元の文字列から切り出すので、パス自体の大小は保たれる。
pub(crate) fn to_repo_relative(file: &str) -> String {
    let normalized = file.replace('\\', "/");
    let Ok(cwd) = std::env::current_dir() else {
        return normalized;
    };
    let root = cwd.to_string_lossy().replace('\\', "/");
    let root = root.strip_suffix('/').unwrap_or(&root);
    if root.is_empty() {
        return normalized;
    }
    strip_root_prefix(&normalized, root).unwrap_or(normalized)
}

/// `normalized` が `root` 配下なら、root を剥がした相対パスを返す。
///
/// `root` の直後が `/` であることまで確認する。これが無いと `/repo` と `/repo-backup` の
/// ように prefix を共有する別ディレクトリを同一視してしまう。
fn strip_root_prefix(normalized: &str, root: &str) -> Option<String> {
    let head = normalized.get(..root.len())?;
    if !head.eq_ignore_ascii_case(root) {
        return None;
    }
    let rest = normalized.get(root.len()..)?;
    let rest = rest.strip_prefix('/')?;
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backslashes_become_forward_slashes() {
        assert_eq!(
            to_repo_relative("docs\\adr\\adr-001.md"),
            "docs/adr/adr-001.md"
        );
    }

    /// リポジトリ外のパスは相対化できないのでそのまま返る (= glob に一致せず対象外)。
    #[test]
    fn path_outside_repo_is_returned_as_is() {
        let outside = if cfg!(windows) {
            "D:/elsewhere/docs/x.md"
        } else {
            "/elsewhere/docs/x.md"
        };
        assert_eq!(to_repo_relative(outside), outside);
    }

    /// 実際の hook 入力形式 (cwd 配下の絶対パス) が相対へ落ちること。
    ///
    /// **本 crate の検査層はすべてこの形の入力を受け取る。** 相対パスだけを渡すテストでは
    /// 正規化の欠落を検出できず、実際に `file_size_check` と `paths` 付き custom rule が
    /// 無言で no-op になっていた (2026-08-13)。
    #[test]
    fn absolute_path_under_cwd_becomes_repo_relative() {
        let cwd = std::env::current_dir().expect("cwd");
        let abs = format!("{}/docs/guide.md", cwd.to_string_lossy().replace('\\', "/"));
        assert_eq!(to_repo_relative(&abs), "docs/guide.md");
    }

    #[test]
    fn absolute_path_with_backslashes_becomes_repo_relative() {
        let cwd = std::env::current_dir().expect("cwd");
        let abs = format!("{}\\src\\main.rs", cwd.to_string_lossy());
        assert_eq!(to_repo_relative(&abs), "src/main.rs");
    }

    /// prefix を共有する別ディレクトリ (`<repo>-backup`) を repo 配下と誤認しない。
    #[test]
    fn sibling_directory_sharing_the_prefix_is_not_stripped() {
        let cwd = std::env::current_dir().expect("cwd");
        let sibling = format!("{}-backup/docs/x.md", cwd.to_string_lossy().replace('\\', "/"));
        assert_eq!(to_repo_relative(&sibling), sibling);
    }

    /// cwd 自身 (末尾にファイル名が無い) は相対化しない。
    #[test]
    fn cwd_itself_is_returned_as_is() {
        let cwd = std::env::current_dir().expect("cwd");
        let root = cwd.to_string_lossy().replace('\\', "/");
        assert_eq!(to_repo_relative(&root), root);
    }
}
