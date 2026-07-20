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
    /// 待ち時間を comment 本文から**実際に読み取れたか**。
    ///
    /// `false` = rate-limit comment とは判定できたが、CR の文面が既知のどの書式にも
    /// 一致せず既定値で代替したことを意味する (CR の書式変更を検知した状態)。
    /// 下流はこれを見て「wakeup 時刻は当てにならない」と扱い、書式追加が必要な事実を
    /// 可視化する。ADR-034 § CR rate-limit format evolution 参照。
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
