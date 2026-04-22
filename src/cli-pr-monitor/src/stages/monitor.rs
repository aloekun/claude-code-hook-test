use crate::config::load_config;
use crate::fix_commit::{create_fix_commit, FixCommitState};
use crate::log::{log_info, truncate_safe};
use crate::stages::collect::collect_findings;
use crate::stages::poll::run_poll_loop;
use crate::stages::repush::execute_repush_flow;
use crate::stages::takt::run_takt;
use crate::state::{write_state, PrMonitorState};
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

    // 早期 reset は run_create_pr 冒頭で実施済み (gh pr create 実行前)。
    // ここは run_monitor_only 経路および冪等化のための最終 reset。
    // poll_loop 内では iteration を跨いで notified を preserve する。
    let init_state = PrMonitorState::new(
        pr_info.pr_number,
        pr_info.repo.clone(),
        pr_info.push_time.clone().unwrap_or_default(),
    );
    if let Err(e) = write_state(&init_state) {
        log_info(&format!("[state] 初期化書き込み失敗 (継続): {}", e));
    }

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
    // takt 実行前の @ commit id (取れない/takt 未実行なら None)。
    // 二段構え re-push 判定のため run_takt の前に捕捉する。
    // fix commit を pre-create した場合は、この cid は空 child を指す。
    let mut pre_takt_cid: Option<String> = None;
    // ADR task 4 (2026-04-20): 分離型 fix commit の状態
    let mut fix_state = FixCommitState::None;

    if has_coderabbit_findings {
        if !collect_findings(&poll_result) {
            log_info("review-comments.json 書き出し失敗 (takt 分析をスキップ)");
        } else if let Some(takt_config) = &config.takt {
            // ADR task 4
            fix_state = create_fix_commit(pr_info.pr_number, &poll_result.findings);

            // Stage 3: takt analysis + fix loop
            pre_takt_cid = crate::runner::capture_commit_id();
            log_info(&format!("[state] pre_takt_commit_id: {:?}", pre_takt_cid));
            takt_succeeded = run_takt(takt_config);
            log_info(&format!("[state] takt_succeeded: {}", takt_succeeded));
            if !takt_succeeded {
                log_info("takt ワークフロー失敗 (非致命的: ポーリング結果はそのまま報告)");
            }
        } else {
            log_info("takt 設定なし: AI 分析をスキップ");
        }
    }

    // Stage 4: re-push (fix_state ごとに分岐)
    // 1) commit id 変化 + 実 diff 非空 = HasChange のみ push 対象
    // 2) さらに auto_push_severity 設定で自動 push 可否を判定
    // 3) fix_state::Created かつ NoChange の場合は空 child を abandon で片付ける
    if takt_succeeded && has_coderabbit_findings {
        execute_repush_flow(&config.fix, &pr_label, pre_takt_cid.as_deref(), &fix_state);
    } else if let FixCommitState::Created { commit_id } = &fix_state {
        // takt が実行されなかった / 失敗した場合: 事前に作った fix child を片付ける。
        crate::fix_commit::try_abandon_empty_fix_commit("takt 未完了:", Some(commit_id));
    }

    // Stage 5: report to stdout
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
