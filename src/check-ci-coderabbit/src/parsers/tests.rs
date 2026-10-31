use super::*;

/// PR #342 の実 rollup 形状 (CheckRun / 大文字 conclusion) をそのまま固定する。
/// 名前も検証するのは、正規化が `name` を取りこぼすと runs が `(unnamed check)` の
/// 羅列になり「見えているのに中身が空」の状態を見逃すため。
#[test]
fn ci_rollup_checkrun_all_success() {
    let json = r#"[
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"CheckRun","name":"rust (windows-latest)","status":"COMPLETED","conclusion":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(json);
    assert_eq!(ci.overall, "success");
    assert_eq!(ci.runs.len(), 2);
    assert_eq!(ci.runs[0].name, "rust (ubuntu-latest)");
    assert_eq!(ci.runs[0].conclusion, "success");
}

#[test]
fn ci_rollup_one_failure() {
    let json = r#"[
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"CheckRun","name":"rust (windows-latest)","status":"COMPLETED","conclusion":"FAILURE"}
    ]"#;
    assert_eq!(parse_ci_rollup(json).overall, "failure");
}

/// 実行中の CheckRun は conclusion が null。`status` へフォールバックして pending と
/// 判定できること (ここが抜けると実行中の check が success に化ける)。
#[test]
fn ci_rollup_in_progress_falls_back_to_status() {
    let json = r#"[
        {"__typename":"CheckRun","name":"rust (windows-latest)","status":"IN_PROGRESS","conclusion":null},
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(json);
    assert_eq!(ci.overall, "pending");
    assert_eq!(ci.runs[0].conclusion, "in_progress");
}

/// 旧 commit status API 由来の `StatusContext` は `context` / `state` を使う。
#[test]
fn ci_rollup_status_context_uses_context_and_state() {
    let json = r#"[
        {"__typename":"StatusContext","context":"legacy","state":"SUCCESS"}
    ]"#;
    let ci = parse_ci_rollup(json);
    assert_eq!(ci.overall, "success");
    assert_eq!(ci.runs[0].name, "legacy");
}

#[test]
fn ci_rollup_status_context_error_is_failure() {
    let json = r#"[{"__typename":"StatusContext","context":"legacy","state":"ERROR"}]"#;
    assert_eq!(parse_ci_rollup(json).overall, "failure");
}

/// SIM-NEW-parsers-L18 fix: CodeRabbit 自身の commit status
/// (`StatusContext.context` に "CodeRabbit" を含む) は `ci.overall` の算出対象から
/// 除外すること。CR の commit status はレート制限中でも pass/error が生の値のまま
/// 混入しうる (PR #307/#309) ため、単独では `ci.overall` の CI failure 最優先分岐
/// (`decide()`) を素通りしてしまう。除外後は他に check が無ければ空 rollup 扱いで
/// `pending` になる。
#[test]
fn ci_rollup_excludes_coderabbit_status_context_from_overall() {
    let json = r#"[
        {"__typename":"StatusContext","context":"CodeRabbit","state":"ERROR"}
    ]"#;
    let ci = parse_ci_rollup(json);
    assert_eq!(
        ci.overall, "pending",
        "CodeRabbit 自身の commit status を ci.overall に混ぜてはならない"
    );
    assert!(ci.runs.is_empty());
}

/// 上記の除外が他の (正規の) check の判定を巻き込まないことを確認する:
/// CodeRabbit の StatusContext が ERROR でも、Actions の CheckRun が success なら
/// `ci.overall` はその success をそのまま反映する。
#[test]
fn ci_rollup_excludes_coderabbit_status_context_but_keeps_other_checks() {
    let json = r#"[
        {"__typename":"CheckRun","name":"rust (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"StatusContext","context":"CodeRabbit","state":"ERROR"}
    ]"#;
    let ci = parse_ci_rollup(json);
    assert_eq!(
        ci.overall, "success",
        "CodeRabbit の commit status (ERROR) を除外すれば残る CheckRun の success がそのまま overall になる"
    );
    assert_eq!(ci.runs.len(), 1);
    assert_eq!(ci.runs[0].name, "rust (ubuntu-latest)");
}

/// check がまだ 1 件も作られていない状態を success と報告しないこと (ADR-064)。
#[test]
fn ci_rollup_empty_is_pending_not_success() {
    let ci = parse_ci_rollup("[]");
    assert_eq!(ci.overall, "pending");
    assert!(ci.runs.is_empty());
}

/// `gh -q .statusCheckRollup` は rollup 不在時に `null` を返す。パース失敗時も
/// success へ倒れないこと。
#[test]
fn ci_rollup_null_is_pending() {
    assert_eq!(parse_ci_rollup("null").overall, "pending");
}

#[test]
fn ci_rollup_cancelled_is_failure() {
    let json = r#"[{"__typename":"CheckRun","name":"deploy","status":"COMPLETED","conclusion":"CANCELLED"}]"#;
    assert_eq!(parse_ci_rollup(json).overall, "failure");
}

/// skipped / neutral は GitHub 自身も PR を赤にしない。失敗に数えないこと。
#[test]
fn ci_rollup_skipped_and_neutral_are_not_failures() {
    let json = r#"[
        {"__typename":"CheckRun","name":"optional","status":"COMPLETED","conclusion":"SKIPPED"},
        {"__typename":"CheckRun","name":"advisory","status":"COMPLETED","conclusion":"NEUTRAL"}
    ]"#;
    assert_eq!(parse_ci_rollup(json).overall, "success");
}

#[test]
fn cr_status_success() {
    let json = r#"[
        {"context": "CodeRabbit", "state": "success"}
    ]"#;
    assert_eq!(parse_coderabbit_status(json), "success");
}

#[test]
fn cr_status_pending() {
    let json = r#"[
        {"context": "CodeRabbit", "state": "pending"}
    ]"#;
    assert_eq!(parse_coderabbit_status(json), "pending");
}

#[test]
fn cr_status_not_found() {
    let json = r#"[
        {"context": "ci/build", "state": "success"}
    ]"#;
    assert_eq!(parse_coderabbit_status(json), "not_found");
}

#[test]
fn cr_status_empty() {
    assert_eq!(parse_coderabbit_status("[]"), "not_found");
}

#[test]
fn cr_status_multiple_takes_first() {
    let json = r#"[
        {"context": "CodeRabbit", "state": "success"},
        {"context": "CodeRabbit", "state": "pending"}
    ]"#;
    assert_eq!(parse_coderabbit_status(json), "success");
}

/// 順位 213 (PR #213 post-merge-feedback T2-1 採用): GitHub commit statuses API は
/// reverse-chronological 返却 (新しい→古い順) であり、`parse_coderabbit_status` は
/// `.first()` で「最新 state」を取得する。この semantics を test 名 + doc comment +
/// assert message で explicit に固定し、refactor 時 (例: `.first()` → `.last()`) に
/// 意図しない意味変更を機械的に検出する。
///
/// 由来: PR #213 takt-fix iter 2 で `.last()` (= 最古 state を選んでいた) → `.first()`
/// の semantic fix が行われたが、元 production code (main.rs 時代) のコメントが
/// 「最新エントリ」と書きながら `.last()` を使っていた drift 事例。test 表現で固定する。
#[test]
fn cr_status_reverse_chronological_picks_first() {
    let json = r#"[
        {"context": "CodeRabbit", "state": "success"},
        {"context": "CodeRabbit", "state": "pending"}
    ]"#;
    assert_eq!(
        parse_coderabbit_status(json),
        "success",
        "GitHub statuses API は reverse-chronological 返却のため、配列先頭 (`.first()`) が最新 state。`.last()` (古い state) を返すと CR レビュー終了を見逃す"
    );
}

/// 順位258 harm #2 回帰: PR #247 head の実測 commit statuses (reverse-chronological,
/// pending/success 混在の 6 件) を与え、`.first()` = 最新 "Review completed" success を
/// 返すことを固定する。正しい head SHA さえ `fetch_coderabbit_commit_state` に渡れば
/// この list から "success" を読み取り `decide()` が park ループを止められることを示す
/// (harm #2 の真因は SHA 取得経路 `get_head_sha` であり、parse 側ではないことの根拠)。
#[test]
fn cr_status_pr247_real_shape_picks_latest_success() {
    let json = r#"[
        {"context": "CodeRabbit", "description": "Review completed", "state": "success"},
        {"context": "CodeRabbit", "description": "Review in progress", "state": "pending"},
        {"context": "CodeRabbit", "description": "Review completed", "state": "success"},
        {"context": "CodeRabbit", "description": "Review in progress", "state": "pending"},
        {"context": "CodeRabbit", "description": "Review skipped: incremental reviews are disabled", "state": "success"},
        {"context": "CodeRabbit", "description": "Review queued", "state": "pending"}
    ]"#;
    assert_eq!(
        parse_coderabbit_status(json),
        "success",
        "指摘ゼロ完了の実測 status list 先頭は success。正しい SHA を渡せば完了検知できる"
    );
}

#[test]
fn walkthrough_clean_detected_when_marker_present_with_header() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"},
         "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nNo actionable comments were generated in the recent review. 🎉",
         "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert!(parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn walkthrough_clean_skipped_when_rate_limit_overlay_present() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"},
         "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nrate limited by coderabbit.ai\nMore reviews will be available in 1 minute and 30 seconds.\nNo actionable comments were generated in the recent review.",
         "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert!(!parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn walkthrough_clean_skipped_when_marker_missing() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"},
         "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nReview summary: 3 changes detected.\n## Walkthrough\n...",
         "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert!(!parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn walkthrough_clean_skipped_when_header_missing_to_avoid_user_post_false_positive() {
    let json = r#"[
        {"user": {"login": "humanreviewer"},
         "body": "Quoting CR: No actionable comments were generated in the recent review.",
         "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert!(!parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn walkthrough_clean_skipped_when_coderabbitai_post_lacks_header_marker() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"},
         "body": "Plain text without header.\nNo actionable comments were generated in the recent review.",
         "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert!(!parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn walkthrough_clean_skipped_when_event_time_before_push_time() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"},
         "body": "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\nNo actionable comments were generated in the recent review.",
         "created_at": "2026-04-01T11:00:00Z"}
    ]"#;
    assert!(!parse_walkthrough_clean_marker(
        json,
        "2026-04-01T12:00:00Z"
    ));
}

#[test]
fn comments_filters_by_time() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "_old comment", "created_at": "2026-04-01T10:00:00Z"},
        {"user": {"login": "coderabbitai[bot]"}, "body": "_new comment", "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
}

#[test]
fn comments_filters_by_user() {
    let json = r#"[
        {"user": {"login": "someuser"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"},
        {"user": {"login": "coderabbitai[bot]"}, "body": "_comment", "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
}

#[test]
fn comments_filters_coderabbit_user_only() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "Summary of changes", "created_at": "2026-04-01T12:30:00Z"},
        {"user": {"login": "coderabbitai[bot]"}, "body": "<!-- auto-generated -->", "created_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 2);
}

#[test]
fn comments_empty() {
    assert_eq!(parse_new_comments("[]", "2026-04-01T12:00:00Z"), 0);
}

#[test]
fn comments_excludes_review_in_progress() {
    let json = r#"[
        {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:00:00Z","body":"<!-- review in progress by coderabbit.ai -->\nCurrently processing..."},
        {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:05:00Z","body":"_Actionable comments posted: 2_\nReview summary..."}
    ]"#;
    assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
}

/// rate-limit comment は new_comments から除外する
/// (CR review feedback PR #97 round 2: rate-limit が new_comments を汚染すると
///  decide() が action_required を早期 return して rate-limit retry 経路に入らない)
#[test]
fn comments_excludes_rate_limit() {
    let json = r#"[
        {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:00:00Z","body":"Rate limit exceeded\nPlease wait 5 minutes and 0 seconds before requesting another review."},
        {"user":{"login":"coderabbitai[bot]"},"created_at":"2026-04-01T13:05:00Z","body":"_Actionable comments posted: 2_\nReview summary..."}
    ]"#;
    assert_eq!(parse_new_comments(json, "2026-04-01T12:00:00Z"), 1);
}

#[test]
fn actionable_extracts_count() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "Some review\nActionable comments posted: 3\nMore text", "submitted_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert_eq!(
        parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
        Some(3)
    );
}

#[test]
fn actionable_no_match() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "No actionable items", "submitted_at": "2026-04-01T12:30:00Z"}
    ]"#;
    assert_eq!(
        parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
        None
    );
}

#[test]
fn actionable_filters_by_time() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 5", "submitted_at": "2026-04-01T10:00:00Z"}
    ]"#;
    assert_eq!(
        parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
        None
    );
}

/// 順位 214 (PR #213 post-merge-feedback T2-2 採用): `parse_actionable_comments` の
/// `submitted_at >= push_time` inclusive 比較を境界で固定する。`>` (exclusive) に
/// 戻された際にこの test が落ちる構造で、direct push 直後の CR review が同時刻
/// イベントとして報告される edge case の retest 取りこぼしを機械的に防ぐ。
///
/// 由来: PR #213 takt-fix iter 2 で `t > push_time` → `t >= push_time` の境界 fix。
/// 既存 rule⑦ `no-time-field-strict-greater` は `submitted_at` 名直接使用 ケースのみ
/// catch し、変数名 `t` に抽出された後は static lint 不能。test による second defense layer。
#[test]
fn actionable_includes_review_at_exact_push_time() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 3", "submitted_at": "2026-04-01T12:00:00Z"}
    ]"#;
    assert_eq!(
        parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
        Some(3),
        "submitted_at == push_time の review は inclusive 比較で含むべき (`>=` が `>` に戻されると取りこぼす)"
    );
}

/// 順位 214 (negative sentinel): 配列 latest 位置に「push 以前の sentinel 99」を
/// 置き、time filter が壊れた場合 `rfind` がそれを先に返す構造にする。time filter が
/// 正しく動くと sentinel は除外され、配列前方の `>=` 適合 review (= 5) が選ばれる。
/// 既存 `actionable_filters_by_time` は単一 review のみで test するが、本 test は
/// 「複数 review 中で boundary 適合のみを選ぶ」exclusion 挙動を sentinel pre-populate
/// で固定する (memory `feedback_test_dry_antipattern.md` 適用、独立 setup)。
#[test]
fn actionable_excludes_review_before_push_time() {
    let json = r#"[
        {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 5", "submitted_at": "2026-04-01T12:30:00Z"},
        {"user": {"login": "coderabbitai[bot]"}, "body": "Actionable comments posted: 99", "submitted_at": "2026-04-01T11:00:00Z"}
    ]"#;
    assert_eq!(
        parse_actionable_comments(json, "2026-04-01T12:00:00Z"),
        Some(5),
        "submitted_at < push_time の sentinel review (Actionable: 99) は time filter で除外され、push_time 以降の review (Actionable: 5) が選ばれるべき。99 が返れば time filter が機能していない"
    );
}

#[test]
fn extract_count_from_body() {
    assert_eq!(
        extract_actionable_count("Actionable comments posted: 7"),
        Some(7)
    );
}

#[test]
fn extract_count_zero() {
    assert_eq!(
        extract_actionable_count("Actionable comments posted: 0"),
        Some(0)
    );
}

#[test]
fn extract_count_not_found() {
    assert_eq!(extract_actionable_count("No issues found"), None);
}

#[test]
fn unresolved_threads_counts() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {"isResolved": false},
                            {"isResolved": true},
                            {"isResolved": false}
                        ]
                    }
                }
            }
        }
    }"#;
    assert_eq!(parse_unresolved_threads(json), Some(2));
}

#[test]
fn unresolved_threads_all_resolved() {
    let json = r#"{
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "nodes": [
                            {"isResolved": true}
                        ]
                    }
                }
            }
        }
    }"#;
    assert_eq!(parse_unresolved_threads(json), Some(0));
}

#[test]
fn unresolved_threads_invalid_json() {
    assert_eq!(parse_unresolved_threads("{}"), None);
}
