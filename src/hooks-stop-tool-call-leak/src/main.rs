//! Stop tool call leak 検知フック (ADR-053)
//!
//! Claude Code がツール呼び出しを正規の tool_use block ではなくテキスト領域に
//! `<invoke name="...">...</invoke>` の生 XML として出力し、実行されないまま
//! turn が終了する不具合を Stop 時に検知し、`decision: block` で正規の
//! ツール呼び出しによる再実行を促す。
//!
//! 設計判断 (ADR-053):
//! - **`stop_hook_active` skip は不採用** (ADR-004 からの意図的逸脱)。
//!   品質ゲート block 後の retry 中に発生した leak を取り逃がさないため、および
//!   再 leak 実績 (実データ) があるため。無限ループ防止は連続 leak カウント上限
//!   (`max_consecutive_blocks`、既定 3) 到達での fail-open で担保する。
//! - **エラー時 fail-open** (ADR-043 からの意図的逸脱)。transcript 読み取り不能で
//!   fail-closed (block) にすると連続カウントも取得できず無限ブロックに陥るため。
//!   本 hook はセキュリティゲートではなく UX 復旧装置である。
//! - **ADR-039 experimental pattern**: config opt-in (code default OFF) +
//!   kill-switch (`enabled = false` / env `STOP_TOOL_CALL_LEAK_OVERRIDE`) +
//!   bounded lifetime (上流修正確認 or leak 4 週間非観測で撤去判定)。

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

mod detect;
mod transcript;

use transcript::{parse_tail_entries, scan_tail, TailScan};

/// 緊急バイパス用 env var (kill-switch)。truthy 値で検査を skip する。
const OVERRIDE_ENV_VAR: &str = "STOP_TOOL_CALL_LEAK_OVERRIDE";

/// 連続 block 上限の既定値。到達で fail-open (stderr 警告 + 停止許可)。
const DEFAULT_MAX_CONSECUTIVE_BLOCKS: u32 = 3;

/// transcript 末尾から読むエントリ行数。leak は必ず turn 終端に位置するため
/// (ADR-053 §調査結果)、末尾のみで判定できる。
const TAIL_LINES: usize = 200;

/// Stop hook 入力 (必要なフィールドのみ)
#[derive(Deserialize)]
struct HookInput {
    transcript_path: Option<String>,
}

/// block 判定の出力
#[derive(Serialize)]
struct BlockDecision {
    decision: String,
    reason: String,
}

/// hooks-config.toml のうち本 hook が参照する section のみ部分デシリアライズ
#[derive(Deserialize, Default)]
struct ConfigFile {
    stop_tool_call_leak: Option<LeakConfig>,
}

/// `[stop_tool_call_leak]` section (ADR-039: code default は disabled)
#[derive(Deserialize, Default)]
struct LeakConfig {
    enabled: Option<bool>,
    max_consecutive_blocks: Option<u32>,
}

fn main() {
    if kill_switch_active() {
        return;
    }
    let config = load_config().stop_tool_call_leak.unwrap_or_default();
    if !config.enabled.unwrap_or(false) {
        return;
    }
    let Some(transcript_path) = read_transcript_path_from_stdin() else {
        return;
    };
    let max_blocks = config
        .max_consecutive_blocks
        .unwrap_or(DEFAULT_MAX_CONSECUTIVE_BLOCKS);
    run_check(Path::new(&transcript_path), max_blocks);
}

/// override env の受理値判定 (FILE_LENGTH_CHECK_OVERRIDE と同 pattern)
fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// kill-switch env が設定されていれば skip (stderr に明示)
fn kill_switch_active() -> bool {
    match std::env::var(OVERRIDE_ENV_VAR) {
        Ok(value) if is_truthy(&value) => {
            eprintln!(
                "[stop-tool-call-leak] {} が設定されているため検査を skip します",
                OVERRIDE_ENV_VAR
            );
            true
        }
        _ => false,
    }
}

/// exe と同じディレクトリの hooks-config.toml パス (hooks-stop-quality と同方式)
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定を読み込む。読み込み / parse 失敗時は default (= disabled) を返す
fn load_config() -> ConfigFile {
    let Ok(content) = std::fs::read_to_string(config_path()) else {
        return ConfigFile::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// stdin の Stop hook 入力 JSON から transcript_path を取り出す。
/// 読み取り / parse 失敗、field 欠落は fail-open (stderr 警告 + None)。
fn read_transcript_path_from_stdin() -> Option<String> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[stop-tool-call-leak] stdin 読み込み失敗 (fail-open): {}", e);
        return None;
    }
    match serde_json::from_str::<HookInput>(&input) {
        Ok(hook_input) => {
            if hook_input.transcript_path.is_none() {
                eprintln!("[stop-tool-call-leak] transcript_path 欠落 (fail-open)");
            }
            hook_input.transcript_path
        }
        Err(e) => {
            eprintln!("[stop-tool-call-leak] 入力 JSON parse 失敗 (fail-open): {}", e);
            None
        }
    }
}

/// transcript を読み、leak 判定と連続カウントに基づいて block / fail-open を決定する
fn run_check(transcript_path: &Path, max_blocks: u32) {
    let content = match std::fs::read_to_string(transcript_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[stop-tool-call-leak] transcript 読み込み失敗 (fail-open): {}: {}",
                transcript_path.display(),
                e
            );
            return;
        }
    };
    let entries = parse_tail_entries(&content, TAIL_LINES);
    let scan = scan_tail(&entries);
    if scan.consecutive_leaks == 0 {
        return;
    }
    if scan.consecutive_leaks >= max_blocks {
        eprintln!(
            "[stop-tool-call-leak] 連続 {} 回 leak を検知しましたが上限 ({}) に達したため停止を許可します (fail-open)",
            scan.consecutive_leaks, max_blocks
        );
        return;
    }
    emit_block(&build_reason(&scan, max_blocks));
}

/// block reason を組み立てる。ツール名と検知回数を明示して再実行を促す
fn build_reason(scan: &TailScan, max_blocks: u32) -> String {
    let tool = scan.last_tool_name.as_deref().unwrap_or("不明");
    format!(
        "ツール呼び出しがテキストとして出力され、実行されていません。\n\n\
         直前の応答は、ツール呼び出し (ツール名: {}) を正規の tool_use block ではなく\
         テキスト領域に生の XML として出力しました。この呼び出しは解釈されず、\
         コマンドは一切実行されていません。\n\n\
         対処: 直前に意図したツール呼び出しを、正規のツール呼び出し機構で\
         直ちに再実行してください。応答テキストに XML を書き直してはいけません。\n\n\
         (stop-tool-call-leak 検知 {} 回目 / 上限 {} 回)",
        tool, scan.consecutive_leaks, max_blocks
    )
}

/// block 判定を stdout に出力する
fn emit_block(reason: &str) {
    let decision = BlockDecision {
        decision: "block".to_string(),
        reason: reason.to_string(),
    };
    match serde_json::to_string(&decision) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!(
            "[stop-tool-call-leak] block 判定の JSON serialize 失敗 (fail-open): {}",
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_disabled() {
        let config = ConfigFile::default().stop_tool_call_leak.unwrap_or_default();
        assert!(!config.enabled.unwrap_or(false));
    }

    #[test]
    fn config_parses_section() {
        let toml_str = r#"
[stop_tool_call_leak]
enabled = true
max_consecutive_blocks = 5
"#;
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        let leak = config.stop_tool_call_leak.unwrap();
        assert!(leak.enabled.unwrap());
        assert_eq!(leak.max_consecutive_blocks.unwrap(), 5);
    }

    #[test]
    fn config_section_missing_yields_disabled() {
        let config: ConfigFile = toml::from_str("[stop_quality]\nstep_timeout = 60\n").unwrap();
        assert!(config.stop_tool_call_leak.is_none());
    }

    #[test]
    fn config_max_blocks_defaults_to_three() {
        let leak = LeakConfig::default();
        assert_eq!(
            leak.max_consecutive_blocks
                .unwrap_or(DEFAULT_MAX_CONSECUTIVE_BLOCKS),
            3
        );
    }

    #[test]
    fn is_truthy_accepts_standard_values() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(is_truthy(value), "{:?} は truthy であるべき", value);
        }
    }

    #[test]
    fn is_truthy_rejects_falsy_values() {
        for value in ["", "0", "false", "off", "no", "2"] {
            assert!(!is_truthy(value), "{:?} は falsy であるべき", value);
        }
    }

    #[test]
    fn hook_input_parses_with_extra_fields() {
        let json = r#"{
            "session_id": "abc",
            "transcript_path": "C:\\tmp\\t.jsonl",
            "stop_hook_active": false,
            "hook_event_name": "Stop"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.transcript_path.as_deref(), Some("C:\\tmp\\t.jsonl"));
    }

    #[test]
    fn build_reason_includes_tool_name_and_counts() {
        let scan = TailScan {
            consecutive_leaks: 2,
            last_tool_name: Some("Bash".to_string()),
        };
        let reason = build_reason(&scan, 3);
        assert!(reason.contains("Bash"));
        assert!(reason.contains("2 回目"));
        assert!(reason.contains("上限 3 回"));
        assert!(reason.contains("実行されていません"));
    }

    #[test]
    fn build_reason_handles_unknown_tool_name() {
        let scan = TailScan {
            consecutive_leaks: 1,
            last_tool_name: None,
        };
        assert!(build_reason(&scan, 3).contains("不明"));
    }

    #[test]
    fn block_decision_serializes_correctly() {
        let decision = BlockDecision {
            decision: "block".to_string(),
            reason: "re-run".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains(r#""decision":"block""#));
        assert!(json.contains(r#""reason":"re-run""#));
    }
}
