use crate::config::load_config;
use crate::log::{log_info, truncate_safe};
use crate::stages::collect::collect_findings;
use crate::stages::poll::run_poll_loop;
use crate::stages::push::run_push;
use crate::stages::takt::run_takt;
use crate::util::{get_pr_info, utc_now_iso8601, PrInfo};

// ─── 監視開始 (sequential chain) ───

pub(crate) fn start_monitoring(pr_info: &PrInfo) -> i32 {
    let config = load_config();

    if !config.monitor.enabled {
        log_info("監視は設定で無効化されています");
        return 0;
    }

    let pr_label = pr_info
        .pr_number
        .map(|n| format!("PR #{}", n))
        .unwrap_or_else(|| "PR".to_string());

    log_info(&format!("{} の監視を開始", pr_label));

    // Stage 1: poll_loop (in-process, blocking)
    let poll_result = run_poll_loop(&config.monitor, pr_info);

    log_info(&format!(
        "ポーリング完了: action={}, summary={}",
        poll_result.action, poll_result.summary
    ));

    // Stage 2: collect_findings -> .takt/review-comments.json
    // takt 分析は CodeRabbit 起因のシグナルに限定する (CI-only 失敗では起動しない)
    let has_coderabbit_findings = !poll_result.findings.is_empty()
        || poll_result
            .coderabbit
            .as_ref()
            .map(|c| c.new_comments > 0 || c.unresolved_threads.unwrap_or(0) > 0)
            .unwrap_or(false);

    let mut takt_succeeded = false;

    if has_coderabbit_findings {
        if !collect_findings(&poll_result) {
            log_info("review-comments.json 書き出し失敗 (takt 分析をスキップ)");
        } else if let Some(takt_config) = &config.takt {
            // Stage 3: takt analysis + fix loop
            takt_succeeded = run_takt(takt_config);
            if !takt_succeeded {
                log_info("takt ワークフロー失敗 (非致命的: ポーリング結果はそのまま報告)");
            }
        } else {
            log_info("takt 設定なし: AI 分析をスキップ");
        }
    }

    // Stage 4: re-push (ハイブリッド)
    // takt が fix を実行した場合、変更の有無を確認して re-push を判断
    if takt_succeeded && has_coderabbit_findings {
        handle_repush(&config.fix, &pr_label);
    }

    // Stage 5: report to stdout
    print_report(&poll_result, &pr_label);

    0
}

// ─── re-push ハンドラ ───

/// auto_push_severity 設定値から自動 push するか否かを返す。
/// "critical" / "major" => true、"none" => false、未知値 => false (警告ログあり)
pub(crate) fn should_auto_push(setting: &str) -> bool {
    match setting {
        "none" => false,
        "critical" | "major" => true,
        other => {
            log_info(&format!(
                "auto_push_severity に未知の値 '{}' が指定されています。'none' として扱い自動 push をスキップします",
                other
            ));
            false
        }
    }
}

fn handle_repush(fix_config: &crate::config::FixConfig, pr_label: &str) {
    // jj diff で実際のコード変更があるか確認
    let (ok, diff_output) = crate::runner::run_cmd_direct("jj", &["diff", "--stat"], &[], 30);
    if !ok {
        log_info(&format!(
            "jj diff --stat 実行失敗: re-push を中断します\n{}",
            diff_output.trim()
        ));
        return;
    }
    if diff_output.trim().is_empty() {
        log_info("takt fix 後の変更なし: re-push スキップ");
        return;
    }

    log_info(&format!(
        "takt fix による変更を検出:\n{}",
        diff_output.trim()
    ));

    // 自動 push 判定: takt が fix を実行し jj diff に変更がある場合のみ re-push 対象
    // NOTE: takt の fitness filter が not_applicable な指摘を除外済みのため、
    // ここでは生 findings ではなく auto_push_severity 設定のみで判断する
    let auto_push = should_auto_push(fix_config.auto_push_severity.as_str());

    if auto_push {
        log_info(&format!(
            "{} の takt fix 修正を自動 re-push します",
            pr_label
        ));
        if run_push(fix_config) {
            log_info("自動 re-push 完了");
        } else {
            log_info("自動 re-push 失敗 (手動対応が必要です)");
        }
    } else {
        log_info("修正内容はコミット済みですが、re-push はユーザー確認待ちです");
        log_info("確認後に pnpm push を実行してください");
    }
}

// ─── 監視のみモード ───

pub(crate) fn run_monitor_only() -> i32 {
    let config = load_config();

    if !config.monitor.enabled {
        return 0;
    }

    let mut pr_info = get_pr_info();

    if pr_info.pr_number.is_none() {
        log_info("PR が存在しないため、監視をスキップします");
        return 0;
    }

    log_info("監視のみモード (既存 PR 検出)");

    pr_info.push_time = Some(utc_now_iso8601());
    start_monitoring(&pr_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_auto_push_none_returns_false() {
        assert!(!should_auto_push("none"));
    }

    #[test]
    fn should_auto_push_critical_returns_true() {
        assert!(should_auto_push("critical"));
    }

    #[test]
    fn should_auto_push_major_returns_true() {
        assert!(should_auto_push("major"));
    }

    #[test]
    fn should_auto_push_unknown_value_returns_false() {
        // タイポや未知値は fail-closed: 自動 push しない
        assert!(!should_auto_push("non"));
        assert!(!should_auto_push("Critical"));
        assert!(!should_auto_push(""));
    }
}

// ─── レポート出力 ───

fn print_report(result: &crate::stages::poll::PollResult, pr_label: &str) {
    let ci_status = result
        .ci
        .as_ref()
        .map(|c| c.overall.as_str())
        .unwrap_or("unknown");

    let cr_comments = result
        .coderabbit
        .as_ref()
        .map(|c| c.new_comments)
        .unwrap_or(0);
    let cr_threads = result
        .coderabbit
        .as_ref()
        .and_then(|c| c.unresolved_threads)
        .unwrap_or(0);

    let findings_count = result.findings.len();
    let critical_major = result
        .findings
        .iter()
        .filter(|f| {
            let s = f.severity.to_lowercase();
            s == "critical" || s == "high" || s == "major"
        })
        .count();

    // 判定
    let verdict = if critical_major > 0 {
        "修正が必要な指摘があります"
    } else if findings_count > 0 {
        "重大な問題は見つかりませんでした。軽微な改善提案があります"
    } else {
        "問題は見つかりませんでした"
    };

    // 統合レポート形式 (post-pr-create-review-check スキルと同一フォーマット)
    println!();
    println!("## Review Report ({})", pr_label);
    println!();
    println!(
        "CI: {} | CodeRabbit: 新規コメント{}件, 未解決スレッド{}件",
        ci_status, cr_comments, cr_threads
    );
    println!("action: {} | summary: {}", result.action, result.summary);
    println!();
    println!("**判定**: {}", verdict);

    if findings_count > 0 {
        println!();
        println!("| # | Source | Severity | File (Line) | Issue | Suggestion |");
        println!("|---|--------|----------|-------------|-------|------------|");
        for (i, f) in result.findings.iter().enumerate() {
            // suggestion を 80 文字 (char 単位) で切り詰め (UTF-8 安全)
            let suggestion = if f.suggestion.chars().count() > 80 {
                format!("{}...", truncate_safe(&f.suggestion, 77))
            } else {
                f.suggestion.clone()
            };
            println!(
                "| {} | {} | {} | {} ({}) | {} | {} |",
                i + 1,
                f.source,
                f.severity,
                f.file,
                f.line,
                f.issue,
                suggestion
            );
        }
    }
}
