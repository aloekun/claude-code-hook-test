//! 「現 HEAD が CodeRabbit にレビュー済みか」の単一判定 (順位 520)。
//!
//! # なぜ 1 か所に集約するのか
//!
//! CodeRabbit の auto レビュー挙動は外部要因で何度も変わっている。2026-09-08 には
//! star 10 未満のリポジトリで auto が止まり、**2026-09-10 頃に bot 作成 PR でも
//! PR 作成直後 (2〜8 秒) に自発 walkthrough が出るよう戻った** (#494 / #502 / #507 /
//! #510 の初動タイムラインで実測、4/4)。
//!
//! この repo には「CodeRabbit に明示要求を投げる」経路が 3 つあり、いずれも
//! 「auto は効かない」という当時の前提に立って書かれていた。auto が効いている間は
//! 3 経路とも二重依頼になり、レビュー枠を無駄に消費する。
//!
//! | 経路 | 判定を使う場所 |
//! |---|---|
//! | [`super::trigger_review`] | PR 作成後の「🔍 Trigger review」チェック前 |
//! | [`super::review_trigger`] | auto-push 後の `@coderabbitai review` 投稿前 |
//! | [`super::poll::rate_limit`] | rate-limit reset 後の再投稿前 |
//!
//! 3 経路が同じ判定を写経すると、CodeRabbit 側の次の変化で**追随した経路と
//! しなかった経路が混在する** ([ADR-081](../../../../docs/adr/adr-081-single-fact-dispersion.md))。
//! 判定はこの module だけが持ち、3 経路は [`should_skip_request`] を呼ぶ。
//! 判定の本体 [`head_already_covered`] は module 内に閉じる。
//!
//! # review-request.yml との関係
//!
//! 4 つ目の経路である `.github/workflows/review-request.yml` は `pull_request_target`
//! で動き、**checkout もコード実行もしない**設計 (PAT を持つ job の信頼境界) のため
//! この関数を呼べない。同じ 2 系統を bash で照会する形になるが、両者がずれないことは
//! `scripts/lint-workflows.mjs` の契約検査 4 が集合比較で固定する
//! ([ADR-081](../../../../docs/adr/adr-081-single-fact-dispersion.md) 決定 1 の
//! 「機械で照合できる 1 種類は検査にする」線)。
//!
//! # fail-open
//!
//! 本 module の判定は**助言層**であり、ゲートではない。確証 (`Some(true)`) のときだけ
//! skip し、判定不能は投稿側へ倒す。誤って skip すると再レビューが恒常的に欠落するため、
//! [ADR-043](../../../../docs/adr/adr-043-security-gates-fail-closed.md) の fail-closed は
//! ここには適用しない。

use crate::log::log_info;

/// CodeRabbit が **レビュー完了** を示すときの commit status description (2026-09-21 実測)。
///
/// `state` は skip 時も `success` になるため、完了の判定にはこの文言が要る
/// ([`parse_commit_status_covered`] に実測した 4 種類の表がある)。
/// `.github/workflows/review-request.yml` の検証段が同じ文言を bash 側で見ており、
/// 両者の同期は `scripts/lint-workflows.mjs` の契約検査が固定する (ADR-081 決定 1)。
const STATUS_COMPLETED: &str = "Review completed";

/// 現 HEAD が既に CodeRabbit にレビュー済みか判定する (WP-05 follow-up / 順位258、再トリガー抑止)。
///
/// `Some(true)` = レビュー済み (skip) / `Some(false)` = 未レビュー確定 (投稿) /
/// `None` = 判定不能 (fail-open で投稿)。
///
/// 判定ソースは 2 系統 (順位258 で commit status を追加):
/// 1. **reviews API**: CodeRabbit の PR review は submit した commit を `commit_id` に持つため、
///    現 HEAD がいずれかの CodeRabbit review の `commit_id` と一致すればレビュー済み。
/// 2. **commit status**: CodeRabbit は「指摘ゼロ」で完了した (再) レビューでは formal review
///    object を提出せず、commit status (context `CodeRabbit` / state `success`) のみで完了を
///    通知する (2026-07-05 実測)。reviews API 単独では指摘ゼロ完了を検知できず (順位258 の動機)、
///    HEAD の combined status に CodeRabbit success があればレビュー済みとみなす。
///
/// 2 系統は [`combine_covered`] で fail-open 合成する: いずれかが確証 (`Some(true)`) なら skip、
/// 両方 `None` (判定不能) なら fail-open で投稿。gh 照会失敗・JSON parse 不能はいずれも `None` に
/// 倒し、確証がある `Some(true)` のときだけ skip して再レビュー欠落を招かない設計。
/// **HEAD の取得にも `--repo` を渡す。** `gh pr view` は `--repo` が無いと cwd の git
/// remote から対象リポジトリを解決するため、cwd が `repo` と別のリポジトリだと
/// 「別リポジトリの HEAD」を `repo` のレビュー集合と突き合わせることになり、判定が常に
/// 「未レビュー」へ倒れる。同 crate の `fetch_mergeable_status` (`poll/rate_limit.rs`) と
/// 同じ扱いに揃える。`gh` 呼び出しの `--repo` 欠落は本 repo で 3 度目の同型欠陥
/// (PR #512 の CodeRabbit 指摘、順位 502 が同じパターンの lint 化を持つ)。
fn head_already_covered(pr: u64, repo: &str) -> Option<bool> {
    let pr_str = pr.to_string();
    let head = crate::runner::run_gh_quiet(&[
        "pr", "view", &pr_str, "--repo", repo, "--json", "headRefOid", "--jq", ".headRefOid",
    ])?;
    let head = head.trim();
    if head.is_empty() {
        return None;
    }
    let via_reviews = covered_via_reviews_api(pr, repo, head);
    let via_status = covered_via_commit_status(repo, head);
    combine_covered(via_reviews, via_status)
}

/// [`head_already_covered`] を state から解決した `(pr, repo)` で呼び、skip 可否を返す。
///
/// 3 経路のうち 2 つ (`review_trigger` / `poll::rate_limit`) は `PrMonitorState` から
/// repo を解決するため、その定型をここに置く。`repo` 未確定は判定不能として `false`
/// (= 投稿する) に倒す。`reason` は log 行の前置きで、どの経路が skip したかを残す。
pub(crate) fn should_skip_request(pr: u64, repo: Option<&str>, reason: &str) -> bool {
    let Some(repo) = repo else {
        return false;
    };
    if head_already_covered(pr, repo) != Some(true) {
        return false;
    }
    log_info(&format!(
        "{reason} PR #{pr} の現 HEAD は既に CodeRabbit レビュー済みのため明示要求をスキップ (二重依頼抑止、レート消費回避)"
    ));
    true
}

/// reviews API 経由の「HEAD レビュー済み」判定。gh 照会失敗は `None` (判定不能)。
fn covered_via_reviews_api(pr: u64, repo: &str, head: &str) -> Option<bool> {
    let reviewed = crate::runner::run_gh_quiet(&[
        "api",
        "--paginate",
        &format!("repos/{}/pulls/{}/reviews", repo, pr),
        "--jq",
        r#".[] | select(.user.login=="coderabbitai[bot]") | .commit_id"#,
    ])?;
    Some(is_head_in_reviewed(head, &reviewed))
}

/// commit status 経由の「HEAD レビュー済み」判定。gh 照会失敗・JSON parse 不能は `None`。
fn covered_via_commit_status(repo: &str, head: &str) -> Option<bool> {
    let status_json =
        crate::runner::run_gh_quiet(&["api", &format!("repos/{}/commits/{}/status", repo, head)])?;
    parse_commit_status_covered(&status_json)
}

/// CodeRabbit がレビューした commit SHA 一覧 (改行区切り) に現 HEAD が含まれるかの純粋判定。
fn is_head_in_reviewed(head: &str, reviewed_commit_ids: &str) -> bool {
    reviewed_commit_ids
        .lines()
        .any(|line| line.trim() == head)
}

/// GitHub combined status API (`repos/{repo}/commits/{sha}/status`) の JSON から
/// **その commit が CodeRabbit のレビュー対象として片付いているか** を判定する純粋関数。
///
/// 「片付いている」= 完了済み **または進行中**。どちらでも明示要求を投げる理由が無い。
/// エントリはあるが該当なしは `Some(false)`。JSON parse 不能 / `statuses` 欠落は
/// `None` (fail-open)。
///
/// # 実測した 4 状態 (2026-09-21、PR #510 / #513 の status 履歴)
///
/// | state | description | 判定 |
/// |---|---|---|
/// | `success` | `Review completed` | 片付いている (完了) |
/// | `pending` | `Review in progress` | 片付いている (進行中) |
/// | `success` | `Review skipped: bot user not eligible for review` | **未レビュー** |
/// | `success` | `Review skipped: incremental reviews are disabled` | **未レビュー** |
///
/// **`state == "success"` だけでは足りない。** CodeRabbit は skip も success で通知する。
/// state だけを見ると後ろ 2 つも「レビュー済み」になり、レビューが走っていない PR で
/// 明示要求を skip してしまう (= 再レビューが永久に来ない)。
///
/// **完了の判定は「skip を除く」ではなく「完了を求める」形にする。** 除外リスト方式だと
/// CodeRabbit が新しい skip 文言を足したとき黙って陽性へ倒れる。未知の文言は
/// 「完了ではない」= 要求する側へ落ちる方が安全である。
///
/// **進行中は `state` だけで見る。** description まで求めないのは
/// `.github/workflows/review-request.yml` の autocheck と同じ判定に揃えるため —
/// 片方だけが文言に依存すると、CodeRabbit が進行中の文言を変えたときに 2 層の挙動が割れる。
///
/// なお**検証ゲート側 (workflow の検証段) は進行中を成功証拠にしない**。あちらは
/// 「レビューが完了したか」を問う層で、こちらは「これ以上要求する必要があるか」を問う
/// 助言層である (ADR-019 § auto レビューの再開と、挙動を実測で吸収する方針)。
fn parse_commit_status_covered(status_json: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(status_json).ok()?;
    let statuses = value.get("statuses")?.as_array()?;
    Some(statuses.iter().any(is_coderabbit_covering_status))
}

/// 単一 status エントリが「CodeRabbit が片付けている」を示すかの純粋判定。
fn is_coderabbit_covering_status(entry: &serde_json::Value) -> bool {
    let field = |key: &str| {
        entry
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    if !field("context").eq_ignore_ascii_case("CodeRabbit") {
        return false;
    }
    let in_progress = field("state") == "pending";
    let completed = field("state") == "success" && field("description").contains(STATUS_COMPLETED);
    in_progress || completed
}

/// reviews API と commit status の 2 系統の「レビュー済み」判定を fail-open で合成する純粋関数。
///
/// - いずれかが `Some(true)` (確証) → `Some(true)` (skip)。
/// - 両方 `None` (判定不能) → `None` (fail-open で投稿)。
/// - それ以外 (true なし、少なくとも一方が `Some(false)`) → `Some(false)` (未レビュー確定 → 投稿)。
///
/// caller は `== Some(true)` のときだけ skip するため `Some(false)` と `None` は同じ「投稿」に
/// 落ちるが、判定不能 (`None`) と未レビュー確定 (`Some(false)`) を型で区別し、ログ・テストで
/// fail-open 経路 (`None` → 投稿) を反転検査できるようにする (順位258 / 順位162 と同型)。
fn combine_covered(via_reviews: Option<bool>, via_status: Option<bool>) -> Option<bool> {
    match (via_reviews, via_status) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (None, None) => None,
        _ => Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_head_in_reviewed_detects_already_reviewed_head() {
        let reviewed = "abc1230000\ndef4560000\n789aaa0000";
        assert!(
            is_head_in_reviewed("def4560000", reviewed),
            "現 HEAD が CodeRabbit review 済み SHA 集合に含まれる → 再トリガー抑止"
        );
        assert!(
            is_head_in_reviewed("789aaa0000", "  789aaa0000  \nabc1230000"),
            "前後空白を trim して一致判定する"
        );
        assert!(
            !is_head_in_reviewed("999zzz0000", reviewed),
            "未レビューの新 HEAD は含まれない → 投稿する"
        );
        assert!(
            !is_head_in_reviewed("abc1230000", ""),
            "CodeRabbit review が無ければ false (投稿する)"
        );
    }

    #[test]
    fn parse_commit_status_detects_coderabbit_success() {
        let json = r#"{
            "state": "success",
            "statuses": [
                {"context": "ci/build", "state": "success"},
                {"context": "CodeRabbit", "state": "success", "description": "Review completed"}
            ]
        }"#;
        assert_eq!(
            parse_commit_status_covered(json),
            Some(true),
            "commit status に CodeRabbit success があれば指摘ゼロ完了でもレビュー済み"
        );
    }

    /// **skip も `success` で来る。** 2026-09-21 に PR #510 / #513 の status 履歴で実測した
    /// 3 つの非完了 description を固定する。`state` だけで判定する実装に戻すと、この
    /// テストが落ちる — 戻した場合はレビューが走っていない PR で明示要求を skip し、
    /// 再レビューが永久に来なくなる。
    #[test]
    fn parse_commit_status_rejects_success_states_that_are_not_completions() {
        for description in [
            "Review skipped: bot user not eligible for review",
            "Review skipped: incremental reviews are disabled",
        ] {
            let json = format!(
                r#"{{"statuses": [{{"context": "CodeRabbit", "state": "success", "description": "{description}"}}]}}"#
            );
            assert_eq!(
                parse_commit_status_covered(&json),
                Some(false),
                "success でも skip はレビュー済みではない: {description}"
            );
        }
    }

    /// 進行中の HEAD へ明示要求を重ねてもレート枠を消費するだけなので、`pending` も
    /// 「片付いている」に含める。`review-request.yml` の autocheck と同じ判定である。
    ///
    /// **description には依存しない。** 片方の層だけが進行中の文言に依存すると、
    /// CodeRabbit がその文言を変えたときに 2 層の挙動が割れる。
    #[test]
    fn parse_commit_status_treats_in_progress_as_covered() {
        let with_description =
            r#"{"statuses": [{"context": "CodeRabbit", "state": "pending", "description": "Review in progress"}]}"#;
        assert_eq!(
            parse_commit_status_covered(with_description),
            Some(true),
            "レビュー進行中は明示要求を投げない"
        );
        let without_description =
            r#"{"statuses": [{"context": "CodeRabbit", "state": "pending"}]}"#;
        assert_eq!(
            parse_commit_status_covered(without_description),
            Some(true),
            "進行中の判定は state だけを見る (workflow の autocheck と同じ)"
        );
    }

    /// description が欠落 / 未知の文言なら「完了ではない」へ倒す (要求する側 = 安全側)。
    #[test]
    fn parse_commit_status_rejects_unknown_or_missing_description() {
        let missing = r#"{"statuses": [{"context": "CodeRabbit", "state": "success"}]}"#;
        assert_eq!(
            parse_commit_status_covered(missing),
            Some(false),
            "description 欠落は完了と見なさない"
        );
        let unknown = r#"{"statuses": [{"context": "CodeRabbit", "state": "success", "description": "Review done"}]}"#;
        assert_eq!(
            parse_commit_status_covered(unknown),
            Some(false),
            "未知の文言は完了と見なさない (除外リスト方式なら黙って陽性に倒れる)"
        );
    }

    #[test]
    fn parse_commit_status_is_case_insensitive_for_context() {
        let json = r#"{"statuses": [{"context": "coderabbit", "state": "success", "description": "Review completed"}]}"#;
        assert_eq!(
            parse_commit_status_covered(json),
            Some(true),
            "context は ASCII 大文字小文字を無視して一致"
        );
    }

    #[test]
    fn parse_commit_status_not_reviewed_when_no_coderabbit_success() {
        let failure = r#"{"statuses": [{"context": "CodeRabbit", "state": "failure"}]}"#;
        assert_eq!(
            parse_commit_status_covered(failure),
            Some(false),
            "完了でも進行中でもない state は未レビュー確定"
        );
        let other = r#"{"statuses": [{"context": "ci/build", "state": "success"}]}"#;
        assert_eq!(
            parse_commit_status_covered(other),
            Some(false),
            "CodeRabbit context が無ければ未レビュー確定"
        );
        let empty = r#"{"statuses": []}"#;
        assert_eq!(
            parse_commit_status_covered(empty),
            Some(false),
            "statuses が空なら未レビュー確定"
        );
    }

    #[test]
    fn parse_commit_status_none_on_unparseable_or_missing_statuses() {
        assert_eq!(
            parse_commit_status_covered("not json"),
            None,
            "JSON parse 不能は None (fail-open で投稿)"
        );
        assert_eq!(
            parse_commit_status_covered(r#"{"state": "success"}"#),
            None,
            "statuses key 欠落は None (fail-open で投稿)"
        );
    }

    #[test]
    fn combine_covered_skips_when_either_source_confirms() {
        assert_eq!(
            combine_covered(Some(true), Some(false)),
            Some(true),
            "reviews API が確証 → skip"
        );
        assert_eq!(
            combine_covered(None, Some(true)),
            Some(true),
            "commit status が確証 (reviews API 判定不能でも) → skip"
        );
        assert_eq!(
            combine_covered(Some(true), None),
            Some(true),
            "reviews API 確証 (commit status 判定不能でも) → skip"
        );
    }

    /// fail-open 反転テスト (順位258 / 順位162 と同型): 両ソース判定不能 → None → 投稿。
    /// caller は `Some(true)` のときだけ skip するため、`None` が誤って `Some(true)` に反転すると
    /// 再レビューが恒常的に欠落する。この経路を明示的に固定する。
    #[test]
    fn combine_covered_fails_open_to_none_when_both_indeterminate() {
        assert_eq!(
            combine_covered(None, None),
            None,
            "両ソース判定不能 (gh 照会失敗 / parse 不能) は None を返し fail-open で投稿する"
        );
    }

    #[test]
    fn combine_covered_posts_when_confirmed_not_reviewed() {
        assert_eq!(
            combine_covered(Some(false), Some(false)),
            Some(false),
            "両ソースが未レビュー確定 → Some(false) で投稿"
        );
        assert_eq!(
            combine_covered(Some(false), None),
            Some(false),
            "一方が未レビュー確定・他方判定不能 → Some(false) で投稿"
        );
        assert_eq!(
            combine_covered(None, Some(false)),
            Some(false),
            "一方判定不能・他方が未レビュー確定 → Some(false) で投稿"
        );
    }

    /// repo 未確定は判定不能 — gh 照会を試みずに `false` (投稿する) へ倒す。
    /// ここが `true` に反転すると、state が壊れた夜に全経路が黙って skip する。
    #[test]
    fn should_skip_request_posts_when_repo_is_unknown() {
        assert!(
            !should_skip_request(1, None, "[test]"),
            "repo 未確定は判定不能 → skip せず投稿する"
        );
    }
}
