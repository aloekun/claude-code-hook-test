//! CodeRabbit walkthrough / rate-limit marker 定数 + 判定 helper。

use crate::models::GhComment;

/// CodeRabbit rate-limit body markers。complete list of known formats.
///
/// CR は format を時間経過で変更するため multi-variant 配列で対応する
/// (PR #182/#184 で silent regression を実体観測)。詳細は ADR-034 § CR rate-limit
/// format evolution 参照。
///
/// **2 つの comment class が混在する。** 前 2 つは walkthrough comment が
/// placeholder として投稿されたときの marker で、3 つ目は `@coderabbitai review`
/// への **command ack** (auto-generated reply) の拒否文言。両者は body の語彙が
/// 全く別で、ack 側は `rate limited by coderabbit.ai` を含まない。
///
/// **ack を入れる理由 (2026-08-20 追加)**: placeholder は**同じコメントが後から
/// 実レビュー本文へ編集される**ため marker が消える。一方 ack は要求 1 回につき
/// 1 コメントが残る。placeholder が出ない / 既に書き換わった時点では、ack だけが
/// 唯一の証拠になる。実データで PR #412 (ack 3 件 / placeholder marker 0 件) と
/// PR #387 (同 1 件 / 0 件) を確認した。ack を持たない旧実装はこの窓で
/// rate-limit を検出できず、park / 再 trigger 経路に入らないまま polling を
/// 続けていた (silent success ではない — ADR-064 の陽性証拠 gate が別途効く)。
pub(crate) const RATE_LIMIT_MARKERS: &[&str] = &[
    "Rate limit exceeded",
    "rate limited by coderabbit.ai",
    // command ack の拒否文言。受理時は "Review finished." になるため衝突しない。
    "Review rate limited.",
];

/// 順位 208: CR walkthrough comment が clean 判定を示すときに body に含まれる marker。
pub(crate) const WALKTHROUGH_CLEAN_MARKER: &str =
    "No actionable comments were generated in the recent review.";

/// 順位 208: CR walkthrough comment body の auto-generated header marker。
pub(crate) const WALKTHROUGH_HEADER_MARKER: &str =
    "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->";

/// 順位 585: CR がレビューをスキップしたときに walkthrough comment へ足す段の marker。
///
/// 文言 (「fewer than 10 stars」/「incremental reviews are disabled」など) はスキップの
/// 理由ごとに変わるので見ない。段はレビューが完了すると消える (#585 で実測)。
/// 以前のレビューの walkthrough と同じ comment に同居することがある (#586: clean な
/// walkthrough + incremental のスキップ)。
pub(crate) const SKIP_REVIEW_MARKER: &str =
    "<!-- This is an auto-generated comment: skip review by coderabbit.ai -->";

/// 順位 585: CR がスキップを通知する commit status の description の接頭辞。
///
/// state は `success` なので、これを見ないと完了と区別できない。実測した description:
/// `Review skipped: manual review required for this OSS repository` /
/// `Review skipped: incremental reviews are disabled` /
/// `Review skipped: bot user not eligible for review`。
pub(crate) const STATUS_SKIPPED_PREFIX: &str = "Review skipped";

/// `c.body` に [`SKIP_REVIEW_MARKER`] が含まれていればスキップ通知と判定する。
pub(crate) fn is_skip_review_comment(c: &GhComment) -> bool {
    c.body
        .as_deref()
        .map(|b| b.contains(SKIP_REVIEW_MARKER))
        .unwrap_or(false)
}

/// スキップ通知の「🔍 Trigger review」行を見分ける目印。行は次の形で、チェックを入れると
/// `- [x]` になる (UI でも API の本文編集でも同じ):
///
/// ```text
/// > - [ ] <!-- {"checkboxId":"..."} --> 🔍 Trigger review
/// ```
///
/// 同じ判定を cli-pr-monitor の `stages::trigger_review::classify` も持つ (そちらはチェックを
/// 入れる側)。crate 間で共有する lib が無いので複製している。行の形が変わったら両方を直す。
const TRIGGER_CHECKBOX_MARKER: &str = r#"<!-- {"checkboxId"#;
const TRIGGER_LABEL: &str = "Trigger review";

/// レビューを依頼するコメントの書き出し。cli-pr-monitor の `request_review_by_comment` が
/// `@coderabbitai review` を投稿する。人間が `full review` で依頼することもある。
const REVIEW_REQUEST_COMMANDS: &[&str] = &["@coderabbitai review", "@coderabbitai full review"];

/// `c` がスキップ通知で、その Trigger review 行がチェック済みか (順位 585 の後続)。
///
/// 依頼の後もスキップの段と commit status はしばらく残るので、これを見ないと依頼済みの
/// PR にも「依頼してください」と案内してしまう (#602 で実測)。チェックは本文の編集なので、
/// 依頼した時刻は `updated_at` に出る。
pub(crate) fn has_checked_trigger(c: &GhComment) -> bool {
    is_skip_review_comment(c)
        && c.body.as_deref().unwrap_or("").lines().any(|line| {
            line.contains(TRIGGER_CHECKBOX_MARKER)
                && line.contains(TRIGGER_LABEL)
                && (line.contains("- [x]") || line.contains("- [X]"))
        })
}

/// `c` が `@coderabbitai review` で始まる行を持つか (順位 585 の後続)。
///
/// 投稿者は問わない (依頼は人間名義で出す — trigger_review の module doc)。依頼した時刻は
/// `created_at` で、`updated_at` は使わない。push 前の依頼を後から編集しても、今回の
/// push を依頼したことにはならない (#603 の CodeRabbit 指摘)。
pub(crate) fn has_review_command(c: &GhComment) -> bool {
    c.body.as_deref().unwrap_or("").lines().any(|line| {
        REVIEW_REQUEST_COMMANDS
            .iter()
            .any(|cmd| line.trim_start().starts_with(cmd))
    })
}

/// `c.body` に [`RATE_LIMIT_MARKERS`] のいずれかが含まれていれば rate-limit comment と判定する。
pub(crate) fn is_rate_limit_comment(c: &GhComment) -> bool {
    c.body
        .as_deref()
        .map(|b| RATE_LIMIT_MARKERS.iter().any(|m| b.contains(m)))
        .unwrap_or(false)
}

/// rate-limit comment の reset 計算に使うタイムスタンプを返す。
///
/// `updated_at` (CR が wait 時間を更新した編集時刻) を優先し、未設定なら `created_at`。
/// `created_at` のみで計算すると premature retrigger を引き起こす
/// (PR #97 round 3 Finding 1、2026-04-30 実観測)。
pub(crate) fn rate_limit_event_time(c: &GhComment) -> Option<&str> {
    c.updated_at.as_deref().or(c.created_at.as_deref())
}

/// 順位 208: 単一 comment が CR walkthrough の clean marker を持つか判定する pure helper。
///
/// [`SKIP_REVIEW_MARKER`] の段がある comment は clean と読まない (順位 585)。段と同居する
/// clean marker は以前のレビューのもので、段を足す編集で `updated_at` が push 後になる。
/// 除かないと、レビューされていない push を clean 完了と判定する。
pub(crate) fn is_clean_walkthrough_comment(c: &GhComment, push_time: &str) -> bool {
    let is_coderabbit = c
        .user
        .as_ref()
        .and_then(|u| u.login.as_deref())
        .map(|l| l == "coderabbitai[bot]")
        .unwrap_or(false);
    if !is_coderabbit {
        return false;
    }
    let after_push_time = rate_limit_event_time(c)
        .map(|t| t >= push_time)
        .unwrap_or(false);
    if !after_push_time {
        return false;
    }
    if is_rate_limit_comment(c) {
        return false;
    }
    if is_skip_review_comment(c) {
        return false;
    }
    let body = c.body.as_deref().unwrap_or("");
    body.contains(WALKTHROUGH_HEADER_MARKER) && body.contains(WALKTHROUGH_CLEAN_MARKER)
}
