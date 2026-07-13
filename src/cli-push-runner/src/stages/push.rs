use super::push_jj_bookmark::advance_jj_bookmarks;
use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::run_stage_cmd;

pub(crate) fn run_push(config: &PushConfig, detected_bookmarks: &[String]) -> bool {
    // NOTE: takt fix や手動 jj describe で @ が進んでも bookmark が旧コミットのまま残る問題の対策
    if config.command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_stage(
                "push",
                &format!("bookmark 自動更新失敗 (push は続行): {}", e),
            );
        }
    }

    let timeout = config.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS);
    let command = build_push_command(&config.command, detected_bookmarks);
    log_stage("push", &command);

    match run_stage_cmd("push", &command, timeout) {
        Ok(output) => {
            if push_was_refused(&output) {
                log_stage(
                    "push",
                    "失敗: リモートに反映されませんでした (jj が push を拒否)",
                );
                if !output.is_empty() {
                    eprintln!("{}", output);
                }
                return false;
            }
            log_stage("push", "成功");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            true
        }
        Err(output) => {
            log_stage("push", "失敗");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            false
        }
    }
}

/// 検出済み bookmark から push コマンドを組み立てる (ADR-045 事故 follow-up)。
///
/// 旧実装は config の `jj git push --all` を無条件実行しており、並列 workspace 運用で
/// 他 workspace の作業中 bookmark を巻き込む実害があった (ADR-045 § Known operational
/// risks)。本関数は bookmark_check stage が検出した名前を `-b <name>` で明示し、push
/// 範囲を自 workspace の bookmark に限定する。jj 0.42 の `-b` は未 tracking の新規
/// bookmark を自動 track して push するため、`--all` が担っていた新規 bookmark 対応を
/// 置き換えられる。
///
/// `-b` を付与するのは次をすべて満たす場合のみ:
/// - base が jj の git push コマンド (`jj ... git push ...`)
/// - base に push 対象の明示 (`-b`/`--bookmark`/`--all`/`--named`/`-c`/`--change`/
///   `--tracked`/`--deleted`) が無い (派生プロジェクトの旧 `--all` config は従来挙動を維持)
/// - 検出 bookmark が 1 件以上あり、全名が shell-safe
///
/// それ以外は base をそのまま返す (fail-open: bare `jj git push` は tracked-only で、
/// 新規 bookmark の無言拒否は `push_was_refused` が検知して失敗報告する)。
fn build_push_command(base: &str, bookmarks: &[String]) -> String {
    let is_jj_git_push = base.starts_with("jj ") && base.contains("git push");
    if !is_jj_git_push
        || has_explicit_push_target(base)
        || bookmarks.is_empty()
        || !bookmarks.iter().all(|b| is_shell_safe_bookmark_name(b))
    {
        return base.to_string();
    }
    let mut command = base.to_string();
    for bookmark in bookmarks {
        command.push_str(" -b ");
        command.push_str(bookmark);
    }
    command
}

/// base コマンドに push 対象の明示指定が既に含まれるか。
fn has_explicit_push_target(base: &str) -> bool {
    const TARGET_FLAGS: &[&str] = &[
        "-b",
        "--bookmark",
        "--all",
        "--named",
        "-c",
        "--change",
        "--tracked",
        "--deleted",
    ];
    base.split_whitespace().any(|token| {
        TARGET_FLAGS
            .iter()
            .any(|flag| token == *flag || token.starts_with(&format!("{flag}=")))
    })
}

/// bookmark 名が shell 経由実行 (`run_stage_cmd`) で安全な文字だけで構成されるか。
/// `jj bookmark list` 出力由来とはいえ shell に渡す文字列のため、許可リストで検証する。
fn is_shell_safe_bookmark_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

/// jj が push を拒否した（が exit 0 を返した）かを出力から判定する。
///
/// jj は新規 bookmark の push を default で拒否する際、エラー終了せず
/// "Refusing to create new remote bookmark" を出力して何もしない。
/// この無言失敗を成功と誤報告しないための検知。`-b` 明示 (jj 0.42 で自動 track) 時は
/// 通常発生しないが、fail-open で bare push になった場合や他の "Refusing to ..."
/// ガード条件を捕捉する安全網として残す。
fn push_was_refused(output: &str) -> bool {
    output.to_lowercase().contains("refusing to")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_detects_new_remote_bookmark_warning() {
        let output = "Warning: Refusing to create new remote bookmark fix/foo@origin\n\
            Hint: Run `jj bookmark track ...` and try again.\nNothing changed.";
        assert!(push_was_refused(output));
    }

    #[test]
    fn refused_is_case_insensitive() {
        assert!(push_was_refused("REFUSING TO push a commit"));
    }

    #[test]
    fn successful_push_is_not_refused() {
        let output = "Changes to push to origin:\n  \
            Add bookmark fix/foo to 3000737e";
        assert!(!push_was_refused(output));
    }

    #[test]
    fn empty_output_is_not_refused() {
        assert!(!push_was_refused(""));
    }

    #[test]
    fn build_appends_single_bookmark() {
        let cmd = build_push_command("jj git push", &["feat/xyz".into()]);
        assert_eq!(cmd, "jj git push -b feat/xyz");
    }

    #[test]
    fn build_appends_multiple_bookmarks() {
        let cmd = build_push_command("jj git push", &["feat/a".into(), "fix-b".into()]);
        assert_eq!(cmd, "jj git push -b feat/a -b fix-b");
    }

    /// 派生プロジェクトの旧 config (`--all`) は従来挙動を維持する (後方互換)。
    #[test]
    fn build_keeps_legacy_all_config_as_is() {
        let cmd = build_push_command("jj git push --all", &["feat/xyz".into()]);
        assert_eq!(cmd, "jj git push --all");
    }

    #[test]
    fn build_keeps_base_when_bookmark_already_specified() {
        let cmd = build_push_command("jj git push -b manual", &["feat/xyz".into()]);
        assert_eq!(cmd, "jj git push -b manual");
    }

    #[test]
    fn build_keeps_non_jj_command_as_is() {
        let cmd = build_push_command("git push", &["feat/xyz".into()]);
        assert_eq!(cmd, "git push");
    }

    /// fail-open: bookmark 未検出時は base のまま (tracked-only push、拒否は refused 検知)。
    #[test]
    fn build_keeps_base_when_no_bookmarks_detected() {
        let cmd = build_push_command("jj git push", &[]);
        assert_eq!(cmd, "jj git push");
    }

    /// shell メタ文字を含む bookmark 名は injection 防止のため付与しない (base のまま)。
    #[test]
    fn build_rejects_shell_unsafe_bookmark_names() {
        let cmd = build_push_command("jj git push", &["feat/x; rm -rf".into()]);
        assert_eq!(cmd, "jj git push");
        let cmd2 = build_push_command("jj git push", &["ok".into(), "bad$(cmd)".into()]);
        assert_eq!(cmd2, "jj git push");
    }

    #[test]
    fn explicit_target_detects_flag_with_equals_form() {
        assert!(has_explicit_push_target("jj git push --bookmark=feat/x"));
        assert!(has_explicit_push_target("jj git push --named myfeature=@"));
        assert!(!has_explicit_push_target("jj git push"));
    }

    #[test]
    fn shell_safe_accepts_typical_names_rejects_meta() {
        assert!(is_shell_safe_bookmark_name("feat/my-feature_1.2"));
        assert!(!is_shell_safe_bookmark_name(""));
        assert!(!is_shell_safe_bookmark_name("a b"));
        assert!(!is_shell_safe_bookmark_name("a\"b"));
        assert!(!is_shell_safe_bookmark_name("a|b"));
    }
}
