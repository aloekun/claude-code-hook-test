use lib_subprocess::{run_cmd_shell_unlimited, truncation_notice};

use super::push_jj_bookmark::advance_jj_bookmarks;
use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::MAX_LINES;

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

    match run_push_cmd(&command, timeout) {
        Ok(output) => {
            if push_was_refused(&output) {
                log_stage(
                    "push",
                    "失敗: リモートに反映されませんでした (jj が push を拒否)",
                );
                print_output(&output);
                return false;
            }
            log_stage("push", "成功");
            print_output(&cap_for_log(&output));
            true
        }
        Err(output) => {
            log_stage("push", "失敗");
            print_output(&output);
            false
        }
    }
}

/// push コマンド専用: 出力を切り詰めずに全行を取得する。
///
/// capped variant (`run_cmd_shell_capped`、`MAX_LINES` 行で silent truncate) を使うと、
/// jj の出力が cap を超えて拒否行が外に落ちた場合に `push_was_refused` が拒否を見逃し、
/// **リモート未反映のまま exit 0** になる (後続の pr-monitor が旧 head を監視する)。
/// `lib-subprocess` の doc が定める「出力を control flow 判定に使う callsite で capped
/// variant を使ってはならない」契約に従い、判定は全量出力に対して行う。
fn run_push_cmd(cmd: &str, timeout: u64) -> Result<String, String> {
    let (success, output) = run_cmd_shell_unlimited("push", cmd, timeout);
    if success {
        Ok(output)
    } else {
        Err(output)
    }
}

fn print_output(output: &str) {
    if !output.is_empty() {
        eprintln!("{}", output);
    }
}

/// 成功時のログ表示用に先頭 `MAX_LINES` 行へ切り詰め、超過分は行数を明示する。
///
/// 判定 (`push_was_refused`) は全量出力に対して行い、cap は表示にのみ掛ける
/// (= 従来のログ量を維持しつつ、判定は truncate の影響を受けない)。失敗経路では
/// 診断情報を落とさないため本関数を通さず全量を出す。
///
/// truncate 表記は `lib_subprocess::truncation_notice` を共有し、`drain_pipe_capped_reporting`
/// (pipe を streaming しながら数える版) とログ上の見え方を揃える。
fn cap_for_log(output: &str) -> String {
    let mut lines = output.lines();
    let head: Vec<&str> = lines.by_ref().take(MAX_LINES).collect();
    let truncated = lines.count();
    if truncated == 0 {
        return head.join("\n");
    }
    format!("{}\n{}", head.join("\n"), truncation_notice(truncated))
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

/// bookmark 名が shell 経由実行 (`run_push_cmd`) で安全な文字だけで構成されるか。
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
///
/// **入力は `run_push_cmd` の全量出力であること**。cap 済み出力を渡すと拒否行が
/// 落ちて silent-failure push を見逃す (本関数が防ぐべき事故そのもの)。
///
/// 単純な部分一致に留めるのは fail-closed (ADR-043) の判断による。行頭マッチ等への
/// 厳格化は誤検知を減らすが、jj のメッセージ書式変更で検知漏れ側に倒れる。両者のリスクは
/// 非対称で、誤検知 (push 成功を失敗と報告) は出力もそのまま表示されるため気付いて
/// 再実行できるのに対し、検知漏れはリモート未反映のまま exit 0 で先へ進む。
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

    /// T5 回帰テスト群: push 拒否検知が 40 行 truncate 済み出力に依存していた不具合
    /// (ADR-049 の流儀: 1 test = 1 failure mode + good/bad)。
    ///
    /// 由来: 2026-07-16 の push パイプライン調査 (コード監査で発見。in the wild の
    /// 発火記録は無く、`lib-subprocess` の doc 契約違反として特定された)。
    ///
    /// 事故の形: `run_push` は当時の `runner::run_stage_cmd` (= `run_cmd_shell_capped`、
    /// `MAX_LINES` 行の silent truncate) の出力に `push_was_refused` を掛けていた。jj の出力が
    /// cap を超えて拒否行が外へ落ちると、拒否を見逃して **リモート未反映のまま exit 0** となり、
    /// 後続の pr-monitor が旧 head を監視する。
    ///
    /// 修正の核心は「判定は全量出力 (`run_push_cmd`)、cap は表示側 (`cap_for_log`) にのみ」。
    /// bad / good とも cap を超える長さの実出力で固定し、判定と表示の分離を seal する。
    mod t5_truncated_refusal_detection {
        use super::*;

        /// 40 行の正常出力の後に拒否行が来る = 拒否行が cap の外に落ちる状況の再現。
        const REFUSAL_BEYOND_CAP: &str = "(for /L %i in (1,1,40) do @echo Changes to push to origin) \
            & echo Warning: Refusing to create new remote bookmark feat/x@origin";

        /// 40 行を超える正常な push 出力 (拒否なし)。
        const SUCCESS_BEYOND_CAP: &str =
            "(for /L %i in (1,1,50) do @echo Add bookmark feat/x to 3000737e)";

        /// incident 再現 (bad): cap の外にある拒否行を検知できること。
        /// jj は拒否時も exit 0 を返すため、この検知が唯一の防波堤になる。
        #[test]
        fn refusal_beyond_the_cap_is_detected() {
            let output = run_push_cmd(REFUSAL_BEYOND_CAP, 30)
                .expect("jj の拒否は exit 0 なので Ok 経路で返る");
            assert!(
                output.lines().count() > MAX_LINES,
                "run_push_cmd が {} 行に切り詰めている ({} 行の fixture を投入) = T5 の不具合。\
                 判定に使う出力は truncate してはならない",
                output.lines().count(),
                MAX_LINES + 1,
            );
            assert!(
                push_was_refused(&output),
                "cap の外にある拒否行を検知できること: {:?}",
                output,
            );
        }

        /// good: cap を超える正常な push 出力を拒否と誤判定しないこと。
        #[test]
        fn long_successful_output_is_not_refused() {
            let output = run_push_cmd(SUCCESS_BEYOND_CAP, 30).expect("成功コマンドは Ok");
            assert!(
                output.lines().count() > MAX_LINES,
                "run_push_cmd が {} 行に切り詰めている = T5 の不具合 (good 側も全量で判定する)",
                output.lines().count(),
            );
            assert!(!push_was_refused(&output), "誤検知しないこと: {:?}", output);
        }

        /// 表示 cap は判定に影響しない: `cap_for_log` は超過分を明示して切り詰めるが、
        /// `push_was_refused` に渡すのは常に全量出力である。
        #[test]
        fn cap_for_log_truncates_display_but_not_the_verdict() {
            let output = run_push_cmd(REFUSAL_BEYOND_CAP, 30).expect("拒否出力は exit 0");
            let displayed = cap_for_log(&output);
            assert!(
                displayed.contains("truncated"),
                "表示側は超過を明示して切り詰めること: {:?}",
                displayed,
            );
            assert!(
                !push_was_refused(&displayed),
                "前提の確認: 表示用に cap すると拒否行が落ちる (だから判定は全量で行う)",
            );
            assert!(push_was_refused(&output), "判定は全量出力に対して真であること");
        }

        #[test]
        fn cap_for_log_keeps_short_output_unchanged() {
            let output = "Changes to push to origin:\n  Add bookmark feat/x to 3000737e";
            assert_eq!(cap_for_log(output), output);
        }

        #[test]
        fn cap_for_log_reports_truncated_line_count() {
            let output: String = (0..MAX_LINES + 3)
                .map(|i| format!("line {}\n", i))
                .collect();
            let displayed = cap_for_log(&output);
            assert!(
                displayed.ends_with("... (3 lines truncated)"),
                "超過行数を明示すること: {:?}",
                displayed,
            );
        }

        #[test]
        fn cap_for_log_uses_singular_form_for_one_truncated_line() {
            let output: String = (0..MAX_LINES + 1).map(|i| format!("line {}\n", i)).collect();
            assert!(
                cap_for_log(&output).ends_with("... (1 line truncated)"),
                "1 行超過は単数形",
            );
        }
    }
}
