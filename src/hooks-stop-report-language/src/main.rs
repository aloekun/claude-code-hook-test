//! 最後の報告の言語の検査 (ADR-091)。
//!
//! ターンの最後の報告 (Stop hook の入力の `last_assistant_message`) が英語で始まっていたら
//! `decision: block` を返し、日本語で書き直させる。ユーザーへの報告は日本語で書くという指示が、
//! memory に書いた後も守られなかったため、決定論の仕組みで止める (ADR-042)。
//!
//! 設計判断 (ADR-091):
//! - **判定するのは最後の報告だけ**。途中経過の文は書き直せないため対象にしない。
//! - **takt のセッションは除く**。transcript の最初のユーザー発言で見分ける ([`session`])。
//! - **`stop_hook_active` のときは判定しない** (ADR-004 と同じ)。書き直しの後にもう一度 block する
//!   ことは無い。Claude Code 自身にも連続 block の上限がある。
//! - **エラー時 fail-open**。入力や transcript を読めないときは停止を許可する。本 hook は
//!   セキュリティゲートではなく、取り逃がしても「ユーザーが指摘する」という従来の状態に戻るだけ
//!   (ADR-053 と同じ判断)。
//! - **ADR-039 experimental pattern**: config opt-in (code default OFF) + kill-switch
//!   (`enabled = false` / env `STOP_REPORT_LANGUAGE_OVERRIDE`) + bounded lifetime (月次レビューで
//!   発火が続けて 0 なら撤去を判定する)。

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::Path;

mod detect;
mod session;

use session::SessionKind;

const HOOK_NAME: &str = "hooks-stop-report-language";

/// 緊急バイパス用 env var (kill-switch)。truthy 値で検査を skip する。
const OVERRIDE_ENV_VAR: &str = "STOP_REPORT_LANGUAGE_OVERRIDE";

/// block したときにモデルへ渡す理由。
pub(crate) const BLOCK_REASON: &str = "直前の報告が英語で始まっています。\
このプロジェクトでは、ユーザーへの報告を日本語で書きます (思考は英語でもかまいません)。\n\n\
対処: 直前の報告と同じ内容を、日本語で書き直してください。ツールは呼ばず、報告だけを書き直します。\
英語の報告を書いたことには触れなくてかまいません。\n\n\
(hooks-stop-report-language、ADR-091)";

/// hook 入力 (必要なフィールドのみ)。
#[derive(Deserialize)]
struct HookInput {
    last_assistant_message: Option<String>,
    transcript_path: Option<String>,
    stop_hook_active: Option<bool>,
}

#[derive(Serialize)]
struct BlockDecision<'a> {
    decision: &'a str,
    reason: &'a str,
}

/// hooks-config.toml のうち本 hook が参照する section のみ部分デシリアライズ
#[derive(Deserialize, Default)]
struct ConfigFile {
    stop_report_language: Option<LanguageConfig>,
}

/// `[stop_report_language]` section (ADR-039: code default は disabled)
#[derive(Deserialize, Default)]
struct LanguageConfig {
    enabled: Option<bool>,
}

/// 判定の結果。block 以外は停止を許可する。
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Block,
    Allow,
}

fn main() {
    if kill_switch_active() {
        return;
    }
    let enabled = load_config()
        .stop_report_language
        .and_then(|c| c.enabled)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let Some(input) = read_hook_input_from_stdin() else {
        return;
    };
    if judge(&input, session::session_kind) == Verdict::Block {
        emit_block();
    }
}

/// 判定の本体。セッションの種類の判定は transcript を読むため、関数として受け取る (テスト用)。
fn judge(input: &HookInput, session_kind: impl Fn(&Path) -> SessionKind) -> Verdict {
    if input.stop_hook_active.unwrap_or(false) {
        return Verdict::Allow;
    }
    let Some(message) = input.last_assistant_message.as_deref() else {
        eprintln!("[{HOOK_NAME}] last_assistant_message が無いため判定しません (fail-open)");
        return Verdict::Allow;
    };
    if !detect::starts_in_english(message) {
        return Verdict::Allow;
    }
    let Some(transcript_path) = input.transcript_path.as_deref() else {
        eprintln!("[{HOOK_NAME}] transcript_path が無く takt のセッションか判定できません (fail-open)");
        return Verdict::Allow;
    };
    match session_kind(Path::new(transcript_path)) {
        SessionKind::Interactive => Verdict::Block,
        SessionKind::Takt => Verdict::Allow,
        SessionKind::Unknown => {
            eprintln!("[{HOOK_NAME}] セッションの種類を判定できません (fail-open): {transcript_path}");
            Verdict::Allow
        }
    }
}

/// kill-switch env が設定されていれば skip (stderr に明示)
fn kill_switch_active() -> bool {
    match std::env::var(OVERRIDE_ENV_VAR) {
        Ok(value) if lib_telemetry::is_truthy(&value) => {
            eprintln!("[{HOOK_NAME}] {OVERRIDE_ENV_VAR} が設定されているため検査を skip します");
            true
        }
        _ => false,
    }
}

/// 設定を読み込む。読み込み / parse 失敗時は default (= disabled) を返す
fn load_config() -> ConfigFile {
    let path = lib_config_path::resolve_config("hooks-config.toml");
    let Ok(content) = std::fs::read_to_string(path) else {
        return ConfigFile::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// stdin の hook 入力 JSON をパースする。読み取り / parse 失敗は fail-open (stderr 警告 + None)。
fn read_hook_input_from_stdin() -> Option<HookInput> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[{HOOK_NAME}] stdin 読み込み失敗 (fail-open): {e}");
        return None;
    }
    match serde_json::from_str::<HookInput>(&input) {
        Ok(hook_input) => Some(hook_input),
        Err(e) => {
            eprintln!("[{HOOK_NAME}] 入力 JSON parse 失敗 (fail-open): {e}");
            None
        }
    }
}

fn emit_block() {
    record_block_firing();
    let decision = BlockDecision {
        decision: "block",
        reason: BLOCK_REASON,
    };
    match serde_json::to_string(&decision) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[{HOOK_NAME}] block 判定の JSON serialize 失敗 (fail-open): {e}"),
    }
}

/// block を発火したことを telemetry に記録する (ADR-055、fail-open)。
fn record_block_firing() {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "hooks-stop-report-language",
        kind: lib_telemetry::FiringKind::Hook,
        id: "hooks-stop-report-language",
        decision: lib_telemetry::Decision::Block,
        session_id: None,
        reason: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGLISH: &str = "PR #636 is ready to merge. Here is the status.";
    const JAPANESE: &str = "PR #636 をマージしました。";

    fn input(message: Option<&str>, stop_hook_active: bool) -> HookInput {
        HookInput {
            last_assistant_message: message.map(str::to_string),
            transcript_path: Some("t.jsonl".to_string()),
            stop_hook_active: Some(stop_hook_active),
        }
    }

    fn interactive(_: &Path) -> SessionKind {
        SessionKind::Interactive
    }

    #[test]
    fn english_report_in_interactive_session_is_blocked() {
        assert_eq!(judge(&input(Some(ENGLISH), false), interactive), Verdict::Block);
    }

    #[test]
    fn japanese_report_is_allowed() {
        assert_eq!(judge(&input(Some(JAPANESE), false), interactive), Verdict::Allow);
    }

    #[test]
    fn english_report_in_takt_session_is_allowed() {
        assert_eq!(judge(&input(Some(ENGLISH), false), |_| SessionKind::Takt), Verdict::Allow);
    }

    #[test]
    fn unknown_session_fails_open() {
        assert_eq!(judge(&input(Some(ENGLISH), false), |_| SessionKind::Unknown), Verdict::Allow);
    }

    #[test]
    fn stop_hook_active_is_allowed() {
        assert_eq!(judge(&input(Some(ENGLISH), true), interactive), Verdict::Allow);
    }

    #[test]
    fn missing_message_fails_open() {
        assert_eq!(judge(&input(None, false), interactive), Verdict::Allow);
    }

    #[test]
    fn missing_transcript_path_fails_open() {
        let mut hook_input = input(Some(ENGLISH), false);
        hook_input.transcript_path = None;
        assert_eq!(judge(&hook_input, interactive), Verdict::Allow);
    }

    /// 日本語の報告では transcript を読まない (判定の順序: 言語 → セッション)。
    #[test]
    fn japanese_report_does_not_read_transcript() {
        let verdict = judge(&input(Some(JAPANESE), false), |_| panic!("transcript を読んだ"));
        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn config_defaults_to_disabled() {
        let config: ConfigFile = toml::from_str("[stop_quality]\nstep_timeout = 60\n").unwrap();
        assert!(config.stop_report_language.is_none());
    }

    #[test]
    fn config_parses_section() {
        let config: ConfigFile = toml::from_str("[stop_report_language]\nenabled = true\n").unwrap();
        assert_eq!(config.stop_report_language.unwrap().enabled, Some(true));
    }

    #[test]
    fn hook_input_parses_with_extra_fields() {
        let json = r#"{"session_id":"abc","transcript_path":"t.jsonl","hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"done","background_tasks":[]}"#;
        let parsed: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.last_assistant_message.as_deref(), Some("done"));
        assert_eq!(parsed.stop_hook_active, Some(false));
    }

    #[test]
    fn block_decision_serializes_correctly() {
        let json = serde_json::to_string(&BlockDecision { decision: "block", reason: "r" }).unwrap();
        assert_eq!(json, r#"{"decision":"block","reason":"r"}"#);
    }

    #[test]
    fn block_reason_is_written_in_japanese_and_names_the_adr() {
        assert!(!detect::starts_in_english(BLOCK_REASON));
        assert!(BLOCK_REASON.contains("ADR-091"));
    }
}
