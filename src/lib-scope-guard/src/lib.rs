//! lib-scope-guard — ADR-054 決定論層 (fix scope 検証) の**単一実装**。
//!
//! fix エージェント (CodeRabbit コメント等の外部非信頼テキストを入力に持つ) が finding 対象外の
//! ファイルを改変する prompt injection を、決定論的に検知するための判定コア。
//!
//! ## 呼び出し元
//!
//! - `cli-pr-monitor` の auto-push 前 scope guard (ローカル経路、jj diff)。
//! - `cli-fix-push-gate` (ADR-067) の CI 側 scope guard (GitHub Actions 経路、git diff)。
//!
//! 2 経路が同じ判定を持つ必要があるため lib 化した (ADR-044 層 1「3+ crate 重複」ではなく
//! 「同一 ADR の判定が 2 経路に分岐する」ケース — 分岐した瞬間に片方だけ緩む drift が
//! ADR-054 の防御を無効化するため、重複数ではなく **判定の同一性** を根拠に抽出する)。
//!
//! ## 本 crate の範囲
//!
//! 純粋な文字列処理のみ。diff の取得 (jj / git)、mode 判定 (enforce / observe)、ログ出力、
//! kill-switch はすべて呼び出し側の責務。本 crate は「変更ファイル集合が許可集合に収まるか」
//! だけを答える。
//!
//! ## fail-closed
//!
//! パース不能な diff summary 行 (rename `R` 等の非 M/A/D 行を含む) は [`Err`] を返す。
//! 呼び出し側は Err を violation として扱うこと (判定不能を通過させない、ADR-043)。

use std::collections::BTreeSet;

/// fix step が正当に refresh する中間ファイル。findings 由来 allowlist に加えて常に許可する。
///
/// `.takt/review-diff.txt` は fix facet が入力 diff を書き戻す先で、finding とは無関係に
/// 更新される。これを violation にすると正当な fix が毎回 block される。
pub const ALWAYS_ALLOWED: &[&str] = &[".takt/review-diff.txt"];

/// パスを正規化する。Windows のバックスラッシュ区切りを `/` へ揃え、前後空白を除去する。
///
/// `jj diff --summary` は Windows で `\` 区切りを出すが findings の `file` は `/` 区切りで、
/// 正規化しないと同一ファイルが別物として allowlist 外に落ちる。
pub fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

/// 許可パス集合を作る。空パスは除外する。
///
/// 呼び出し側は findings の `file` 列 (ローカル経路) や findings JSON (CI 経路) から
/// パス文字列を取り出して渡す。型を受け取らないのは、findings の表現が経路ごとに
/// 異なるため (`lib_report_formatter::Finding` / CI 側の JSON)。
pub fn allowlist_from_paths<'a, I>(paths: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    paths
        .into_iter()
        .map(normalize_path)
        .filter(|p| !p.is_empty())
        .collect()
}

/// `M path` 形式の diff summary から変更ファイルパスを抽出する。
///
/// fail-closed: パース不能な行・未対応 status (rename `R` 等) は [`Err`]。
/// rename を通さないのは、rename が「元パスの削除 + 新パスの追加」であり、
/// 片方が allowlist 外である可能性を 1 行からは判定できないためである。
pub fn parse_changed_files(summary: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((status, path)) = line.split_once(' ') else {
            return Err(format!("パース不能な diff summary 行: {line:?}"));
        };
        if !matches!(status, "M" | "A" | "D") {
            return Err(format!("未対応の diff status (fail-closed): {line:?}"));
        }
        files.push(normalize_path(path));
    }
    Ok(files)
}

/// 変更ファイルのうち allowlist にも [`ALWAYS_ALLOWED`] にも含まれないものを返す。
/// 空 Vec は「scope 内に収まっている」= PASS を意味する。
pub fn find_out_of_scope(changed: &[String], allowlist: &BTreeSet<String>) -> Vec<String> {
    changed
        .iter()
        .filter(|f| !allowlist.contains(*f) && !ALWAYS_ALLOWED.contains(&f.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_converts_windows_separators_and_trims() {
        assert_eq!(normalize_path("src\\cli\\main.rs"), "src/cli/main.rs");
        assert_eq!(normalize_path("  src/a.rs  "), "src/a.rs");
    }

    #[test]
    fn allowlist_normalizes_and_drops_empty() {
        let allow = allowlist_from_paths(["src/a.rs", "src\\b.rs", "", "   "]);
        assert!(allow.contains("src/a.rs"));
        assert!(allow.contains("src/b.rs"), "バックスラッシュは正規化される");
        assert_eq!(allow.len(), 2, "空パス / 空白のみは除外される");
    }

    #[test]
    fn parse_changed_files_extracts_mad_paths() {
        let files = parse_changed_files("M src/a.rs\nA src/b.rs\nD src/c.rs\n").unwrap();
        assert_eq!(files, vec!["src/a.rs", "src/b.rs", "src/c.rs"]);
    }

    #[test]
    fn parse_changed_files_skips_blank_lines() {
        let files = parse_changed_files("\n\nM src/a.rs\n\n").unwrap();
        assert_eq!(files, vec!["src/a.rs"]);
    }

    #[test]
    fn parse_changed_files_rejects_rename_and_unparseable_lines() {
        assert!(parse_changed_files("R old.rs new.rs").is_err(), "rename は fail-closed");
        assert!(parse_changed_files("weird-line-without-status").is_err());
        assert!(parse_changed_files("M\tsrc/a.rs").is_err(), "tab 区切りは想定外形式");
    }

    #[test]
    fn parse_changed_files_accepts_paths_containing_spaces() {
        let files = parse_changed_files("M docs/a b.md\n").unwrap();
        assert_eq!(files, vec!["docs/a b.md"], "最初の空白のみを区切りとして扱う");
    }

    #[test]
    fn empty_summary_yields_no_changed_files() {
        assert_eq!(parse_changed_files("").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn find_out_of_scope_flags_files_outside_allowlist() {
        let allowlist = allowlist_from_paths(["src/a.rs"]);
        let changed = vec!["src/a.rs".to_string(), ".claude/settings.json".to_string()];
        assert_eq!(find_out_of_scope(&changed, &allowlist), vec![".claude/settings.json"]);
    }

    #[test]
    fn find_out_of_scope_allows_review_diff_refresh() {
        let allowlist = allowlist_from_paths(["src/a.rs"]);
        let changed = vec!["src/a.rs".to_string(), ".takt/review-diff.txt".to_string()];
        assert!(
            find_out_of_scope(&changed, &allowlist).is_empty(),
            ".takt/review-diff.txt は fix の正当な refresh 対象として常に許可"
        );
    }

    /// 正当な fix (変更が allowlist 内に収まる) を violation にしない false-positive ガード。
    #[test]
    fn in_scope_fix_produces_no_violation() {
        let allowlist = allowlist_from_paths(["src/main.rs", "src/lib.rs"]);
        let changed = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        assert!(find_out_of_scope(&changed, &allowlist).is_empty());
    }

    /// findings が空 (= allowlist 空) のとき、あらゆる変更が out-of-scope になる。
    /// 「finding が無いのに fix が何か書いた」は injection の典型形なので block 側が正しい。
    #[test]
    fn empty_allowlist_rejects_every_change() {
        let allowlist = allowlist_from_paths(std::iter::empty::<&str>());
        let changed = vec!["src/a.rs".to_string()];
        assert_eq!(find_out_of_scope(&changed, &allowlist), vec!["src/a.rs"]);
    }
}
