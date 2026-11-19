//! ADR-092: CI 時間の SLI が超過している間、毎セッション改善作業への着手を指示する。
//!
//! 計測は `pnpm ci-sli` (scripts/ci-sli.mjs) が weekly-review skill の中で行い、結果を
//! `.claude/ci-sli-status.json` に書く。本 module はそれを読むだけで、ネットワークに出ない
//! (SessionStart hook にネットワークを入れない原則、ADR-070 § 残課題)。
//!
//! 発火は `level == "breach"` のときだけ。警告 (`warn`、しきい値の 80% 以上) は weekly-review の
//! 報告で伝え、ここでは出さない — 毎セッション出す指示は「着手必須」に限る (ADR-092 決定 3)。
//! 次の計測で breach を外れると状態ファイルの level が書き換わり、指示も止まる。
//!
//! 状態ファイルは gitignore 済みで workspace ローカルのため、weekly / monthly reminder と同じく
//! [`lib_jj_helpers::resolve_main_workspace_root`] でメイン workspace root から読む (ADR-045)。
//! 書き側 (scripts/ci-sli.mjs の `resolveMainRoot`) も同じ解釈で `.jj/repo` を辿る。
//!
//! fail-open: ファイル不在 (= 未計測) / 読込失敗 / JSON 不正は何も出さない。これは助言の出力で
//! あってゲートではない (ADR-043 の fail-closed はゲート限定)。

use lib_hook_output::SingleLineMessage;
use serde::Deserialize;
use std::path::Path;

use crate::hooks_config::CiSliReminderConfig;

pub(crate) const CI_SLI_STATUS_PATH: &str = ".claude/ci-sli-status.json";

/// additionalContext に付す明示指示 (ADR-059 defense-in-depth)。
const CI_SLI_TELL_USER_INSTRUCTION: &str =
    "セッション最初の応答で、この CI 時間 SLI の超過をユーザーに伝え、改善作業への着手を提案すること (ADR-092)。";

/// `.claude/ci-sli-status.json` の必要フィールドのみ。
#[derive(Deserialize)]
struct CiSliStatus {
    level: String,
    median_secs: u64,
    threshold_secs: u64,
    measured_at: Option<String>,
}

/// 超過中なら状態を返す。不在・読込失敗・不正・breach 以外は `None`。
fn read_breach(main_root: &Path) -> Option<CiSliStatus> {
    let content = std::fs::read_to_string(main_root.join(CI_SLI_STATUS_PATH)).ok()?;
    breach_from_content(&content)
}

fn breach_from_content(content: &str) -> Option<CiSliStatus> {
    let status: CiSliStatus = serde_json::from_str(content).ok()?;
    (status.level == "breach").then_some(status)
}

fn build_lines(status: &CiSliStatus) -> Vec<String> {
    let measured_at = status.measured_at.as_deref().unwrap_or("不明");
    vec![
        "[CI_TIME_SLI_BREACH]".to_string(),
        format!(
            "CI 時間の SLI がしきい値を超えています (直近の成功した CI run の待ち時間の中央値 {} 秒 / しきい値 {} 秒、計測 {})。\n\
             ADR-092 により、**CI 時間の改善作業に着手するのが必須**です。ほかの作業より先に、遅くなった原因 (step ごとの時間、キャッシュの当たり外れ、テスト数の増加) を調べて改善の PR を提案すること。\n\
             改善を入れた後に `pnpm ci-sli` で再計測し、しきい値を下回れば本指示は止まる。",
            status.median_secs, status.threshold_secs, measured_at,
        ),
    ]
}

/// CI SLI 超過 nudge の出力 (ADR-059 の 2 層可視化チャネル)。
pub(crate) struct CiSliNudge {
    pub(crate) additional_context: String,
    pub(crate) system_message: Option<SingleLineMessage>,
}

/// ADR-092: CI 時間の SLI 超過 nudge を組み立てる。超過していなければ `None`。
pub(crate) fn compute_ci_sli_nudge(repo_root: &Path, config: &CiSliReminderConfig) -> Option<CiSliNudge> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    let main_root = lib_jj_helpers::resolve_main_workspace_root(repo_root)
        .unwrap_or_else(|| repo_root.to_path_buf());
    let status = read_breach(&main_root)?;
    let mut lines = build_lines(&status);
    lines.push(String::new());
    lines.push(CI_SLI_TELL_USER_INSTRUCTION.to_string());

    let system_message = config.system_message_enabled.unwrap_or(false).then(|| {
        SingleLineMessage::new(format!(
            "CI 時間 SLI 超過: 中央値 {} 秒 / しきい値 {} 秒。改善作業に着手してください (ADR-092)",
            status.median_secs, status.threshold_secs
        ))
    });
    Some(CiSliNudge {
        additional_context: lines.join("\n"),
        system_message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_temp_root(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "ci-sli-{}-{}-{}",
            prefix,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        root
    }

    fn status_json(level: &str) -> String {
        format!(
            r#"{{"schema_version":1,"measured_at":"2026-11-19T12:00:00Z","level":"{}","median_secs":340,"threshold_secs":330,"warn_ratio":0.8,"samples":20}}"#,
            level
        )
    }

    fn config(enabled: bool, system_message: bool) -> CiSliReminderConfig {
        CiSliReminderConfig {
            enabled: Some(enabled),
            system_message_enabled: Some(system_message),
        }
    }

    #[test]
    fn breach_emits_mandatory_instruction_and_system_message() {
        let root = unique_temp_root("breach");
        std::fs::write(root.join(CI_SLI_STATUS_PATH), status_json("breach")).unwrap();
        let nudge = compute_ci_sli_nudge(&root, &config(true, true)).expect("breach must nudge");
        assert!(nudge.additional_context.starts_with("[CI_TIME_SLI_BREACH]\n"));
        assert!(nudge.additional_context.contains("中央値 340 秒 / しきい値 330 秒、計測 2026-11-19T12:00:00Z"));
        assert!(nudge.additional_context.contains("着手するのが必須"));
        assert!(nudge.additional_context.ends_with(CI_SLI_TELL_USER_INSTRUCTION));
        assert_eq!(
            nudge.system_message.as_ref().map(SingleLineMessage::as_str),
            Some("CI 時間 SLI 超過: 中央値 340 秒 / しきい値 330 秒。改善作業に着手してください (ADR-092)")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn warn_and_ok_do_not_nudge() {
        for level in ["warn", "ok"] {
            let root = unique_temp_root(level);
            std::fs::write(root.join(CI_SLI_STATUS_PATH), status_json(level)).unwrap();
            assert!(
                compute_ci_sli_nudge(&root, &config(true, true)).is_none(),
                "level={} は毎セッションの指示を出さない",
                level
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn disabled_config_does_not_nudge_even_on_breach() {
        let root = unique_temp_root("disabled");
        std::fs::write(root.join(CI_SLI_STATUS_PATH), status_json("breach")).unwrap();
        assert!(compute_ci_sli_nudge(&root, &config(false, true)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn system_message_is_omitted_when_its_switch_is_off() {
        let root = unique_temp_root("no-sysmsg");
        std::fs::write(root.join(CI_SLI_STATUS_PATH), status_json("breach")).unwrap();
        let nudge = compute_ci_sli_nudge(&root, &config(true, false)).expect("breach must nudge");
        assert!(nudge.system_message.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_or_malformed_status_does_not_nudge() {
        let root = unique_temp_root("missing");
        assert!(compute_ci_sli_nudge(&root, &config(true, true)).is_none());
        std::fs::write(root.join(CI_SLI_STATUS_PATH), "{not json").unwrap();
        assert!(compute_ci_sli_nudge(&root, &config(true, true)).is_none());
        std::fs::write(root.join(CI_SLI_STATUS_PATH), r#"{"level":"breach"}"#).unwrap();
        assert!(
            compute_ci_sli_nudge(&root, &config(true, true)).is_none(),
            "数値が欠けた記録では指示しない"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn measured_at_missing_is_reported_as_unknown() {
        let status = breach_from_content(r#"{"level":"breach","median_secs":400,"threshold_secs":330}"#)
            .expect("breach");
        assert!(build_lines(&status)[1].contains("計測 不明"));
    }
}
