use lib_subprocess::{combine_output, run_cmd_direct_capture, run_cmd_shell_unlimited, truncation_notice};

use super::push_bookmarks::PushBookmarks;
use super::push_range_check::revset_string;
use super::push_jj_bookmark::advance_jj_bookmarks;
use crate::config::{PushConfig, DEFAULT_PUSH_TIMEOUT_SECS};
use crate::log::log_stage;
use crate::runner::MAX_LINES;

/// takt fix や手動 jj describe で @ が進んでも bookmark が旧コミットのまま残る問題の対策。
///
/// `run_push` から分けて呼び出し元に置くのは、前進後の bookmark で push 範囲を再検査
/// してから push するため (順位 537、PR #568 の指摘)。事前検査は前進前の bookmark しか
/// 見ておらず、takt の後に増えたコミットを検査できない。
pub(crate) fn advance_bookmarks_before_push(config: &PushConfig) {
    if config.command.starts_with("jj ") {
        if let Err(e) = advance_jj_bookmarks() {
            log_stage(
                "push",
                &format!("bookmark 自動更新失敗 (push は続行): {}", e),
            );
        }
    }
}

/// 検出した bookmark を対象に push を実行し、リモートへ反映できたかを返す。
///
/// 次のいずれかで `false` (pipeline 失敗) を返す。
/// - コマンドを組み立てられない (`build_push_command` の `Err`)
/// - プロセスが失敗した
/// - 終了コードが 0 でも、jj が push を拒否した (`push_was_refused`)
///
/// 拒否の判定は切り詰める前の全量出力で行い、ログに出すときだけ `cap_for_log` で縮める。
pub(crate) fn run_push(config: &PushConfig, detected_bookmarks: &PushBookmarks) -> bool {
    let timeout = config.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS);
    let command = match build_push_command(&config.command, detected_bookmarks) {
        Ok(command) => command,
        Err(reason) => {
            log_stage("push", &format!("失敗: {reason}"));
            return false;
        }
    };
    log_stage("push", &command.display());

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
///
/// `Direct` は `run_cmd_direct_capture` (全量を読む `drain_pipe_unlimited`) で起動し、
/// shell 経由と同じく stdout と stderr を結合して判定に渡す (jj の拒否行は stderr に出る)。
fn run_push_cmd(command: &PushCommand, timeout: u64) -> Result<String, String> {
    let (success, output) = match command {
        PushCommand::Shell(cmd) => run_cmd_shell_unlimited("push", cmd, timeout),
        PushCommand::Direct { program, args } => {
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let capture = run_cmd_direct_capture(program, &argv, timeout);
            let mut output = combine_output(&capture.stdout, &capture.stderr);
            if capture.timed_out {
                output = format!("timeout ({timeout}s) で push を打ち切りました\n{output}");
            }
            (capture.ok, output)
        }
    };
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
/// base をそのまま shell で実行するのは、検出した名前を使わない config だけ:
/// - base が jj の git push コマンド (`jj ... git push ...`) でない
/// - base に push 対象の明示 (`-b`/`--bookmark`/`--all`/`--named`/`-c`/`--change`/
///   `--tracked`/`--deleted`) がある (派生プロジェクトの旧 `--all` config は従来挙動を維持)
///
/// それ以外は **shell を通さず argv で起動し**、全名を `-b exact:"<name>"` で渡す
/// (順位 484、[ADR-044] § 実行時のデータを shell 文字列に埋め込まない)。名前は jj から
/// 読んだ実行時のデータで、shell 文字列に埋め込むと名前ごとに許可リストかエスケープが要る。
/// 許可リストは jj で有効な名前 (`fix+12` など) を拒否し、エスケープは cmd.exe と sh で
/// 規則が違う。argv で渡せば shell は名前を解釈しない。
///
/// argv にしても jj 自身が `-b` の値を文字列パターンの式として読む (既定は glob で、
/// `|` `&` `~` も演算子)。`*` を含む名前が他の bookmark に一致して push 範囲が広がらないよう、
/// `exact:` と jj の文字列リテラル (`revset_string`) で完全一致させる。
///
/// base は config が書いた文字列なので、`-b` を付ける経路では空白で argv に分割する。
/// 分割できない記法 (shell の記号) を含む base は `validate_push_command` が config を
/// 読んだ時点で拒否するため、ここでは `Err` にならない前提だが、念のため同じ検査を通す。
///
/// `bookmarks` は `PushBookmarks` なので空にならない (順位 484)。以前は `&[String]` を
/// 受けて「空なら base のまま」の分岐を持っており、jj 実行失敗時に `Some(空)` が渡ると
/// bare push が tracked bookmark を全件送ってレビュー範囲外の ref が出ていた
/// (順位 288(b)、`stages::bookmark_check` の module doc 参照)。その分岐は型で到達不能に
/// なったので消した。
fn build_push_command(base: &str, bookmarks: &PushBookmarks) -> Result<PushCommand, String> {
    let Some(base_argv) = argv_if_bookmarks_are_appended(base)? else {
        return Ok(PushCommand::Shell(base.to_string()));
    };
    let (program, base_args) = base_argv.split_first().expect("jj で始まるので空でない");
    let mut args: Vec<String> = base_args.to_vec();
    for bookmark in bookmarks.iter() {
        args.push("-b".to_string());
        args.push(format!("exact:{}", revset_string(bookmark)));
    }
    Ok(PushCommand::Direct { program: program.clone(), args })
}

/// config の push command を読んだ時点で検査する。`-b` を付ける経路なのに argv へ
/// 分割できない base は、品質ゲートや takt の前に止める (push の直前まで気づかない
/// 失敗にしない)。
pub(crate) fn validate_push_command(base: &str) -> Result<(), String> {
    argv_if_bookmarks_are_appended(base).map(|_| ())
}

/// `-b` を付ける経路なら base の argv を返す。名前を使わない config なら `None`。
fn argv_if_bookmarks_are_appended(base: &str) -> Result<Option<Vec<String>>, String> {
    let is_jj_git_push = base.starts_with("jj ") && base.contains("git push");
    if !is_jj_git_push || has_explicit_push_target(base) {
        return Ok(None);
    }
    let tokens: Vec<&str> = base.split_whitespace().collect();
    if let Some(bad) = tokens.iter().find(|t| !is_plain_argv_token(t)) {
        return Err(format!(
            "push.command `{base}` の `{bad}` は shell の記法を含むため、bookmark を argv で渡せません。\
             push.command は `jj git push` のような単純なコマンドにしてください \
             (shell が要るなら `-b` などで push 対象を明示すると、command はそのまま shell で実行される)"
        ));
    }
    Ok(Some(tokens.iter().map(|t| t.to_string()).collect()))
}

/// shell を通さずに argv の 1 要素として渡しても意味が変わらない語か。base は config が
/// 書いた文字列なので、許可リストで絞っても正当な値を拒否する心配は小さい。
fn is_plain_argv_token(token: &str) -> bool {
    token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.' | '=' | ':' | '@' | '+' | ','))
}

/// push で実行するコマンド。実行時のデータ (bookmark 名) は `Direct` の argv にしか入らない。
#[derive(Debug, PartialEq)]
enum PushCommand {
    /// config の command をそのまま shell で実行する。実行時のデータは入らない。
    Shell(String),
    /// shell を通さずに起動する。検出した bookmark 名は `args` の要素として渡る。
    Direct { program: String, args: Vec<String> },
}

impl PushCommand {
    /// ログ表示用。`Direct` は実行時の argv を空白で並べたもので、shell に渡す文字列ではない。
    fn display(&self) -> String {
        match self {
            PushCommand::Shell(command) => command.clone(),
            PushCommand::Direct { program, args } => format!("{program} {}", args.join(" ")),
        }
    }
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

/// jj が push を拒否した（が exit 0 を返した）かを出力から判定する。
///
/// jj は新規 bookmark の push を default で拒否する際、エラー終了せず
/// "Refusing to create new remote bookmark" を出力して何もしない。
/// この無言失敗を成功と誤報告しないための検知。`-b` 明示 (jj 0.42 で自動 track) 時は
/// 通常発生しないが、検出した名前を使わない config で bare push になった場合や他の "Refusing to ..."
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

    /// 空のリストは `PushBookmarks` にできないので、bookmark 未検出で `build_push_command`
    /// に届く経路のテストは無い (順位 484。届かないことは型が保証する)。
    fn bookmarks(names: &[&str]) -> PushBookmarks {
        PushBookmarks::new(names.iter().map(|n| n.to_string()).collect()).expect("non-empty test input")
    }

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

    fn direct(args: &[&str]) -> PushCommand {
        PushCommand::Direct { program: "jj".to_string(), args: args.iter().map(|a| a.to_string()).collect() }
    }

    fn shell(command: &str) -> PushCommand {
        PushCommand::Shell(command.to_string())
    }

    #[test]
    fn build_appends_single_bookmark_as_exact_argv() {
        let cmd = build_push_command("jj git push", &bookmarks(&["feat/xyz"]));
        assert_eq!(cmd, Ok(direct(&["git", "push", "-b", "exact:\"feat/xyz\""])));
    }

    #[test]
    fn build_appends_multiple_bookmarks() {
        let cmd = build_push_command("jj git push", &bookmarks(&["feat/a", "fix-b"]));
        assert_eq!(cmd, Ok(direct(&["git", "push", "-b", "exact:\"feat/a\"", "-b", "exact:\"fix-b\""])));
    }

    /// shell の記号を含む名前も、shell を通さないので 1 つの argv としてそのまま渡る。
    /// 以前の許可リストは jj で有効な `fix+12` まで拒否していた (PR #623 CodeRabbit)。
    #[test]
    fn build_passes_names_with_shell_metacharacters_as_single_arguments() {
        let cmd = build_push_command("jj git push", &bookmarks(&["fix+12", "a;b $(c)"]));
        assert_eq!(cmd, Ok(direct(&["git", "push", "-b", "exact:\"fix+12\"", "-b", "exact:\"a;b $(c)\""])));
    }

    /// jj は `-b` の値を文字列パターンの式として読む (既定 glob、`|` も演算子)。名前の
    /// 記号がパターンとして働かないよう、`exact:` と文字列リテラルで包む。
    #[test]
    fn build_quotes_names_so_jj_does_not_expand_them_as_patterns() {
        let cmd = build_push_command("jj git push", &bookmarks(&["feat/*", "a|b", "q\"uote\\"]));
        assert_eq!(
            cmd,
            Ok(direct(&[
                "git", "push", "-b", "exact:\"feat/*\"", "-b", "exact:\"a|b\"", "-b", "exact:\"q\\\"uote\\\\\"",
            ]))
        );
    }

    /// 派生プロジェクトの旧 config (`--all`) は従来挙動を維持する (後方互換)。
    #[test]
    fn build_keeps_legacy_all_config_as_is() {
        let cmd = build_push_command("jj git push --all", &bookmarks(&["feat/xyz"]));
        assert_eq!(cmd, Ok(shell("jj git push --all")));
    }

    #[test]
    fn build_keeps_base_when_bookmark_already_specified() {
        let cmd = build_push_command("jj git push -b manual", &bookmarks(&["feat/xyz"]));
        assert_eq!(cmd, Ok(shell("jj git push -b manual")));
    }

    /// 名前を使わない config は shell で base のまま実行する。名前は shell に入らない。
    #[test]
    fn build_keeps_non_jj_command_as_is() {
        let cmd = build_push_command("git push", &bookmarks(&["a;b"]));
        assert_eq!(cmd, Ok(shell("git push")));
    }

    #[test]
    fn build_keeps_other_flags_of_the_base_in_the_argv() {
        let cmd = build_push_command("jj git push --remote=origin", &bookmarks(&["feat/x"]));
        assert_eq!(cmd, Ok(direct(&["git", "push", "--remote=origin", "-b", "exact:\"feat/x\""])));
    }

    /// `-b` を付ける経路で base が shell の記法を含むと argv に分割できない。config を
    /// 読んだ時点で止める (push の直前まで気づかない失敗にしない)。
    #[test]
    fn validate_rejects_a_base_that_needs_a_shell_only_when_names_are_appended() {
        let err = validate_push_command("jj git push && echo done").expect_err("rejected");
        assert!(err.contains("&&"), "err was: {err}");
        assert!(validate_push_command("JJ_X=1 jj git push").is_ok(), "jj で始まらないので名前を使わない");
        assert!(validate_push_command("jj git push --all | tee log").is_ok(), "対象を明示すると shell のまま");
        assert!(validate_push_command("jj git push").is_ok());
    }

    #[test]
    fn display_shows_the_argv_that_is_executed() {
        assert_eq!(direct(&["git", "push", "-b", "exact:\"x\""]).display(), "jj git push -b exact:\"x\"");
        assert_eq!(shell("jj git push --all").display(), "jj git push --all");
    }

    #[test]
    fn explicit_target_detects_flag_with_equals_form() {
        assert!(has_explicit_push_target("jj git push --bookmark=feat/x"));
        assert!(has_explicit_push_target("jj git push --named myfeature=@"));
        assert!(!has_explicit_push_target("jj git push"));
    }

    #[test]
    fn plain_argv_token_accepts_config_words_and_rejects_shell_syntax() {
        for token in ["jj", "git", "push", "--remote=origin", "feat/x_1.2", "a@b+c,d:e"] {
            assert!(is_plain_argv_token(token), "{token}");
        }
        for token in ["&&", "|", "$HOME", "a;b", "\"x\"", "'x'", "`x`", "a>b", "*"] {
            assert!(!is_plain_argv_token(token), "{token}");
        }
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
        /// cmd.exe と POSIX sh で構文が非互換なため OS 別に出し分ける (WP-15)。
        /// **出力行数と拒否行の文面は両 OS で揃えること** — ズレると片側だけ
        /// 「cap の外の拒否行を検知する」という本 mod の主題を検証しなくなる。
        #[cfg(windows)]
        const REFUSAL_BEYOND_CAP: &str =
            "(for /L %i in (1,1,40) do @echo Changes to push to origin) \
            & echo Warning: Refusing to create new remote bookmark feat/x@origin";
        #[cfg(not(windows))]
        const REFUSAL_BEYOND_CAP: &str =
            "i=1; while [ $i -le 40 ]; do echo Changes to push to origin; i=$((i+1)); done; \
             echo Warning: Refusing to create new remote bookmark feat/x@origin";

        /// 40 行を超える正常な push 出力 (拒否なし)。
        #[cfg(windows)]
        const SUCCESS_BEYOND_CAP: &str =
            "(for /L %i in (1,1,50) do @echo Add bookmark feat/x to 3000737e)";
        #[cfg(not(windows))]
        const SUCCESS_BEYOND_CAP: &str =
            "i=1; while [ $i -le 50 ]; do echo Add bookmark feat/x to 3000737e; i=$((i+1)); done";

        /// incident 再現 (bad): cap の外にある拒否行を検知できること。
        /// jj は拒否時も exit 0 を返すため、この検知が唯一の防波堤になる。
        #[test]
        fn refusal_beyond_the_cap_is_detected() {
            let output = run_push_cmd(&PushCommand::Shell(REFUSAL_BEYOND_CAP.to_string()), 30)
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
            let output = run_push_cmd(&PushCommand::Shell(SUCCESS_BEYOND_CAP.to_string()), 30).expect("成功コマンドは Ok");
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
            let output = run_push_cmd(&PushCommand::Shell(REFUSAL_BEYOND_CAP.to_string()), 30).expect("拒否出力は exit 0");
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
            assert!(
                push_was_refused(&output),
                "判定は全量出力に対して真であること"
            );
        }

        /// argv 経路 (`PushCommand::Direct`) も全量を読み、stderr の拒否行を検知すること
        /// (順位 484 で bookmark を付ける push はこちらの経路に移った。jj の拒否行は stderr)。
        /// shell 構文の fixture を動かすため program に cmd / sh を置くが、検査対象は
        /// `run_cmd_direct_capture` + 結合の経路そのもの。
        #[test]
        fn direct_refusal_on_stderr_beyond_the_cap_is_detected() {
            #[cfg(windows)]
            let command = PushCommand::Direct {
                program: "cmd".to_string(),
                args: vec![
                    "/C".to_string(),
                    "(for /L %i in (1,1,40) do @echo Changes to push to origin) \
                     & echo Warning: Refusing to create new remote bookmark feat/x@origin 1>&2"
                        .to_string(),
                ],
            };
            #[cfg(not(windows))]
            let command = PushCommand::Direct {
                program: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "i=1; while [ $i -le 40 ]; do echo Changes to push to origin; i=$((i+1)); done; \
                     echo Warning: Refusing to create new remote bookmark feat/x@origin >&2"
                        .to_string(),
                ],
            };
            let output = run_push_cmd(&command, 30).expect("拒否は exit 0 なので Ok 経路で返る");
            assert!(output.lines().count() > MAX_LINES, "全量を読むこと: {} 行", output.lines().count());
            assert!(push_was_refused(&output), "stderr の拒否行を検知すること: {:?}", output);
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
            let output: String = (0..MAX_LINES + 1)
                .map(|i| format!("line {}\n", i))
                .collect();
            assert!(
                cap_for_log(&output).ends_with("... (1 line truncated)"),
                "1 行超過は単数形",
            );
        }
    }
}
