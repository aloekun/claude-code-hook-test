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
pub const EXTERNAL_ENV: &str = "AUTONOMY_ENABLED";

/// `autonomy-config.toml` のトップレベル (autonomy section のみ関心)。
#[derive(serde::Deserialize)]
struct AutonomyConfigFile {
    autonomy: Option<AutonomySection>,
}

#[derive(serde::Deserialize)]
struct AutonomySection {
    enabled: Option<bool>,
    max_open_autonomous_prs: Option<u32>,
}

/// `[autonomy]` から読めた値。各フィールドの `None` は「読めなかった」= 停止側。
///
/// 型として 1 つにまとめてあるのは、呼び手にファイルを 2 回読ませないため。読み取りを
/// 分けると、2 回の read の間に config が差し替わった場合に「kill-switch は旧世代・閾値は
/// 新世代」という混成状態で判定しうる。1 回の read から派生した値だけを使う。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepoConfig {
    /// `[autonomy] enabled` (kill-switch のリポジトリ側の面)。
    pub enabled: Option<bool>,
    /// `[autonomy] max_open_autonomous_prs` (自律 PR 背圧の閾値、ADR-071)。
    pub max_open_autonomous_prs: Option<u32>,
}

/// `[autonomy]` section を読む。読めない一切のケースは全フィールド `None`。
///
/// toml の parse は section 単位ではなくファイル単位なので、**どれか 1 つのキーが型違いだと
/// 全フィールドが `None` に倒れる**。これは意図した挙動で、config の一部が壊れている状態を
/// 「半分だけ有効」で運転させない (ADR-043 fail-closed)。
///
/// **CI から呼ぶ場合、`path` は master ref から取り出した写しでなければならない。**
/// PR ブランチの checkout をそのまま渡すと、自律 actor 自身 (または injection を受けた
/// fix エージェント) が `claude/` ブランチ上で config を書き換えて自己解除できる
/// (ADR-054 が塞いだ信頼境界と同型)。本 exe はパスの出所を検証できないため、これは
/// 呼び手の契約であり、履行の監査は deny/allow 行に出る `config=` の実パスで行う。
pub fn read_repo_config(path: &Path) -> RepoConfig {
    let Some(section) = read_autonomy_section(path) else {
        return RepoConfig::default();
    };
    RepoConfig {
        enabled: section.enabled,
        max_open_autonomous_prs: section.max_open_autonomous_prs,
    }
}

fn read_autonomy_section(path: &Path) -> Option<AutonomySection> {
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: AutonomyConfigFile = toml::from_str(&content).ok()?;
    parsed.autonomy
}

/// 外部フラグの生値。未設定 / 非 UTF-8 は `None` (= 停止)。
pub fn read_external_raw() -> Option<String> {
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

    fn enabled_of(body: &str) -> Option<bool> {
        let (_dir, path) = config_with(body);
        read_repo_config(&path).enabled
    }

    fn limit_of(body: &str) -> Option<u32> {
        let (_dir, path) = config_with(body);
        read_repo_config(&path).max_open_autonomous_prs
    }

    #[test]
    fn reads_explicit_boolean_values() {
        assert_eq!(enabled_of("[autonomy]\nenabled = true\n"), Some(true));
        assert_eq!(enabled_of("[autonomy]\nenabled = false\n"), Some(false));
    }

    #[test]
    fn reads_the_autonomous_pr_backpressure_threshold() {
        assert_eq!(
            limit_of("[autonomy]\nenabled = true\nmax_open_autonomous_prs = 3\n"),
            Some(3)
        );
        assert_eq!(
            limit_of("[autonomy]\nenabled = true\nmax_open_autonomous_prs = 0\n"),
            Some(0)
        );
    }

    /// 閾値キーが無ければ `None` = 背圧未接続 = 自律 PR は deny。既定値へ倒さない
    /// (「書き忘れたら 3 件まで自動で作る」という fail-open を作らないため)。
    #[test]
    fn missing_threshold_key_is_none_not_a_default() {
        assert_eq!(limit_of("[autonomy]\nenabled = true\n"), None);
    }

    /// 旧キー名 `max_open_draft_prs` だけが残った config は「閾値なし」= deny に倒れる
    /// (ADR-072 決定 15 の改名)。unknown key は無視される仕様なので、改名を取りこぼした
    /// config が**古い値のまま効き続ける**ことはない。
    #[test]
    fn the_pre_rename_threshold_key_does_not_connect_the_backpressure() {
        assert_eq!(limit_of("[autonomy]\nenabled = true\nmax_open_draft_prs = 3\n"), None);
    }

    #[test]
    fn missing_file_is_all_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            read_repo_config(&dir.path().join("absent.toml")),
            RepoConfig::default()
        );
    }

    #[test]
    fn missing_section_or_key_is_none() {
        assert_eq!(enabled_of("[other]\nvalue = 1\n"), None);
        assert_eq!(enabled_of("[autonomy]\n"), None);
    }

    #[test]
    fn malformed_toml_is_all_none() {
        let (_dir, path) = config_with("[autonomy\nenabled = true\n");
        assert_eq!(read_repo_config(&path), RepoConfig::default());
    }

    /// bool 以外の型で書かれた `enabled` は parse 失敗 → `None` (= 停止)。
    /// 「`enabled = "true"` と書いたのに有効にならない」は fail-closed として正しい挙動。
    #[test]
    fn non_boolean_enabled_is_none() {
        assert_eq!(enabled_of("[autonomy]\nenabled = \"true\"\n"), None);
        assert_eq!(enabled_of("[autonomy]\nenabled = 1\n"), None);
    }

    /// 閾値の型違い (文字列 / 負値 / 小数) は section 全体の parse を失敗させ、
    /// **kill-switch 側の `enabled` も** `None` へ倒す。config が半壊した状態で
    /// 「kill-switch だけ有効」と読ませないための挙動を明示的に固定する。
    #[test]
    fn malformed_threshold_also_disables_the_kill_switch_flag() {
        for body in [
            "[autonomy]\nenabled = true\nmax_open_autonomous_prs = \"3\"\n",
            "[autonomy]\nenabled = true\nmax_open_autonomous_prs = -1\n",
            "[autonomy]\nenabled = true\nmax_open_autonomous_prs = 1.5\n",
        ] {
            let (_dir, path) = config_with(body);
            assert_eq!(
                read_repo_config(&path),
                RepoConfig::default(),
                "半壊 config が部分的に有効と読まれた: {body}"
            );
        }
    }

    /// ディレクトリを指された場合も read に失敗して全 `None`。
    #[test]
    fn directory_path_is_all_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_repo_config(dir.path()), RepoConfig::default());
    }

    /// 未知のキーは無視する。config へ新しい設定を足しても、旧バイナリが
    /// 「parse 失敗 → 全停止」に倒れないようにするため。
    #[test]
    fn unknown_keys_are_ignored() {
        assert_eq!(
            enabled_of("[autonomy]\nenabled = true\nfuture_knob = \"x\"\n"),
            Some(true)
        );
    }
}
