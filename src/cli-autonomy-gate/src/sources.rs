//! フラグ 2 拠点の読み取り (I/O 境界)。
//!
//! ADR-052 原則 5 が要求する「単一フラグ (リポジトリ内 config + CI variable)」の物理面。
//! 判定は行わず、読めた値をそのまま [`crate::decision`] へ渡す。
//!
//! # エラーを `None` へ潰す理由
//!
//! ファイル欠落 / パーミッション / parse 失敗を区別しても判定は変わらない — どれも
//! 「フラグが接続されていない」であり、ADR-052 原則 5 は一律 deny を要求する。区別を
//! 型に持ち込むと、呼び手に「この Err なら通してよい」と誤読させる余地を作る。
//! 診断に必要な情報は deny 時に読み取り先パスを loud 出力することで担保する。

use std::path::Path;

/// 外部フラグの env 名。
///
/// CI では workflow が `vars.AUTONOMY_ENABLED` (Actions variable、admin のみ書き込み可) を
/// この env へ写して渡す。ローカル自律 actor では実行環境の env をそのまま使う。
/// どちらの経路でも「未設定 = 停止」で、変数の削除がそのまま緊急停止になる
/// (ADR-060 の `CLOUD_HARNESS` と同じ操作反射)。
pub(crate) const EXTERNAL_ENV: &str = "AUTONOMY_ENABLED";

/// `autonomy-config.toml` のトップレベル (autonomy section のみ関心)。
#[derive(serde::Deserialize)]
struct AutonomyConfigFile {
    autonomy: Option<AutonomySection>,
}

#[derive(serde::Deserialize)]
struct AutonomySection {
    enabled: Option<bool>,
}

/// `[autonomy] enabled` を読む。読めない一切のケースは `None`。
///
/// **CI から呼ぶ場合、`path` は master ref から取り出した写しでなければならない。**
/// PR ブランチの checkout をそのまま渡すと、自律 actor 自身 (または injection を受けた
/// fix エージェント) が `claude/` ブランチ上で config を書き換えて自己解除できる
/// (ADR-054 が塞いだ信頼境界と同型)。本 exe はパスの出所を検証できないため、これは
/// 呼び手の契約であり、履行の監査は deny/allow 行に出る `config=` の実パスで行う。
pub(crate) fn read_repo_config_enabled(path: &Path) -> Option<bool> {
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: AutonomyConfigFile = toml::from_str(&content).ok()?;
    parsed.autonomy?.enabled
}

/// 外部フラグの生値。未設定 / 非 UTF-8 は `None` (= 停止)。
pub(crate) fn read_external_raw() -> Option<String> {
    std::env::var(EXTERNAL_ENV).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn config_with(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("autonomy-config.toml");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        (dir, path)
    }

    #[test]
    fn reads_explicit_boolean_values() {
        let (_dir, path) = config_with("[autonomy]\nenabled = true\n");
        assert_eq!(read_repo_config_enabled(&path), Some(true));
        let (_dir, path) = config_with("[autonomy]\nenabled = false\n");
        assert_eq!(read_repo_config_enabled(&path), Some(false));
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            read_repo_config_enabled(&dir.path().join("absent.toml")),
            None
        );
    }

    #[test]
    fn missing_section_or_key_is_none() {
        let (_dir, path) = config_with("[other]\nvalue = 1\n");
        assert_eq!(read_repo_config_enabled(&path), None);
        let (_dir, path) = config_with("[autonomy]\n");
        assert_eq!(read_repo_config_enabled(&path), None);
    }

    #[test]
    fn malformed_toml_is_none() {
        let (_dir, path) = config_with("[autonomy\nenabled = true\n");
        assert_eq!(read_repo_config_enabled(&path), None);
    }

    /// bool 以外の型で書かれた `enabled` は parse 失敗 → `None` (= 停止)。
    /// 「`enabled = "true"` と書いたのに有効にならない」は fail-closed として正しい挙動。
    #[test]
    fn non_boolean_enabled_is_none() {
        let (_dir, path) = config_with("[autonomy]\nenabled = \"true\"\n");
        assert_eq!(read_repo_config_enabled(&path), None);
        let (_dir, path) = config_with("[autonomy]\nenabled = 1\n");
        assert_eq!(read_repo_config_enabled(&path), None);
    }

    /// ディレクトリを指された場合も read に失敗して `None`。
    #[test]
    fn directory_path_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_repo_config_enabled(dir.path()), None);
    }
}
