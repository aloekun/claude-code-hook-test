//! `custom-lint-rules.toml` の `[[rules]]` エントリ型定義と compiled 形式。
//!
//! **サポート field 一覧** (rule author 向け reference、`.claude/custom-lint-rules.toml` 冒頭コメントと整合):
//!
//! | field | 必須 | semantics |
//! |---|---|---|
//! | `id` | yes | ルール一意 identifier |
//! | `pattern` | yes | 検出する正規表現 (case-insensitive にしたい場合は `(?i)` prefix を pattern 内に明示) |
//! | `severity` | yes | `"error"` or `"warning"` |
//! | `message` | yes | 違反時のメッセージ |
//! | `extensions` | yes | 対象拡張子の list (例: `["rs", "toml"]`)。空配列を使うと全 file が対象になる anti-pattern なので避ける |
//! | `why` | optional | ルールの根拠 (ADR 参照 / PR 由来等)。省略可だが post-merge-feedback 由来は明記推奨 |
//! | `paths` | optional | glob pattern による file path filter (順位 102 land 済)。指定時は `extensions` との **AND** 結合で評価。例: `paths = ["docs/**/*.md"]` で docs/ 配下のみ対象。未指定 (None) または空配列は「path filter なし」(= `extensions` のみで判定) |
//! | `fix` | optional | `CustomRuleFix` (strategy + steps) |
//! | `example` | optional | `CustomRuleExample` (bad + good) |
//! | `test_coverage` | optional | `CustomRuleTestCoverage`。rule が targets する main ext (`rs` / `toml` / `yaml` / `yml`) ごとに対応 test 関数名を明示宣言する meta field (順位 137 land 済) |
//!
//! **glob syntax** (`globset` crate 準拠):
//!
//! - `*` = 同階層の 0+ 文字 (path separator は含まない)
//! - `**` = 任意階層の recursive match (`docs/**/*.md` は `docs/a.md` / `docs/adr/b.md` 両方マッチ)
//! - `?` = 単一文字
//! - `[abc]` = 文字 class
//!
//! **`extensions` x `paths` の AND 結合の意義**: `extensions` は file 種別 (rust / toml / md) を絞る軸、
//! `paths` は file 位置 (docs/ 配下 / tests/ 配下) を絞る軸で直交。両方マッチで初めて rule 対象とすることで、
//! rule scope を明示的に二次元で表現できる (ADR-007 amendment 順位 104 で codify 予定)。

use globset::GlobSet;
use regex::Regex;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub(crate) struct CustomRulesConfig {
    pub(crate) rules: Option<Vec<CustomRule>>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct CustomRule {
    pub(crate) id: String,
    pub(crate) pattern: String,
    pub(crate) severity: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) why: String,
    pub(crate) extensions: Vec<String>,
    #[serde(default)]
    pub(crate) paths: Option<Vec<String>>,
    pub(crate) fix: Option<CustomRuleFix>,
    pub(crate) example: Option<CustomRuleExample>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) test_coverage: Option<CustomRuleTestCoverage>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) incident: Option<CustomRuleIncident>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct CustomRuleFix {
    pub(crate) strategy: String,
    pub(crate) steps: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct CustomRuleExample {
    pub(crate) bad: String,
    pub(crate) good: String,
}

/// `[rules.incident]` meta field。WP-08 (ADR-049) で導入。
///
/// ルールを生んだ実 incident (PR 番号) と、その incident を再現する回帰 fixture
/// (bad = ルールが fire すべき入力 / good = fire してはいけない clean 入力) を
/// 機械可読に記録する。`incident_fixture_coverage_check` cargo test が
/// 「incident 由来ルール ⇒ bad/good fixture が実在」を fail-closed で強制し、
/// `tests/incident_eval.rs` E2E test が fixture を実 exe に stdin JSON で食わせて
/// 検出 (severity/type/line) と誤検知ゼロ (good) を検証する。
///
/// この section を持たないルール (例: no-console-log = 汎用サンプルで incident 由来
/// でない) は fixture 要求から免除される (coverage check の NON_INCIDENT_RULES allowlist)。
#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct CustomRuleIncident {
    /// ルールを生んだ実 incident の PR 番号。
    pub(crate) pr: u64,
    /// tests/fixtures/incidents/bad/ 配下の fixture ファイル名 (ルールが fire すべき入力)。
    pub(crate) bad_fixture: String,
    /// tests/fixtures/incidents/good/ 配下の fixture ファイル名 (fire してはいけない clean 入力)。
    pub(crate) good_fixture: String,
    /// 設計根拠となる ADR (例: "adr-007" = custom-lint 正規表現層)。任意。
    #[serde(default)]
    pub(crate) adr: Option<String>,
}

/// `[rules.test_coverage]` meta field。順位 137 (PR #163 T1-#1 採用) で導入。
///
/// 各 rule が「主要拡張子 (`rs` / `toml` / `yaml` / `yml`) のうち targets するもの」に対して
/// **少なくとも 1 個の対応 test 関数** を明示宣言する。`rule_test_coverage_check` cargo test が
/// deploy 済 `.claude/custom-lint-rules.toml` を読んで、宣言された test 関数が module 群に
/// 存在することと、必須カバレッジ (main ext ごとに 1+ test、非 main 専用 rule には
/// `other_ext_tests` 1+) を機械検証する。
///
/// 命名規約に依存しない明示的 mapping を採用 (= 案 b、TOML meta field 方式) することで、
/// `ps_empty_catch_*` / `md_mutable_anchor_*` / `no_ephemeral_todo_*` 等の **異なる命名
/// 規約が混在する既存テスト** を rule_id とは独立に対応付けできる。
#[derive(Deserialize, Clone, Default, Debug)]
#[allow(dead_code)]
pub(crate) struct CustomRuleTestCoverage {
    /// 主要拡張子 (`rs` / `toml` / `yaml` / `yml`) -> 対応 test 関数名の list。
    #[serde(default)]
    pub(crate) main_ext_tests: std::collections::BTreeMap<String, Vec<String>>,
    /// 主要拡張子以外 (`md` / `txt` / `ts` / `js` / `py` / `ps1` 等) の対応 test 関数名 list。
    #[serde(default)]
    pub(crate) other_ext_tests: Vec<String>,
}

/// コンパイル済み正規表現と paths glob set を持つルール。
///
/// `paths_glob` は `rule.paths` が `Some(non-empty)` の場合のみ compiled GlobSet を保持し、
/// `None` (path filter なし) では `None` を保持する。Empty Vec は **filter なし** として扱う。
pub(crate) struct CompiledRule {
    pub(crate) rule: CustomRule,
    pub(crate) regex: Regex,
    pub(crate) paths_glob: Option<GlobSet>,
}
