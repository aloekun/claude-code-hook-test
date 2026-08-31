//! open-questions gate stage (機2) — 未解決の設計の問いが残ったまま push されるのを止める。
//!
//! 設計と採否の記録先は [ADR-077](../../../../docs/adr/adr-077-open-questions-gate.md)、
//! 位置づけは `docs/defect-convergence-plan.md` § Phase 2。
//!
//! # 何を保証するか (そして何を保証しないか)
//!
//! 保証するのは「**書かれた問いは必ず push 前にユーザーへ届く**」ことだけである。
//! **「問うべきなのに書かなかった」は検出できない** — 問いが浮上すること自体は、
//! 機1 が促す pure 化作業 (I/O と判定を分ける過程で設計の穴が見える) に依存する。
//!
//! # 止め方
//!
//! `docs/open-questions.md` にエントリが 1 件でもあれば deny する。ファイル不在 /
//! エントリ 0 件は pass。**書式の不備を理由に通さない** ([`detect`] の module doc 参照)。

pub(crate) mod detect;

use std::path::Path;

use crate::config::OpenQuestionsGateConfig;
use crate::log::{log_info, log_stage};

use detect::{open_questions, OpenQuestion};

const STAGE: &str = "open-questions";
const OVERRIDE_ENV_VAR: &str = "OPEN_QUESTIONS_GATE_OVERRIDE";
const DOCUMENT_PATH: &str = "docs/open-questions.md";

/// push を続行してよいか。
pub(crate) fn run_open_questions_gate(config: Option<&OpenQuestionsGateConfig>) -> bool {
    run_open_questions_gate_with(config, |path| std::fs::read_to_string(path).ok())
}

/// 判定の本体 (ファイル読み取りは `read` から注入する)。
///
/// 読み取りを注入するのは、**「ファイルが無い」と「読めない」と「空」を区別した挙動**を
/// テストで固定するためである。`None` (不在 / 読めない) は pass に倒す — 問いが書かれて
/// いないのに push を止めると、gate を無効化する動機を作ってしまう。
fn run_open_questions_gate_with(
    config: Option<&OpenQuestionsGateConfig>,
    read: impl FnOnce(&Path) -> Option<String>,
) -> bool {
    if !enabled(config) {
        return true;
    }
    if lib_telemetry::is_truthy(std::env::var(OVERRIDE_ENV_VAR).unwrap_or_default().as_str()) {
        log_info(&format!(
            "open_questions_gate: {OVERRIDE_ENV_VAR} により検査を skip します"
        ));
        return true;
    }
    let Some(content) = read(Path::new(DOCUMENT_PATH)) else {
        log_stage(STAGE, "未解決の問いはありません (ファイル不在)");
        return true;
    };
    report(&open_questions(&content))
}

/// 既定は有効。`enabled = false` で恒久停止する ([ADR-039](../../../../docs/adr/adr-039-experimental-feature-standard-pattern.md))。
fn enabled(config: Option<&OpenQuestionsGateConfig>) -> bool {
    config.and_then(|c| c.enabled).unwrap_or(true)
}

fn report(questions: &[OpenQuestion]) -> bool {
    if questions.is_empty() {
        log_stage(STAGE, "未解決の問いはありません");
        return true;
    }
    log_stage(
        STAGE,
        &format!("未解決の問いが {} 件あります (機2):", questions.len()),
    );
    for q in questions {
        log_info(&format!("  Q-{}: {}", q.id, q.question));
        if let Some(related) = &q.related {
            log_info(&format!("    関連: {related}"));
        }
        log_info(&format!(
            "    仮定: {}",
            q.assumption.as_deref().unwrap_or("(未記入)")
        ));
        record_firing();
    }
    log_info(&format!(
        "  対処: 問いをユーザーへ確認し、回答を然るべき場所 (ADR / doc コメント / 計画書) へ書いてから\n  \
         {DOCUMENT_PATH} のエントリを削除してください。確認より先に push する必要がある場合は\n  \
         `{OVERRIDE_ENV_VAR}=1` で明示的にバイパスできます"
    ));
    false
}

/// 発火を telemetry へ記録する ([ADR-055](../../../../docs/adr/adr-055-firing-telemetry-collection.md))。
///
/// `id` は呼び出し側リテラルの固定カテゴリ名のみ (`testability_gate::record_firing` と同じ
/// 閉集合パターン)。`docs/open-questions.md` 由来の見出し `id` はユーザーの自由記述文字列
/// であり検証されていないため、telemetry へは埋め込まない (同 ADR § プライバシー:
/// 記録はメタデータのみ)。問い 1 件ごとに 1 回発火を記録する。
fn record_firing() {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "cli-push-runner",
        kind: lib_telemetry::FiringKind::Hook,
        id: "open_questions_gate:fired",
        decision: lib_telemetry::Decision::Block,
        session_id: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(enabled: Option<bool>) -> OpenQuestionsGateConfig {
        OpenQuestionsGateConfig { enabled }
    }

    const ONE_QUESTION: &str = "## Q-1: 問い\n\n関連: a.rs\n仮定: A\n";

    #[test]
    fn a_document_with_questions_blocks_the_push() {
        assert!(!run_open_questions_gate_with(None, |_| Some(
            ONE_QUESTION.to_string()
        )));
    }

    #[test]
    fn a_document_without_questions_passes() {
        assert!(run_open_questions_gate_with(None, |_| Some(
            "# 未解決の問い\n\n(現在なし)\n".to_string()
        )));
    }

    /// ファイル不在は pass。問いが書かれていないのに止めると、gate を切る動機を作る。
    #[test]
    fn a_missing_document_passes() {
        assert!(run_open_questions_gate_with(None, |_| None));
    }

    /// 既定は有効 (config 不在でも問いがあれば止まる)。
    #[test]
    fn the_gate_is_enabled_by_default() {
        assert!(enabled(None));
        assert!(enabled(Some(&config(None))));
    }

    /// `enabled = false` は恒久停止 (kill-switch)。
    #[test]
    fn disabling_the_gate_skips_the_check() {
        assert!(run_open_questions_gate_with(
            Some(&config(Some(false))),
            |_| Some(ONE_QUESTION.to_string())
        ));
    }

    /// 読み取り対象は `docs/open-questions.md` に固定されている。
    #[test]
    fn the_gate_reads_the_documented_path() {
        let mut seen = None;
        let _ = run_open_questions_gate_with(None, |path| {
            seen = Some(path.to_path_buf());
            None
        });
        assert_eq!(seen.as_deref(), Some(Path::new(DOCUMENT_PATH)));
    }

    #[test]
    fn the_report_lists_every_question() {
        let questions = open_questions("## Q-1: 一つ目\n\n## Q-2: 二つ目\n");
        assert_eq!(questions.len(), 2);
        assert!(!report(&questions));
        assert!(report(&[]));
    }
}
