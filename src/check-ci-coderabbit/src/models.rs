//! Output / GH API deserialize models。
//!
//! 順位 209 / 順位 208 PR A refactor (2369→<800 行) でこの module に集約。
//! production code が触る struct と test 用 default impl をまとめる。

use lib_report_formatter::Finding;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct CheckResult {
    pub(crate) status: String,
    pub(crate) action: String,
    pub(crate) ci: CiStatus,
    pub(crate) coderabbit: CodeRabbitStatus,
    pub(crate) summary: String,
    pub(crate) findings: Vec<Finding>,
    /// CodeRabbit rate-limit が検出された場合のみ Some
    /// PR #89 T2-1: cli-pr-monitor 側で sleep + retrigger の根拠データ
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rate_limit: Option<RateLimitInfo>,
}

/// CodeRabbit rate-limit 検出時の制御情報
#[derive(Serialize, Default)]
pub(crate) struct RateLimitInfo {
    pub(crate) until_unix_secs: i64,
    #[serde(rename = "comment_created_at")]
    pub(crate) comment_event_time: String,
    pub(crate) wait_minutes: u64,
    pub(crate) wait_seconds: u64,
    /// `wait_minutes` / `wait_seconds` を既知書式から実際に読めたか。
    ///
    /// `false` = marker だけ一致した未知書式で、待機時間は既定値
    /// ([`crate::rate_limit::UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES`])。
    ///
    /// **本 field は出力 JSON 上の観測用で、消費するコードは無い**。
    /// cli-pr-monitor の `RateLimitState` は本 field を持たず (typed 化すると
    /// 全 struct literal の更新が必要になる一方、得られるのは park summary の
    /// 文言精度という副次的な利得のため見送った)、既定値適用を運用者に伝える
    /// 経路は [`crate::rate_limit::parse_rate_limit`] が出す stderr 警告
    /// (monitor がログ転送する) が担う。値自体は monitor が保持する checker の
    /// 生 JSON (`check_output`) から参照できる。
    pub(crate) wait_time_parsed: bool,
}

#[derive(Serialize, Default)]
pub(crate) struct CiStatus {
    pub(crate) overall: String,
    pub(crate) runs: Vec<CiRunSummary>,
}

#[derive(Serialize, Clone)]
pub(crate) struct CiRunSummary {
    pub(crate) name: String,
    pub(crate) conclusion: String,
}

#[derive(Serialize, Default)]
pub(crate) struct CodeRabbitStatus {
    pub(crate) review_state: String,
    pub(crate) new_comments: usize,
    pub(crate) actionable_comments: Option<usize>,
    pub(crate) unresolved_threads: Option<usize>,
    /// 順位 208: CR walkthrough body の clean marker を検出した場合 true。
    #[serde(default)]
    pub(crate) walkthrough_clean: bool,
}

#[derive(Deserialize)]
pub(crate) struct GhRunItem {
    pub(crate) name: String,
    pub(crate) conclusion: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GhStatusItem {
    pub(crate) context: Option<String>,
    pub(crate) state: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct GhComment {
    pub(crate) user: Option<GhUser>,
    pub(crate) body: Option<String>,
    pub(crate) created_at: Option<String>,
    /// CodeRabbit が rate-limit comment を編集して待機時間を更新する場合に使用。
    pub(crate) updated_at: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GhUser {
    pub(crate) login: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GhReview {
    pub(crate) user: Option<GhUser>,
    pub(crate) body: Option<String>,
    pub(crate) submitted_at: Option<String>,
}

/// PR インラインレビューコメント (pulls/{pr}/comments)
#[derive(Deserialize)]
pub(crate) struct GhPullComment {
    pub(crate) id: Option<u64>,
    pub(crate) user: Option<GhUser>,
    pub(crate) body: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) line: Option<u64>,
    pub(crate) original_line: Option<u64>,
    pub(crate) created_at: Option<String>,
    pub(crate) in_reply_to_id: Option<u64>,
    pub(crate) html_url: Option<String>,
}

/// `--list-findings` モードの出力 1 件分 (ADR-034 Sub-PR 1)。
#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct ListedFinding {
    pub(crate) severity: String,
    pub(crate) file: String,
    pub(crate) line: u64,
    pub(crate) summary: String,
    pub(crate) url: String,
}

/// `--list-findings` モードの top-level 出力 (`{"findings": [...]}`).
#[derive(Serialize)]
pub(crate) struct ListFindingsOutput {
    pub(crate) findings: Vec<ListedFinding>,
}
