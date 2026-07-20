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

/// branch 名を trim し、空文字を `None` に落とす (空白のみの設定値を未設定扱いにする)。
fn normalize_branch(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

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
    /// PR base branch を解決する。優先順:
    /// section override → [`Config::effective_default_branch`] (top-level か、
    /// 全一致する section override 群) → 既定値。
    ///
    /// section override は後方互換のために残してある (既存の派生プロジェクト config が
    /// `[pr_size_check] default_branch` 等を持つため)。新規に section 側で持たせないこと。
    /// override 値が stage 間で食い違わないことは [`validate_base_branch_ranges_agree`]
    /// が `load_config` 時に fail-closed で保証する。
    pub(crate) fn resolve_base_branch(&self, section_override: Option<&str>) -> String {
        normalize_branch(section_override)
            .or_else(|| self.effective_default_branch())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string())
    }

    /// config 全体で共有する effective base branch を返す (CodeRabbit #313)。
    ///
    /// top-level `default_branch` が明示されていればそれ。無ければ section override 群が
    /// **全て一致**していればその値を全 stage 共通の base とする。top-level を書かず
    /// section override だけで base を揃えた legacy config で、override 未設定の stage
    /// (例 `[diff]`) が `DEFAULT_BASE_BRANCH` に落ちる理由だけで
    /// [`validate_base_branch_ranges_agree`] に reject されるのを防ぐ。override 群が
    /// 食い違う場合は `None` を返し、各 stage が自分の override or 既定に解決した結果を
    /// validate が fail-closed で検知する (genuine な不一致は従来どおり拒否)。
    fn effective_default_branch(&self) -> Option<String> {
        if let Some(top) = normalize_branch(self.default_branch.as_deref()) {
            return Some(top);
        }
        let overrides: Vec<String> = [
            self.diff.as_ref().and_then(|c| c.default_branch.as_deref()),
            self.docs_only_routing
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
            self.pr_size_check
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
        ]
        .into_iter()
        .filter_map(normalize_branch)
        .collect();
        let first = overrides.first()?;
        overrides.iter().all(|v| v == first).then(|| first.clone())
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
    validate_diff_command_uses_pr_range(config)?;
    validate_base_branch_ranges_agree(config)?;
    Ok(())
}

/// `[diff] command` が [`DIFF_PR_RANGE_PLACEHOLDER`] を含むことを config-load 時に検証する
/// (CodeRabbit #313)。
///
/// プレースホルダを欠いた command (例: legacy の `jj diff -r @`) は PR 範囲を無視して
/// tip のみをレビュー対象にし得る。範囲カバレッジ検査 (`stages::diff::verify_diff_covers_pr_range`)
/// が runtime の backstop として残るが、config-load 時に明示エラーで弾くことで
/// 「なぜ diff が狭い/空なのか」を実行時ではなく設定時点で判る診断に前倒しする
/// (fail-closed / ADR-043、defense-in-depth)。`[diff]` section 不在時は無検査。
fn validate_diff_command_uses_pr_range(config: &Config) -> Result<(), String> {
    let Some(diff) = &config.diff else {
        return Ok(());
    };
    if diff.command.contains(DIFF_PR_RANGE_PLACEHOLDER) {
        return Ok(());
    }
    Err(format!(
        "設定ファイルエラー: [diff] command が {placeholder} を含みません (現在: {command:?})。\
         PR 範囲全体をレビュー対象にするため command には {placeholder} を使うこと \
         (例: \"jj diff --git -r {placeholder}\")。直書きの範囲 (`-r @` 等) は tip のみを\
         レビューし、祖先コミットが未レビューで merge される (todo 順位 288、4 回再発)",
        placeholder = DIFF_PR_RANGE_PLACEHOLDER,
        command = diff.command,
    ))
}

/// 3 stage (`diff` / `docs_only_routing` / `pr_size_check`) が解決する PR 範囲が
/// 一致することを検査する (SIM-NEW-config-mod-rs-L69)。
///
/// section 側の `default_branch` override は後方互換のために残しているが、
/// 各 stage が独立解決するため、override が top-level や他 section の値と
/// 食い違うと `[diff]` だけが狭い範囲を見る非対称が復活する (todo 順位 288、
/// 4 回再発)。「値を同期する義務」を config のコメントだけに頼らず、
/// コード上の不変条件として fail-closed で強制する。
fn validate_base_branch_ranges_agree(config: &Config) -> Result<(), String> {
    let ranges = [
        ("diff", config.diff_pr_range()),
        ("docs_only_routing", config.docs_only_pr_range()),
        ("pr_size_check", config.pr_size_pr_range()),
    ];
    let (_, diff_range) = &ranges[0];
    if let Some((name, range)) = ranges.iter().find(|(_, range)| range != diff_range) {
        return Err(format!(
            "設定ファイルエラー: PR 範囲が stage 間で一致しません ([{name}] は \"{range}\" に解決、\
             [diff] は \"{diff_range}\" に解決)。default_branch は top-level か、全 section で同じ値にすること"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
