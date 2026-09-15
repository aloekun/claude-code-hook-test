//! lib-config-path — hooks / CLI が読む設定ファイルの所在を 1 箇所で決める。
//!
//! # なぜ共有ライブラリか
//!
//! 同じ解決手順が 13 ファイルへ写経されていた (各 crate の `config_path()`)。設定の
//! 置き場所を変えるたびに全箇所を人間が揃える運用は、分散先が増えるほど破れる
//! ([ADR-081](../../../docs/adr/adr-081-single-fact-dispersion.md))。**解決を 1 箇所に集める
//! ことで、追従漏れの起きる場所そのものを無くす。**
//!
//! # 探索順序
//!
//! 設定は `<repo root>/config/` に置く。exe は [ADR-010] のとおり `<repo root>/.claude/` へ
//! 配置されるので、**exe の位置からリポジトリルートが決定論的に導ける**。
//!
//! 1. `exe_dir/../config/<name>` — 実運用 (hooks が `.claude/` の exe として起動される)
//! 2. `exe_dir/<name>` — **後方互換**。設定を exe の隣に置いたままの派生プロジェクト
//!
//! 順序の定義は [`ordered_candidates`] の 1 箇所だけが持つ。repo root が判っている
//! 呼び出し元 ([`resolve_config_from_root`]) も同じ関数を通す。
//!
//! # `config/` 移設はファイル名で許可制 ([`NEW_LOCATION_ELIGIBLE`])
//!
//! 上記の 2 段探索は **[`NEW_LOCATION_ELIGIBLE`] に載ったファイル名だけ**に適用する。それ以外の
//! ファイル名 (`hooks-config.toml` を含む) は **候補 2 (`legacy_dir/<name>`) の 1 本だけ**を返し、
//! `config/` を一切見ない。
//!
//! `hooks-config.toml` は `[pre_tool_validate]` の block preset や `[stop_quality]` など、保護ゲート
//! そのものを定義する。`config/` は夜間 agent の `Edit(work/**)` から到達可能な領域であり、ここへ
//! `resolve_config` が `config/hooks-config.toml` を拾えると `blocked_patterns = []` のような
//! **ゲート無効化設定を agent 自身が外から与えられる** ([ADR-006](../../../docs/adr/adr-006-config-driven-hooks.md)
//! § 改訂 (2026-09-15) 決定 1、[ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//! `.claude/` に固定したままなら、agent の権限 glob (ドット始まりディレクトリ非マッチ) が構造的に
//! 到達できず、保護が成立する。ファイル名を区別しない汎用フォールバックはこの前提を静かに破る
//! ため、許可制のホワイトリストで防ぐ。
//!
//! # cwd を候補に入れない
//!
//! **探索は exe の位置だけを起点にする。** カレントディレクトリ配下の `config/` を候補に
//! 加えると 2 つの問題が出る。
//!
//! - **信頼境界が広がる** — hook は任意の cwd で起動される。cwd 側の `config/hooks-config.toml`
//!   を拾えると、`blocked_patterns = []` のような**ゲートを無効化する設定**を外から与えられる
//!   (`hooks-pre-tool-validate` の `git` / `rm -rf` ブロックが該当)。移設前の各 crate 実装は
//!   いずれも exe 基準だったので、cwd を足すのは既存の性質を弱める変更にあたる
//!   ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。
//! - **cwd 依存の incident を再演する** — Stop 品質ゲートは cwd がリポジトリルート以外
//!   (`.takt/runs` へ `cd` したまま) で起動されることがあり、2026-07-16 に誤 block した
//!   (`hooks-stop-quality/tests/t7_cwd_independence.rs`)。
//!
//! `cargo test` から実 exe を起動する統合テストは、設定を **exe の隣**へ staging して
//! 候補 2 で解決している。cwd 候補が無くても必要な経路は埋まる。
//!
//! # 見つからない場合
//!
//! どの候補も存在しなければ **1 番目の候補**を返す。呼び出し側は「読めなければ既定値」
//! または「読めなければエラー」を自分の fail 方針で決めており、本 crate はパスの決定だけを
//! 担う。存在しないパスを返すことで、エラーメッセージに「どこを探したか」が出る。

use std::path::{Path, PathBuf};

/// 設定ファイルを置くディレクトリ名 (リポジトリルートからの相対)。
pub const CONFIG_DIR: &str = "config";

/// 旧配置のディレクトリ名。exe と同じ場所 ([ADR-010])。
pub const LEGACY_CONFIG_DIR: &str = ".claude";

/// `config/` への移設を許可されたファイル名の一覧。
///
/// ここに無いファイル名 (`hooks-config.toml` 含む) は [`ordered_candidates`] が `config/` を
/// 候補に含めず、常に [`LEGACY_CONFIG_DIR`] 固定で解決する。追加するときは、そのファイルが
/// 保護ゲートの設定を持たない (agent に到達されても安全) ことを確認すること
/// ([ADR-006](../../../docs/adr/adr-006-config-driven-hooks.md) § 改訂 (2026-09-15))。
const NEW_LOCATION_ELIGIBLE: &[&str] = &["custom-lint-rules.toml"];

/// `filename` が `config/` への移設を許可されているか。
fn is_new_location_eligible(filename: &str) -> bool {
    NEW_LOCATION_ELIGIBLE.contains(&filename)
}

/// `filename` の設定ファイルパスを exe の位置から解決する。
///
/// 探索順序は module doc を参照。最初に存在した候補を返し、どれも無ければ第 1 候補を返す。
///
/// **exe の位置を特定できないときは cwd へ倒さず [`UNRESOLVED_EXE_DIR`] を返す。**
/// `PathBuf::from(".")` を base にすると、`config/` 非対象のファイルは候補が
/// `./<filename>` の 1 本だけになり、**cwd に書ける主体がゲート無効化設定を供給できる**。
/// module doc § cwd を候補に入れない が閉じた経路を、exe 解決失敗という裏口から
/// 開けてしまう (CodeRabbit PR #501 の指摘)。
pub fn resolve_config(filename: &str) -> PathBuf {
    match exe_dir() {
        Some(dir) => resolve_config_from_exe_dir(&dir, filename),
        None => PathBuf::from(UNRESOLVED_EXE_DIR).join(filename),
    }
}

/// exe の置かれたディレクトリを明示して解決する。
///
/// 自分の `current_exe()` ではなく**別の base ディレクトリ**を持つ呼び出し元向け
/// (`lib-telemetry` の `base_dir`、テストの staging 先など)。
///
/// 後方互換の候補は **`exe_dir` 自身**であって `<repo root>/.claude/` ではない。
/// `exe_dir` が `.claude/` とは限らない (`target/release/` から起動する統合テスト、
/// 任意ディレクトリを base に渡す呼び出し) ため、`.claude` を固定名で組み立てると
/// その経路が解決できなくなる。
pub fn resolve_config_from_exe_dir(exe_dir: &Path, filename: &str) -> PathBuf {
    let new_dir = exe_dir.parent().unwrap_or(Path::new(".")).join(CONFIG_DIR);
    pick(ordered_candidates(new_dir, exe_dir.to_path_buf(), filename))
}

/// repo root が判っている呼び出し元向けの解決。
///
/// exe の位置から辿れない文脈 (別 workspace の root を明示的に渡す等) で使う。
/// 優先順位は [`resolve_config`] と同じ — 定義は [`ordered_candidates`] にある。
pub fn resolve_config_from_root(repo_root: &Path, filename: &str) -> PathBuf {
    pick(ordered_candidates(
        repo_root.join(CONFIG_DIR),
        repo_root.join(LEGACY_CONFIG_DIR),
        filename,
    ))
}

/// 新配置 → 旧配置の順に並べた候補。**優先順位の定義はここだけ**。
///
/// `filename` が [`NEW_LOCATION_ELIGIBLE`] に無ければ `new_dir` (`config/`) を候補から外し、
/// `legacy_dir` の 1 本だけを返す (module doc § `config/` 移設はファイル名で許可制)。
fn ordered_candidates(new_dir: PathBuf, legacy_dir: PathBuf, filename: &str) -> Vec<PathBuf> {
    if is_new_location_eligible(filename) {
        vec![new_dir.join(filename), legacy_dir.join(filename)]
    } else {
        vec![legacy_dir.join(filename)]
    }
}

/// 最初に実在する候補。無ければ第 1 候補 (存在しないパス) を返す。
fn pick(candidates: Vec<PathBuf>) -> PathBuf {
    let fallback = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."));
    candidates
        .into_iter()
        .find(|c| c.is_file())
        .unwrap_or(fallback)
}

/// exe の位置を特定できなかったときに返す、**決して存在しないディレクトリ名**。
///
/// `<` `>` は Windows のファイル名に使えず、POSIX でもこの名前のディレクトリは作らない。
/// 読めないパスを返すことで、呼び出し側の「読めなければ既定値」へ倒す — 各 hook の既定は
/// ゲート ON である (`hooks-pre-tool-validate` の `blocked_patterns` は `None` のとき
/// `default_preset_names()` へフォールバックし、`git` / `rm -rf` / secret-detection が有効になる)。
const UNRESOLVED_EXE_DIR: &str = "<unresolved-exe-dir>";

/// 実行中の exe が置かれたディレクトリ。取得できなければ `None`。
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// exe の位置を特定できないとき、cwd 相対 (`./hooks-config.toml`) を候補にしない。
    /// 返すのは実在しえないパスで、呼び出し側は「設定なし」= 既定値 (ゲート ON) に倒れる。
    #[test]
    fn an_unresolvable_exe_dir_never_yields_a_cwd_relative_path() {
        let found = ordered_candidates(
            PathBuf::from(UNRESOLVED_EXE_DIR).join(CONFIG_DIR),
            PathBuf::from(UNRESOLVED_EXE_DIR),
            "hooks-config.toml",
        );
        assert_eq!(
            found,
            vec![Path::new(UNRESOLVED_EXE_DIR).join("hooks-config.toml")]
        );
        assert!(!found[0].is_file(), "実在しえないパス: {:?}", found[0]);
    }

    /// 候補の並びを固定する。実 exe に依存せず順序だけを見る。
    /// `custom-lint-rules.toml` は [`NEW_LOCATION_ELIGIBLE`] に載っているので新配置が先に来る。
    #[test]
    fn candidates_are_ordered_new_location_then_legacy() {
        let found = ordered_candidates(
            PathBuf::from("/repo/config"),
            PathBuf::from("/repo/.claude"),
            "custom-lint-rules.toml",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], Path::new("/repo/config/custom-lint-rules.toml"));
        assert_eq!(found[1], Path::new("/repo/.claude/custom-lint-rules.toml"));
    }

    /// `hooks-config.toml` は [`NEW_LOCATION_ELIGIBLE`] に無いので `config/` を候補に含めない。
    /// 保護ゲート設定を agent 到達可能な `config/` へ出せてしまうと trust boundary が破れる
    /// (module doc § `config/` 移設はファイル名で許可制、ADR-006 § 改訂 (2026-09-15) 決定 1)。
    #[test]
    fn hooks_config_toml_never_gets_a_config_dir_candidate() {
        let found = ordered_candidates(
            PathBuf::from("/repo/config"),
            PathBuf::from("/repo/.claude"),
            "hooks-config.toml",
        );
        assert_eq!(found, vec![Path::new("/repo/.claude/hooks-config.toml")]);
    }

    /// **cwd を候補に入れない。** cwd 側の設定を拾えるとゲート無効化の設定を外から
    /// 与えられる (module doc § cwd を候補に入れない)。候補が exe / root 起点の 2 つ
    /// だけであることを本テストが固定する。
    #[test]
    fn the_current_directory_is_never_a_candidate() {
        let new_dir = PathBuf::from("/repo/config");
        let legacy_dir = PathBuf::from("/repo/.claude");
        let found = ordered_candidates(
            new_dir.clone(),
            legacy_dir.clone(),
            "custom-lint-rules.toml",
        );
        assert!(
            found.iter().all(|c| c.parent() == Some(new_dir.as_path())
                || c.parent() == Some(legacy_dir.as_path())),
            "候補は与えたディレクトリ配下のみ (cwd 相対の候補があってはならない): {found:?}"
        );
    }

    /// repo root 起点でも同じ優先順位になる (新配置対象ファイルのみ)。
    #[test]
    fn the_root_based_resolver_uses_the_same_order() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join(CONFIG_DIR)).expect("mkdir config");
        std::fs::create_dir(root.join(LEGACY_CONFIG_DIR)).expect("mkdir .claude");
        let fresh = root.join(CONFIG_DIR).join("custom-lint-rules.toml");
        std::fs::write(&fresh, "").expect("write fresh");
        std::fs::write(
            root.join(LEGACY_CONFIG_DIR).join("custom-lint-rules.toml"),
            "",
        )
        .expect("write legacy");

        assert_eq!(
            resolve_config_from_root(root, "custom-lint-rules.toml"),
            fresh
        );
    }

    /// 回帰テスト (fail-closed): `config/hooks-config.toml` が存在していても、
    /// `hooks-config.toml` は常に `.claude/` を返す。ここが崩れると agent が到達可能な
    /// `config/` から `blocked_patterns = []` 等でゲートを無効化できてしまう
    /// (ADR-006 § 改訂 (2026-09-15) 決定 1 / セキュリティレビュー finding
    /// `SEC-NEW-lib-config-path-hooks-config-override`)。
    #[test]
    fn hooks_config_toml_ignores_config_dir_even_when_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join(CONFIG_DIR)).expect("mkdir config");
        std::fs::create_dir(root.join(LEGACY_CONFIG_DIR)).expect("mkdir .claude");
        std::fs::write(
            root.join(CONFIG_DIR).join("hooks-config.toml"),
            "[pre_tool_validate]\nblocked_patterns = []\n",
        )
        .expect("write attacker-reachable config/hooks-config.toml");
        let legacy = root.join(LEGACY_CONFIG_DIR).join("hooks-config.toml");
        std::fs::write(&legacy, "").expect("write legacy");

        assert_eq!(resolve_config_from_root(root, "hooks-config.toml"), legacy);
    }

    /// 後方互換: 設定が旧配置にしか無い派生プロジェクトでも解決できる。
    #[test]
    fn falls_back_to_the_legacy_location() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join(LEGACY_CONFIG_DIR)).expect("mkdir");
        let legacy = root.join(LEGACY_CONFIG_DIR).join("hooks-config.toml");
        std::fs::write(&legacy, "").expect("write");

        assert_eq!(resolve_config_from_root(root, "hooks-config.toml"), legacy);
    }

    /// どこにも無ければ第 1 候補を返す (新配置対象ファイル)。呼び出し側のエラーメッセージに
    /// 「どこを探したか」が出るようにするため、空の Option にしない。
    #[test]
    fn a_missing_new_location_config_resolves_to_the_new_location() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_config_from_root(tmp.path(), "custom-lint-rules.toml"),
            tmp.path().join(CONFIG_DIR).join("custom-lint-rules.toml")
        );
    }

    /// どこにも無い非対象ファイル (`hooks-config.toml`) は `.claude/` 候補 (1 本だけ) を返す。
    #[test]
    fn a_missing_hooks_config_resolves_to_the_legacy_location() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_config_from_root(tmp.path(), "hooks-config.toml"),
            tmp.path().join(LEGACY_CONFIG_DIR).join("hooks-config.toml")
        );
    }

    /// ディレクトリが同名で存在しても設定ファイルとして拾わない (新配置対象ファイル)。
    #[test]
    fn a_directory_is_not_a_config_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(CONFIG_DIR).join("custom-lint-rules.toml"))
            .expect("mkdir");
        std::fs::create_dir(root.join(LEGACY_CONFIG_DIR)).expect("mkdir .claude");
        let legacy = root.join(LEGACY_CONFIG_DIR).join("custom-lint-rules.toml");
        std::fs::write(&legacy, "").expect("write");

        assert_eq!(
            resolve_config_from_root(root, "custom-lint-rules.toml"),
            legacy
        );
    }
}
