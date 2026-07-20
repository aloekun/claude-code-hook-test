use serde::Deserialize;
use std::path::{Path, PathBuf};

mod docs_only_routing;
mod lint_screen;
mod post_takt_regate;
mod pr_size_check;
mod scratch_file_warning;

pub(crate) use docs_only_routing::DocsOnlyRoutingConfig;
pub(crate) use post_takt_regate::PostTaktRegateConfig;
pub(crate) use lint_screen::{
    LintScreenConfig, DEFAULT_LINT_SCREEN_ENDPOINT, DEFAULT_LINT_SCREEN_EXE_PATH,
    DEFAULT_LINT_SCREEN_MAX_DIFF_LINES, DEFAULT_LINT_SCREEN_MODEL, DEFAULT_LINT_SCREEN_OUTPUT_PATH,
    DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
};
pub(crate) use pr_size_check::{
    PrSizeCheckConfig, DEFAULT_PR_SIZE_BLOCK_THRESHOLD,
    DEFAULT_PR_SIZE_WARNING_THRESHOLD,
};
pub(crate) use scratch_file_warning::ScratchFileWarningConfig;

use lint_screen::{apply_lint_screen_env_override, ENV_LINT_SCREEN_ENABLED};

pub(crate) const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;
pub(crate) const DEFAULT_PUSH_TIMEOUT_SECS: u64 = 300;

/// diff stage の既定 timeout (T6)。
///
/// 他の jj 系呼び出し (`bookmark_check` の `JJ_TIMEOUT_SECS = 30`) より長く取るのは、
/// diff が working copy の snapshot + 大 diff の書き出しを伴い、読み取りのみの
/// `jj bookmark list` より重いため。timeout の目的は**ハング検知**であって latency
/// 制限ではなく、誤 timeout は diff 失敗 = pipeline 全体の中断 (exit 5) を招くので
/// 余裕側に倒す。詰まる環境では `[diff] timeout` で上書きする。
pub(crate) const DEFAULT_DIFF_TIMEOUT_SECS: u64 = 60;

/// PR base branch の既定値。
///
/// 「PR 範囲」= `format!("{}..@", base_branch)` を組み立てる全 stage
/// (`diff` / `docs_only_routing` / `pr_size_check`) がこの 1 箇所を参照する。
/// 以前は section ごとに `default_branch` を持ち「値を同期する義務」を config の
/// コメントで課していたが、義務はコード上の不変条件ではないため非対称を許した
/// (実際に `[diff]` だけ PR 範囲を見ておらず、祖先コミットが AI レビュー未経由で
/// merge される欠陥が 4 回再発した。todo 順位 288 / ADR-051 cross-config coupling)。
pub(crate) const DEFAULT_BASE_BRANCH: &str = "master";

#[derive(Deserialize)]
pub(crate) struct Config {
    /// 全 stage 共通の PR base branch。section 側の `default_branch` が
    /// 設定されていればそちらが優先される (後方互換のための override)。
    pub(crate) default_branch: Option<String>,
    pub(crate) quality_gate: QualityGateConfig,
    pub(crate) diff: Option<DiffConfig>,
    pub(crate) lint_screen: Option<LintScreenConfig>,
    pub(crate) takt: TaktConfig,
    pub(crate) push: PushConfig,
    pub(crate) scratch_file_warning: Option<ScratchFileWarningConfig>,
    pub(crate) pr_size_check: Option<PrSizeCheckConfig>,
    pub(crate) pre_push_review: Option<PrePushReviewConfig>,
    pub(crate) docs_only_routing: Option<DocsOnlyRoutingConfig>,
    pub(crate) post_takt_regate: Option<PostTaktRegateConfig>,
}

impl Config {
    /// PR base branch を解決する。優先順: section override → top-level → 既定値。
    ///
    /// section override は後方互換のために残してある (既存の派生プロジェクト config が
    /// `[pr_size_check] default_branch` 等を持つため)。新規に section 側で持たせないこと。
    pub(crate) fn resolve_base_branch(&self, section_override: Option<&str>) -> String {
        section_override
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                self.default_branch
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string())
    }

    /// PR 範囲の revset (`<base>..@`) を組み立てる。
    ///
    /// revset literal を各所に散らさないための唯一の組立点
    /// (rule⑫ `no-hardcoded-jj-revset-range` の趣旨を config 側にも適用する)。
    pub(crate) fn pr_range_revset(&self, section_override: Option<&str>) -> String {
        format!("{}..@", self.resolve_base_branch(section_override))
    }

    /// AI レビュー対象 diff の PR 範囲。
    pub(crate) fn diff_pr_range(&self) -> String {
        self.pr_range_revset(self.diff.as_ref().and_then(|c| c.default_branch.as_deref()))
    }

    /// docs-only routing 判定の PR 範囲。[`Config::diff_pr_range`] と一致していなければ
    /// 「レビューした範囲」と「routing を決めた範囲」がずれる。
    pub(crate) fn docs_only_pr_range(&self) -> String {
        self.pr_range_revset(
            self.docs_only_routing
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
        )
    }

    /// PR size 計測の PR 範囲。
    pub(crate) fn pr_size_pr_range(&self) -> String {
        self.pr_range_revset(
            self.pr_size_check
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
        )
    }
}

#[derive(Deserialize)]
pub(crate) struct QualityGateConfig {
    pub(crate) parallel: Option<bool>,
    pub(crate) step_timeout: Option<u64>,
    pub(crate) groups: Vec<GroupConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct GroupConfig {
    pub(crate) name: String,
    pub(crate) pre: Option<String>,
    pub(crate) commands: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct TaktConfig {
    pub(crate) workflow: String,
    pub(crate) task: String,
    pub(crate) extra_args: Option<Vec<String>>,
}

/// pre-push review の refute variant 制御 (WP-06 / ADR-047, 試験運用)。
///
/// ADR-039 (config opt-in): section 不在 / `refute_enabled != Some(true)` /
/// `refute_workflow` 未指定 のいずれでも現行 `[takt] workflow` を使う (default OFF)。
/// 明示的に `refute_enabled = true` かつ `refute_workflow` 指定時のみ refute
/// variant workflow に切り替わる。派生プロジェクトの templates は section を
/// 置かない or `refute_enabled = false` で default OFF を継承する。
#[derive(Deserialize)]
pub(crate) struct PrePushReviewConfig {
    pub(crate) refute_enabled: Option<bool>,
    pub(crate) refute_workflow: Option<String>,
}

/// `[diff] command` 内で PR 範囲 revset に展開されるプレースホルダ。
///
/// config に revset を直書きさせないための間接層。直書きを許すと
/// 「`-r @` (tip のみ)」のような **PR より狭い範囲**を書けてしまい、
/// 祖先コミットが AI レビューを一度も経ずに merge される (todo 順位 288、4 回再発)。
pub(crate) const DIFF_PR_RANGE_PLACEHOLDER: &str = "{{PR_RANGE}}";

#[derive(Deserialize)]
pub(crate) struct DiffConfig {
    /// diff 生成コマンド。[`DIFF_PR_RANGE_PLACEHOLDER`] が PR 範囲 revset に展開される。
    ///
    /// 出力は **unified diff (`--git` 形式)** である必要がある。範囲カバレッジ検査が
    /// `diff --git a/… b/…` ヘッダからファイル一覧を抽出するため。jj の既定形式は
    /// `+`/`-` マーカーを持たず LLM レビュアーが削除を追加と誤読する問題もある
    /// (todo 順位 264、PR #256 で実害)。
    pub(crate) command: String,
    pub(crate) output_path: String,
    /// 未指定時は `DEFAULT_DIFF_TIMEOUT_SECS` (T6)。`[push] timeout` と同形。
    pub(crate) timeout: Option<u64>,
    /// PR base branch の section override (後方互換)。未設定なら top-level を使う。
    pub(crate) default_branch: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PushConfig {
    pub(crate) command: String,
    pub(crate) timeout: Option<u64>,
}

/// `push-runner-config.toml` の探索順序: カレントディレクトリ (pnpm scripts は
/// リポジトリルートで実行される) を優先し、無ければ exe 隣接パスに fallback する。
pub(crate) fn config_path() -> PathBuf {
    let filename = "push-runner-config.toml";
    let cwd_path = Path::new(filename).to_path_buf();
    if cwd_path.exists() {
        return cwd_path;
    }
    exe_adjacent_config_path(filename)
}

/// exe と同じディレクトリ (`.claude/` 配置パターン) 上の config path を返す。
/// `config_path` が cwd に見つからなかった場合の fallback。
fn exe_adjacent_config_path(filename: &str) -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(filename)
}

pub(crate) fn load_config() -> Result<Config, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("設定ファイルの読み込みに失敗: {} ({})", path.display(), e))?;
    let mut config: Config =
        toml::from_str(&content).map_err(|e| format!("設定ファイルのパースに失敗: {}", e))?;
    apply_lint_screen_env_override(&mut config, std::env::var(ENV_LINT_SCREEN_ENABLED).ok());
    validate_config(&config)?;
    Ok(config)
}

/// takt に渡す workflow 名を解決する (WP-06 / ADR-047)。
///
/// 切替判定を本関数 1 箇所に集約する (ADR-039 §設計6点 #5: 3 段 gate の単一化)。
/// `[pre_push_review] refute_enabled = true` かつ `refute_workflow` 指定時のみ
/// refute variant を返し、それ以外は現行 `[takt] workflow` を返す (fail-safe で
/// 現行フロー)。
pub(crate) fn resolve_takt_workflow(config: &Config) -> String {
    if let Some(pre_push) = &config.pre_push_review {
        if pre_push.refute_enabled == Some(true) {
            if let Some(workflow) = &pre_push.refute_workflow {
                return workflow.clone();
            }
        }
    }
    config.takt.workflow.clone()
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.quality_gate.groups.is_empty() {
        return Err("設定ファイルエラー: quality_gate.groups が空です".into());
    }
    for group in &config.quality_gate.groups {
        if group.commands.is_empty() {
            return Err(format!(
                "設定ファイルエラー: group '{}' の commands が空です",
                group.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// base branch 解決の 3 段 (section override → top-level → 既定値) と、
    /// **3 stage が同じ範囲に解決される**ことを固定する。
    ///
    /// 後者が本 PR の要点: 以前は stage ごとに独立解決で、`[diff]` だけが PR 範囲を
    /// 見ていない非対称を許していた (todo 順位 288、4 回再発)。
    mod base_branch_resolution {
        use super::*;

        const MINIMAL: &str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "t"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"

[diff]
command = "jj diff --git -r {{PR_RANGE}}"
output_path = ".takt/d.txt"
"#;

        /// TOML の table 構文上、top-level key は**全 section より前**に置く必要が
        /// あるため、prefix / suffix を分けて組み立てる。
        fn parse(top_level: &str, sections: &str) -> Config {
            toml::from_str(&format!("{}{}{}", top_level, MINIMAL, sections))
                .expect("config should parse")
        }

        #[test]
        fn all_stages_share_the_same_range_by_default() {
            let config = parse("", "");
            let expected = format!("{}..@", DEFAULT_BASE_BRANCH);
            assert_eq!(config.diff_pr_range(), expected);
            assert_eq!(config.docs_only_pr_range(), expected);
            assert_eq!(config.pr_size_pr_range(), expected);
        }

        #[test]
        fn top_level_default_branch_applies_to_all_stages() {
            let config = parse("default_branch = \"main\"\n", "");
            assert_eq!(config.diff_pr_range(), "main..@");
            assert_eq!(config.docs_only_pr_range(), "main..@");
            assert_eq!(config.pr_size_pr_range(), "main..@");
        }

        /// 後方互換: 既存の派生プロジェクト config が持つ section 側の
        /// `default_branch` は top-level より優先される。
        #[test]
        fn section_override_wins_over_top_level() {
            let config = parse(
                "default_branch = \"main\"\n",
                "\n[pr_size_check]\nenabled = true\ndefault_branch = \"develop\"\n",
            );
            assert_eq!(config.pr_size_pr_range(), "develop..@");
            assert_eq!(
                config.diff_pr_range(),
                "main..@",
                "override は指定した section にのみ効く"
            );
        }

        #[test]
        fn blank_override_falls_back_instead_of_producing_empty_range() {
            let config = parse(
                "default_branch = \"main\"\n",
                "\n[pr_size_check]\nenabled = true\ndefault_branch = \"   \"\n",
            );
            assert_eq!(
                config.pr_size_pr_range(),
                "main..@",
                "空白のみの override は未設定として扱う (`..@` を作らない)"
            );
        }

        #[test]
        fn blank_top_level_falls_back_to_default() {
            let config = parse("default_branch = \"\"\n", "");
            assert_eq!(config.diff_pr_range(), format!("{}..@", DEFAULT_BASE_BRANCH));
        }
    }

    #[test]
    fn config_parses_full_without_diff() {
        let toml_str = r#"
[quality_gate]
parallel = true
step_timeout = 60

[[quality_gate.groups]]
name = "lint"
commands = ["pnpm lint"]

[[quality_gate.groups]]
name = "test"
pre = "pnpm install"
commands = ["pnpm test", "pnpm test:e2e"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"
extra_args = ["--pipeline", "--skip-git"]

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.quality_gate.parallel, Some(true));
        assert_eq!(config.quality_gate.step_timeout, Some(60));
        assert_eq!(config.quality_gate.groups.len(), 2);
        assert!(config.diff.is_none());

        assert_eq!(config.takt.workflow, "pre-push-review");
        assert_eq!(config.takt.task, "pre-push review");
        assert_eq!(config.takt.extra_args.as_ref().unwrap().len(), 2);

        assert_eq!(config.push.command, "jj git push");
        assert!(config.push.timeout.is_none());
    }

    #[test]
    fn config_push_timeout_explicit() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "jj git push"
timeout = 600
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.push.timeout, Some(600));
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            600,
        );
    }

    #[test]
    fn config_push_timeout_defaults() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.push.timeout.is_none());
        assert_eq!(
            config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
            DEFAULT_PUSH_TIMEOUT_SECS,
        );
    }

    #[test]
    fn config_parses_with_diff() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "jj git push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        let diff = config.diff.unwrap();
        assert_eq!(diff.command, "jj diff -r @");
        assert_eq!(diff.output_path, ".takt/review-diff.txt");
        assert!(diff.timeout.is_none());
    }

    /// T6: `[diff] timeout` 未指定時は既定値に落ちる (本リポジトリの config は未指定)。
    #[test]
    fn config_diff_timeout_defaults() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let diff = config.diff.unwrap();
        assert!(diff.timeout.is_none());
        assert_eq!(
            diff.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS),
            DEFAULT_DIFF_TIMEOUT_SECS,
        );
    }

    /// T6: 大 diff / 低速環境向けの escape hatch (既定 60s では足りない場合)。
    #[test]
    fn config_diff_timeout_explicit() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"
timeout = 180

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.diff.unwrap().timeout, Some(180));
    }

    #[test]
    fn config_quality_gate_defaults() {
        let toml_str = r#"
[quality_gate]

[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.quality_gate.parallel.unwrap_or(true));
        assert_eq!(
            config
                .quality_gate
                .step_timeout
                .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
            DEFAULT_STEP_TIMEOUT_SECS,
        );
        assert!(config.takt.extra_args.is_none());
    }

    #[test]
    fn config_pre_field_optional() {
        let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "no-pre"
commands = ["echo test"]

[[quality_gate.groups]]
name = "with-pre"
pre = "echo install"
commands = ["echo test"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.quality_gate.groups[0].pre.is_none());
        assert!(config.quality_gate.groups[1].pre.is_some());
    }

    #[test]
    fn validate_rejects_empty_groups() {
        let config = Config {
            default_branch: None,
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![],
            },
            diff: None,
            lint_screen: None,
            scratch_file_warning: None,
            pr_size_check: None,
            pre_push_review: None,
            docs_only_routing: None,
            post_takt_regate: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("groups が空"));
    }

    #[test]
    fn validate_rejects_empty_commands() {
        let config = Config {
            default_branch: None,
            quality_gate: QualityGateConfig {
                parallel: None,
                step_timeout: None,
                groups: vec![GroupConfig {
                    name: "empty".into(),
                    pre: None,
                    commands: vec![],
                }],
            },
            diff: None,
            lint_screen: None,
            scratch_file_warning: None,
            pr_size_check: None,
            pre_push_review: None,
            docs_only_routing: None,
            post_takt_regate: None,
            takt: TaktConfig {
                workflow: "w".into(),
                task: "t".into(),
                extra_args: None,
            },
            push: PushConfig {
                command: "echo".into(),
                timeout: None,
            },
        };
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'empty'"));
    }

    /// resolve_takt_workflow テスト用に base config + 任意の [pre_push_review]
    /// section を組み立てる。base workflow は "pre-push-review"。
    fn config_with_optional_pre_push(pre_push_section: &str) -> Config {
        let toml_str = format!(
            r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "echo push"
{pre_push_section}
"#
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn resolve_workflow_base_when_section_absent() {
        let config = config_with_optional_pre_push("");
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }

    #[test]
    fn resolve_workflow_base_when_refute_disabled() {
        let config = config_with_optional_pre_push(
            "[pre_push_review]\nrefute_enabled = false\nrefute_workflow = \"pre-push-review-refute\"",
        );
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }

    #[test]
    fn resolve_workflow_refute_when_enabled() {
        let config = config_with_optional_pre_push(
            "[pre_push_review]\nrefute_enabled = true\nrefute_workflow = \"pre-push-review-refute\"",
        );
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review-refute");
    }

    #[test]
    fn resolve_workflow_base_when_enabled_but_no_refute_workflow() {
        let config = config_with_optional_pre_push("[pre_push_review]\nrefute_enabled = true");
        assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
    }
}
