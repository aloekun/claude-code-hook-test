use crate::config::load_config;
use crate::log::log_info;
use crate::stages::collect::collect_findings;
use crate::stages::poll::run_poll_loop;
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

    if has_coderabbit_findings {
        if !collect_findings(&poll_result) {
            log_info("review-comments.json 書き出し失敗 (takt 分析をスキップ)");
        } else if let Some(takt_config) = &config.takt {
            // Stage 3: takt analysis
            if !run_takt(takt_config) {
                log_info("takt 分析失敗 (非致命的: ポーリング結果はそのまま報告)");
            }
        } else {
            log_info("takt 設定なし: AI 分析をスキップ");
        }
    }

    // Stage 4: report to stdout
    print_report(&poll_result, &pr_label);

    0
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

// ─── レポート出力 ───

fn print_report(result: &crate::stages::poll::PollResult, pr_label: &str) {
    let ci_status = result
        .ci
        .as_ref()
        .map(|c| c.overall.as_str())
        .unwrap_or("unknown");

    let cr_status = result
        .coderabbit
        .as_ref()
        .map(|c| {
            format!(
                "新規コメント{}件, 未解決スレッド{}件",
                c.new_comments,
                c.unresolved_threads.unwrap_or(0)
            )
        })
        .unwrap_or_else(|| "unknown".into());

    let findings_count = result.findings.len();

    println!();
    println!("=== {} 監視完了 ===", pr_label);
    println!("CI: {}", ci_status);
    println!("CodeRabbit: {}", cr_status);
    println!("action: {}", result.action);
    println!("summary: {}", result.summary);

    if findings_count > 0 {
        println!();
        println!("--- findings ({} 件) ---", findings_count);
        for f in &result.findings {
            println!(
                "  [{}] {}:{} - {} ({})",
                f.severity, f.file, f.line, f.issue, f.source
            );
        }
    }
}
