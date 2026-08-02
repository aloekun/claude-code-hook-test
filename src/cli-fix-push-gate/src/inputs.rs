//! ゲート入力の読み取り (I/O 境界)。
//!
//! findings JSON と fix diff summary をファイルから読む。どちらも読めない / 壊れている場合は
//! [`Err`] を返し、呼び出し側が deny に倒す (ADR-043 fail-closed)。

use std::collections::BTreeSet;
use std::path::Path;

/// findings JSON の 1 要素。`file` 以外のフィールド (severity / line / issue / …) は
/// scope 判定に不要なため読み飛ばす。
///
/// `lib_report_formatter::Finding` を直接使わないのは、CI 経路の findings が分析 step の
/// 出力であり、全フィールドが揃う保証を型で要求すると些細な欠落で正当な fix まで
/// deny になるため。scope guard に必要な最小限だけを契約とする。
#[derive(serde::Deserialize)]
struct FindingPath {
    file: String,
}

/// findings JSON ファイルから allowlist を組み立てる。
///
/// **この findings は fix を書いたエージェント自身ではなく、先行する読み取り専用の分析 step が
/// 出力したものでなければならない。** 同一エージェントが findings と fix の両方を出すと、
/// scope guard は自己申告の追認になり ADR-054 の防御が成立しない (ローカル経路で review facet と
/// fix step が別エージェントなのと同じ分離を CI でも保つ)。この分離は workflow 側の契約。
pub(crate) fn read_allowlist(path: &Path) -> Result<BTreeSet<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("findings ファイルを読めません ({}): {e}", path.display()))?;
    let findings: Vec<FindingPath> = serde_json::from_str(&content)
        .map_err(|e| format!("findings JSON をパースできません ({}): {e}", path.display()))?;
    Ok(lib_scope_guard::allowlist_from_paths(
        findings.iter().map(|f| f.file.as_str()),
    ))
}

/// fix diff summary をファイルから読む。
///
/// 期待形式は `M path` / `A path` / `D path` の行列 (jj diff --summary と同形)。
/// git 由来の tab 区切りはパース時に fail-closed で弾かれるため、workflow 側が
/// `--name-status` の tab を空白へ正規化して渡す契約とする。
pub(crate) fn read_diff_summary(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("diff summary を読めません ({}): {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("input.json");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        (dir, path)
    }

    #[test]
    fn reads_file_paths_from_findings_json() {
        let (_dir, path) = write_temp(
            r#"[{"file":"docs/a.md","severity":"Major"},{"file":"docs\\b.md"}]"#,
        );
        let allow = read_allowlist(&path).expect("parse");
        assert!(allow.contains("docs/a.md"));
        assert!(allow.contains("docs/b.md"), "バックスラッシュは正規化される");
    }

    #[test]
    fn empty_findings_array_yields_empty_allowlist() {
        let (_dir, path) = write_temp("[]");
        assert!(read_allowlist(&path).expect("parse").is_empty());
    }

    #[test]
    fn missing_file_is_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read_allowlist(&dir.path().join("absent.json")).is_err());
        assert!(read_diff_summary(&dir.path().join("absent.txt")).is_err());
    }

    #[test]
    fn malformed_json_is_error() {
        let (_dir, path) = write_temp("{not json");
        assert!(read_allowlist(&path).is_err());
    }

    /// `file` を欠く要素は契約違反。パース失敗 → 呼び出し側で deny (fail-closed)。
    #[test]
    fn findings_without_file_field_is_error() {
        let (_dir, path) = write_temp(r#"[{"severity":"Major"}]"#);
        assert!(read_allowlist(&path).is_err());
    }

    #[test]
    fn reads_diff_summary_verbatim() {
        let (_dir, path) = write_temp("M docs/a.md\n");
        assert_eq!(read_diff_summary(&path).expect("read"), "M docs/a.md\n");
    }
}
