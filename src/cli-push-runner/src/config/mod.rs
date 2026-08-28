use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod docs_only_routing;
mod ledger_completion;
mod lint_screen;
mod post_takt_regate;
mod pr_size_check;
mod scratch_file_warning;
mod testability_gate;

pub(crate) use docs_only_routing::DocsOnlyRoutingConfig;
pub(crate) use ledger_completion::LedgerCompletionConfig;
pub(crate) use lint_screen::{
    LintScreenConfig, DEFAULT_LINT_SCREEN_ENDPOINT, DEFAULT_LINT_SCREEN_EXE_PATH,
    DEFAULT_LINT_SCREEN_MAX_DIFF_LINES, DEFAULT_LINT_SCREEN_MODEL, DEFAULT_LINT_SCREEN_OUTPUT_PATH,
    DEFAULT_LINT_SCREEN_TIMEOUT_SECS,
};
pub(crate) use post_takt_regate::{PostTaktRegateConfig, DEFAULT_MAX_SHRINK_PCT};
pub(crate) use pr_size_check::{
    PrSizeCheckConfig, DEFAULT_PR_SIZE_BLOCK_THRESHOLD, DEFAULT_PR_SIZE_WARNING_THRESHOLD,
};
pub(crate) use scratch_file_warning::ScratchFileWarningConfig;
pub(crate) use testability_gate::{
    validate_testability_gate_mode, TestabilityGateConfig, DEFAULT_TESTABILITY_GATE_MODE,
};

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

/// base branch として使う ref を決める: **remote tracking ref (`<branch>@origin`) が
/// 解決できればそちらを優先**し、無ければローカル bookmark 名のまま返す (順位 254)。
///
/// ## なぜローカル bookmark ではいけないか
///
/// ローカル `master` は `jj git fetch` では自動追随しない (`git.auto-local-bookmark`
/// 既定 false)。並列 workspace 運用 (ADR-045) で片方が fetch しただけの状態だと
/// ローカル `master` が remote より古いまま残り、`master..@` に**他 PR のマージ済み
/// commit が混入**する。pr_size_check はこれを PR 範囲として計測するため、実 160 行の
/// PR が 1604 行と判定されて誤 block した (順位 254 実観測)。
///
/// 使い捨て jj リポジトリでの実測 (jj 0.42): ローカル master を 3 commit 分遅らせた
/// 構成で `master..@` = 4 commit / `master@origin..@` = 1 commit。
///
/// ADR-013 の `sync_local` が `master@origin` を原則としてテストで固定しているのと
/// 同じ原則を、push 経路の PR 範囲解決にも適用する。
///
/// ## fallback を持つ理由
///
/// remote 名が `origin` でない / remote 自体が無い / まだ fetch していない環境では
/// `<branch>@origin` の revset 解決が **エラーになる** (jj 0.42 実測:
/// `Revision \`nosuch@origin\` doesn't exist`)。config を `master@origin` に
/// 書き換える案を採らなかったのはこのためで、解決できないときは静かにローカル名へ
/// 戻す (派生プロジェクトへ配布しても壊れない)。
///
/// `exists` は revset 解決可否の判定関数。テストから注入できるよう引数化する
/// (jj subprocess を呼ばない pure test を可能にするため)。
pub(crate) fn preferred_base_ref(branch: &str, exists: impl Fn(&str) -> bool) -> String {
    // NOTE: 既に remote 修飾されている (`master@origin` 等) なら二重修飾しない。
    if branch.contains('@') {
        return branch.to_string();
    }
    let remote_ref = format!("{}@{}", branch, DEFAULT_REMOTE);
    if exists(&remote_ref) {
        remote_ref
    } else {
        branch.to_string()
    }
}

/// remote tracking ref の解決先 remote 名。jj / git の慣習どおり `origin` 固定。
/// 別名 remote を使う環境では [`preferred_base_ref`] の fallback でローカル名に戻る。
pub(crate) const DEFAULT_REMOTE: &str = "origin";

/// `remote_ref_exists` 用の timeout (I/O)。remote 追跡 ref 解決の軽量な有無チェック
/// (`--limit 1` の読み取り専用 `jj log`) であり、`push_jj_bookmark.rs::JJ_TIMEOUT_SECS`
/// (30s、bookmark 一覧取得などより重い呼び出し向け) より短く取る。
const REMOTE_REF_EXISTS_TIMEOUT_SECS: u64 = 10;

thread_local! {
    /// `remote_ref_exists` の結果メモ化用プロセス内キャッシュ (SIM-NEW-config-mod-L228)。
    ///
    /// `Config::diff_pr_range` / `docs_only_pr_range` / `pr_size_pr_range` は 1 回の push で
    /// 合計最低 9 回呼ばれるが、同一 push 内で revset の remote 解決結果が変わることはない
    /// (push-runner は実行中に fetch しない) ため、revset 文字列をキーに `jj log` spawn の
    /// 結果を使い回して subprocess spawn 回数を stage 数 (3) 相当まで減らす。プロセス内で
    /// `Config` をスレッドを跨いで共有しない (quality_gate の並列実行は `GroupConfig` のみを
    /// 子スレッドへ渡す) ため `thread_local` で足りる。
    static REMOTE_REF_EXISTS_CACHE: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
}

/// `jj log -r <revset>` が成功するかで revset の解決可否を判定する (I/O)。
///
/// タイムアウト無しの同期呼び出しは呼び出し元 (`diff_pr_range` / `pr_size_pr_range` 経由の
/// pre-check/diff 準備段階、いずれも `DEFAULT_STEP_TIMEOUT_SECS` 等のバックストップ対象外)
/// を無期限にハングさせ得るため、`push_jj_bookmark.rs::run_jj` と同じ
/// `lib_subprocess::wait_with_timeout_basic` 経由のタイムアウト付き spawn にする。
/// 結果は `REMOTE_REF_EXISTS_CACHE` にメモ化する。
fn remote_ref_exists(revset: &str) -> bool {
    remote_ref_exists_cached(revset, || {
        resolve_with_timeout(REMOTE_REF_EXISTS_TIMEOUT_SECS, || {
            use std::process::Stdio;
            std::process::Command::new("jj")
                .args([
                    "log", "-r", revset, "--no-graph", "-T", "commit_id", "--limit", "1",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
    })
}

/// [`REMOTE_REF_EXISTS_CACHE`] を通して `resolve` を高々 1 回だけ呼ぶ。
///
/// 解決の I/O を closure で受けるのは、**キャッシュが効いているか**を jj に依存せず
/// テストするため (呼び出し回数を数える closure を注入する)。
pub(crate) fn remote_ref_exists_cached(revset: &str, resolve: impl FnOnce() -> bool) -> bool {
    if let Some(cached) = REMOTE_REF_EXISTS_CACHE.with(|cache| cache.borrow().get(revset).copied()) {
        return cached;
    }
    let resolved = resolve();
    REMOTE_REF_EXISTS_CACHE.with(|cache| cache.borrow_mut().insert(revset.to_string(), resolved));
    resolved
}

/// spawn 済み child を **timeout 付きで待ち**、正常終了したかを返す。
///
/// spawn 自体を closure で受けるのは、timeout / spawn 失敗の経路を
/// jj に依存せずテストするため (テストからは `sleep` 等の任意コマンドを注入する)。
///
/// **どの失敗経路も `false` に倒す**のが要点。`false` = 「remote tracking ref は
/// 解決できない」= ローカル bookmark 名へフォールバックであり、
/// [`preferred_base_ref`] の安全側 (従来挙動) にあたる。
fn resolve_with_timeout(
    timeout_secs: u64,
    spawn: impl FnOnce() -> std::io::Result<std::process::Child>,
) -> bool {
    let mut child = match spawn() {
        Ok(child) => child,
        Err(_) => return false,
    };

    let stdout_handle =
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status = lib_subprocess::wait_with_timeout_basic(
        "remote_ref_exists",
        &mut child,
        timeout_secs,
    );

    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    matches!(status, Ok(Some(s)) if s.success())
}

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
    pub(crate) ledger_completion: Option<LedgerCompletionConfig>,
    pub(crate) pr_size_check: Option<PrSizeCheckConfig>,
    pub(crate) pre_push_review: Option<PrePushReviewConfig>,
    pub(crate) docs_only_routing: Option<DocsOnlyRoutingConfig>,
    pub(crate) post_takt_regate: Option<PostTaktRegateConfig>,
    pub(crate) testability_gate: Option<TestabilityGateConfig>,
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
    ///
    /// base は [`preferred_base_ref`] を通し、**remote tracking ref を優先**する
    /// (順位 254)。この 1 箇所を通すことで diff / docs_only_routing / pr_size_check の
    /// 3 stage が同時に直る。
    pub(crate) fn pr_range_revset(&self, section_override: Option<&str>) -> String {
        self.pr_range_revset_with(section_override, remote_ref_exists)
    }

    /// [`Config::pr_range_revset`] の I/O 注入版。
    ///
    /// remote tracking ref の解決可否判定 (`exists`) を差し替えられるようにして、
    /// config 解決の単体テストが**実 jj リポジトリの状態に依存しない**ようにする
    /// (本 repo では `master@origin` が解決できてしまうため、注入しないと
    /// 「base の優先順位」を検証する既存テストが環境依存になる)。
    pub(crate) fn pr_range_revset_with(
        &self,
        section_override: Option<&str>,
        exists: impl Fn(&str) -> bool,
    ) -> String {
        format!(
            "{}..@",
            preferred_base_ref(&self.resolve_base_branch(section_override), exists)
        )
    }

    /// AI レビュー対象 diff の PR 範囲。`remote_ref_exists` 側でメモ化されるため
    /// 同一 revset に対する 2 回目以降の呼び出しでも `jj log` を再 spawn しない
    /// (SIM-NEW-config-mod-L228)。
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
    validate_testability_gate_mode(config.testability_gate.as_ref())?;
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
