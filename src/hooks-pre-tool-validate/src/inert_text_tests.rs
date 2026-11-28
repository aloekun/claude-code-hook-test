//! [`super::strip_inert_text`] のテスト (順位 620)。

use super::*;

fn bash(command: &str) -> String {
    strip_inert_text(command, Dialect::Bash)
}

fn ps(command: &str) -> String {
    strip_inert_text(command, Dialect::PowerShell)
}

/// 外さないこと (元の文字列のまま) を確かめる。
fn kept_bash(command: &str) {
    assert_eq!(bash(command), command, "外してはいけない: {command:?}");
}

#[test]
fn strips_quoted_message_values() {
    assert_eq!(bash("jj describe -m 'avoid --ignore-immutable'"), "jj describe -m ''");
    assert_eq!(bash("jj describe -m \"avoid --ignore-immutable\" -r @-"), "jj describe -m \"\" -r @-");
    assert_eq!(bash("jj describe --message 'x jj new'"), "jj describe --message ''");
    assert_eq!(bash("jj describe --message=\"x --ignore-immutable\""), "jj describe --message=\"\"");
}

/// 二重引用符の中のコマンド置換は実行されるので外さない。
#[test]
fn keeps_double_quoted_values_that_run_commands() {
    kept_bash("jj describe -m \"$(jj rebase --ignore-immutable)\"");
    kept_bash("jj describe -m \"`jj git push`\"");
}

/// `-m` の値でない引用符の中は外さない (引用符は argv を変えない)。
#[test]
fn keeps_quoted_words_that_are_not_message_values() {
    kept_bash("jj rebase \"--ignore-immutable\" -r x");
    kept_bash("jj new 'main'");
    kept_bash("grep \"git-only\" file");
}

/// 値が引用符の後ろに続く語 (`-m 'a'b`) は、値の範囲が引用符と一致しないので外さない。
#[test]
fn keeps_message_values_glued_to_more_text() {
    kept_bash("jj describe -m 'a'--ignore-immutable");
}

#[test]
fn strips_the_body_of_quoted_heredocs() {
    let cmd = "pnpm threads fixed 3 --stdin <<'EOF'\njj new で直した\n--ignore-immutable は使わない\nEOF\n";
    assert_eq!(bash(cmd), "pnpm threads fixed 3 --stdin <<'EOF'\nEOF\n");
    let cmd = "jj describe --stdin <<\"EOF\"\nbody\nEOF";
    assert_eq!(bash(cmd), "jj describe --stdin <<\"EOF\"\nEOF");
    let cmd = "jj describe --stdin <<\\EOF\nbody\nEOF";
    assert_eq!(bash(cmd), "jj describe --stdin <<\\EOF\nEOF");
}

/// `<<-` は区切り語の行の先頭のタブを許す。CRLF の行も区切り語として読む。
#[test]
fn strips_tab_stripped_and_crlf_heredocs() {
    assert_eq!(bash("cat <<-'EOF'\n\tbody\n\tEOF\n"), "cat <<-'EOF'\n\tEOF\n");
    assert_eq!(bash("cat <<'EOF'\r\nbody\r\nEOF\r\n"), "cat <<'EOF'\r\nEOF\r\n");
}

/// 区切り語を引用しない heredoc は本文が展開されるので外さない。
#[test]
fn keeps_unquoted_heredocs() {
    kept_bash("cat <<EOF\n$(jj git push)\nEOF\n");
    kept_bash("cat <<EOF\njj new\nEOF\n");
}

/// 本文や値をシェル・処理系が読む形は、何も外さない。
#[test]
fn keeps_text_that_an_interpreter_may_run() {
    kept_bash("bash <<'EOF'\njj git push\nEOF\n");
    kept_bash("cat <<'EOF' | sh\njj git push\nEOF\n");
    kept_bash("xargs jj <<'EOF'\n--ignore-immutable\nEOF\n");
    kept_bash("env bash -c 'x' -m 'y'");
    kept_bash("C:/Git/bin/bash.exe <<'EOF'\njj git push\nEOF\n");
    kept_bash("python -m 'mod'");
}

/// 許可リストにない処理系が受け取る本文は外さない (SEC-NEW-inert_text-L264)。
#[test]
fn keeps_heredoc_bodies_given_to_commands_outside_the_allowlist() {
    kept_bash("awk -f /dev/stdin <<'EOF'\nBEGIN{system(\"jj git push\")}\nEOF\n");
    kept_bash("make -f - <<'EOF'\nall:\n\tjj git push\nEOF\n");
    kept_bash("tclsh <<'EOF'\nexec jj git push\nEOF\n");
    kept_bash("awk -m 'jj git push' x");
}

/// 許可リスト内の受け手の出力を、リスト外の処理系へ流す形も外さない (SEC-NEW-inert_text-L153)。
#[test]
fn keeps_heredoc_bodies_that_flow_to_commands_outside_the_allowlist() {
    kept_bash("cat <<'EOF' | awk -f /dev/stdin\nBEGIN{system(\"jj git push\")}\nEOF\n");
    kept_bash("cat <<'EOF' | make -f -\nall:\n\tjj git push\nEOF\n");
    kept_bash("awk -f <(cat <<'EOF'\nBEGIN{system(\"jj git push\")}\nEOF\n)\n");
    let cmd = "@'\njj new\n'@ | awk -f /dev/stdin";
    assert_eq!(ps(cmd), cmd);
    assert_eq!(
        bash("cd x && pnpm threads fixed 3 --stdin <<'EOF'\njj new\nEOF\n"),
        "cd x && pnpm threads fixed 3 --stdin <<'EOF'\nEOF\n"
    );
}

/// キーワードや前置きの代入の後ろのコマンドも、`-m` の受け手として読む。
#[test]
fn strips_message_values_after_keywords_or_assignments() {
    assert_eq!(
        bash("if true; then jj describe -m 'x --ignore-immutable'; fi"),
        "if true; then jj describe -m ''; fi"
    );
    assert_eq!(bash("X=1 jj describe -m 'jj new'"), "X=1 jj describe -m ''");
}

/// 許可リストの受け手でも、プロセス置換や変数で本文を別のコマンドへ流せる形は外さない。
#[test]
fn keeps_heredoc_bodies_routed_through_substitutions_or_variables() {
    kept_bash("tee >(bash) <<'EOF'\njj git push\nEOF\n");
    kept_bash("cat <<'EOF' > >(sh)\njj git push\nEOF\n");
    kept_bash("cat \"$F\" <<'EOF'\njj new\nEOF\n");
}

/// 許可リスト内のコマンド名の下で別のコマンドや設定を差し込む形も外さない (SEC-NEW-inert_text-L148)。
#[test]
fn keeps_text_when_an_allowlisted_command_launches_another() {
    kept_bash("pnpm exec awk -f /dev/stdin <<'EOF'\nBEGIN{system(\"jj new\")}\nEOF\n");
    kept_bash("pnpm dlx awk -f /dev/stdin <<'EOF'\nBEGIN{system(\"jj new\")}\nEOF\n");
    kept_bash("git -c alias.x='!sh' x <<'EOF'\njj new\nEOF\n");
    kept_bash("git -c alias.x='!sh' commit -m 'jj new'");
    kept_bash("jj --config-toml='aliases.x=[\"util\"]' describe -m 'jj new'");
    kept_bash("gh alias set x '!sh' <<'EOF'\njj new\nEOF\n");
}

/// 受け手の前の大域オプションは、許可リスト内のコマンドに別の処理系を起動させられるので外さない
/// (PR #667 の CodeRabbit 指摘)。
#[test]
fn keeps_text_when_a_global_option_precedes_the_subcommand() {
    kept_bash("git --config-env=core.editor=SHELL commit --allow-empty --template=/dev/stdin <<'EOF'\njj git push\nEOF\n");
    kept_bash("git --config-env core.editor=SHELL commit -m 'jj new'");
    kept_bash("git --exec-path=/x commit -m 'jj new'");
    kept_bash("git -c core.editor=bash commit -m 'jj new'");
    kept_bash("git -C dir commit -m 'jj new'");
    kept_bash("jj --config 'ui.editor=\"bash\"' describe -m 'jj new'");
    kept_bash("jj -R x describe --stdin <<'EOF'\njj new\nEOF\n");
}

/// jj は `--config` をサブコマンドの後ろでも受け付けるので、後ろに書いた形も外さない (順位 629)。
#[test]
fn keeps_text_when_jj_config_follows_the_subcommand() {
    kept_bash("jj describe --config 'ui.editor=\"bash\"' -m 'jj new'");
    kept_bash("jj describe --config='ui.editor=\"bash\"' -m 'jj new'");
    kept_bash("jj describe --config-file x.toml -m 'jj new'");
    kept_bash("jj describe --stdin --config 'ui.editor=\"bash\"' <<'EOF'\njj new\nEOF\n");
}

/// jj の大域オプションのうち設定を書き換えないもの (`-R` など) は、サブコマンドの後ろなら外す
/// (順位 629、ADR-093 決定 4)。`cd dir && jj describe` と同じく、別の repo を指すだけで処理系は起動しない。
#[test]
fn still_strips_with_jj_global_options_other_than_config_after_the_subcommand() {
    assert_eq!(bash("jj describe -R x -m 'jj new'"), "jj describe -R x -m ''");
    assert_eq!(
        bash("jj describe --stdin --ignore-working-copy <<'EOF'\njj new\nEOF\n"),
        "jj describe --stdin --ignore-working-copy <<'EOF'\nEOF\n"
    );
}

/// エディタや対話ツールを起動するオプションがあると、`-m` があってもメッセージを処理系に
/// 渡せるので外さない (PR #667 の CodeRabbit 指摘)。
#[test]
fn keeps_text_when_an_editor_launching_option_is_present() {
    kept_bash("GIT_EDITOR=bash git commit --allow-empty --no-status --edit -m '\njj git push\n'");
    kept_bash("git commit -e -m 'jj git push'");
    kept_bash("git commit -em 'jj git push'");
    kept_bash("jj describe --edit -m 'jj new'");
    kept_bash("jj squash -i -m 'jj new'");
    kept_bash("jj squash --tool x -m 'jj new'");
    kept_bash("jj commit --editor -m 'jj new'");
    kept_bash("jj describe --interactive -m 'jj new'");
    kept_bash("GIT_EDITOR=bash git commit --allow-empty --edi -m '\njj git push\n'");
    kept_bash("jj describe --interact -m 'jj new'");
    kept_bash("jj squash --too x -m 'jj new'");
}

/// 変数で渡した語は `--edit` に展開されても見えないので、`-m` の値も外さない。
#[test]
fn keeps_message_values_when_any_word_of_the_command_is_dynamic() {
    kept_bash("GIT_EDITOR=bash git commit $OPT -m 'jj git push'");
    kept_bash("jj describe \"$FLAG\" -m 'jj new'");
}

/// オプションをサブコマンドの後ろに置く普段の形は、これまでどおり外す。
#[test]
fn still_strips_with_ordinary_options_after_the_subcommand() {
    assert_eq!(bash("jj describe -r @- -m 'jj new'"), "jj describe -r @- -m ''");
    assert_eq!(bash("git commit --allow-empty -m 'jj git push'"), "git commit --allow-empty -m ''");
    assert_eq!(bash("git commit -F - <<'EOF'\njj new\nEOF\n"), "git commit -F - <<'EOF'\nEOF\n");
    assert_eq!(
        bash("gh pr create --title x --body-file - <<'EOF'\njj new\nEOF\n"),
        "gh pr create --title x --body-file - <<'EOF'\nEOF\n"
    );
}

/// 前置きの環境変数は、許可リスト内のコマンドにエディタ等として別プロセスを起動させられるので、
/// heredoc の本文は外さない (SEC-NEW-inert_text-L392)。
#[test]
fn keeps_heredoc_body_when_env_assignment_precedes_the_reader() {
    kept_bash("JJ_EDITOR='bash -s' jj describe <<'EOF'\njj git push\nEOF\n");
    kept_bash("GIT_EDITOR='sh -s' git commit <<'EOF'\njj new\nEOF\n");
}

/// PowerShell の here-string は、許可リスト外のコマンドへ渡すなら外さない (SEC-NEW-inert_text-L212)。
#[test]
fn keeps_powershell_here_strings_given_to_commands_outside_the_allowlist() {
    for cmd in ["awk @'\njj new\n'@ /dev/null", "make @'\njj new\n'@", "pnpm exec awk @'\njj new\n'@"] {
        assert_eq!(ps(cmd), cmd);
    }
    assert_eq!(ps("@'\njj new\n'@ | jj describe --stdin"), "@'\n'@ | jj describe --stdin");
}

/// `-m` の値に here-string を渡す形は、値が文字列として決まらない語になるので外さない。
/// 外せるのは、here-string を `jj describe --stdin` へパイプで渡す形。
#[test]
fn keeps_powershell_here_strings_passed_as_message_arguments() {
    let cmd = "jj describe -m @'\njj new\n'@";
    assert_eq!(ps(cmd), cmd);
}

/// 先頭の語が変数やコマンド置換のコマンドは、何を実行するか分からないので外さない。
#[test]
fn keeps_text_when_a_command_name_is_dynamic() {
    kept_bash("$SHELL <<'EOF'\njj git push\nEOF\n");
    kept_bash("while read l; do $l; done <<'EOF'\njj git push\nEOF\n");
}

/// 引用符やコメントの中の `<<'X'` は heredoc ではない。後ろの行は本物のコマンド。
#[test]
fn heredoc_markers_inside_quotes_or_comments_are_not_heredocs() {
    kept_bash("echo \"<<'X'\"\njj git push\nX\n");
    kept_bash("echo '<<X'\njj git push\nX\n");
    kept_bash("# <<'X'\njj git push\nX\n");
}

/// 閉じない heredoc や引用符は読めないので外さない。
#[test]
fn keeps_unterminated_constructs() {
    kept_bash("cat <<'EOF'\nbody without end\n");
    kept_bash("jj describe -m 'unterminated");
    kept_bash("cat <<'EOF'");
}

/// 区切り語の直後に文字が続く形 (`'E'OF`) は区切り語を決められないので外さない。
#[test]
fn keeps_heredocs_with_partly_quoted_delimiters() {
    kept_bash("cat <<'E'OF\nbody\nEOF\n");
}

#[test]
fn strips_powershell_message_values_and_single_quoted_here_strings() {
    assert_eq!(ps("jj describe -m 'avoid --ignore-immutable'"), "jj describe -m ''");
    assert_eq!(ps("jj describe -m 'it''s --ignore-immutable'"), "jj describe -m ''");
    let cmd = "@'\njj new\n--ignore-immutable\n'@ | jj describe --stdin";
    assert_eq!(ps(cmd), "@'\n'@ | jj describe --stdin");
}

/// PowerShell の `@"` here-string は `$(...)` が展開されるので外さない。
#[test]
fn keeps_powershell_double_quoted_here_strings() {
    let cmd = "@\"\n$(jj git push)\n\"@ | jj describe --stdin";
    assert_eq!(ps(cmd), cmd);
}

#[test]
fn keeps_powershell_text_run_by_invoke_expression() {
    let cmd = "@'\njj git push\n'@ | Invoke-Expression";
    assert_eq!(ps(cmd), cmd);
}

#[test]
fn cmd_dialect_is_left_untouched() {
    let cmd = "jj describe -m 'x --ignore-immutable'";
    assert_eq!(strip_inert_text(cmd, Dialect::Cmd), cmd);
}

/// 走査の段 ([`Stripper`]) だけの結果。外した後の確認 ([`accept_if_same_commands`]) は通さない。
/// 確認が後段で拾う誤りも、走査の段で起こさないことを固定する。
fn scan(command: &str, dialect: Dialect) -> Option<String> {
    Stripper::new(command, dialect).run()
}

#[test]
fn scanner_keeps_message_values_that_run_commands() {
    let cmd = "jj describe -m \"$(jj rebase --ignore-immutable)\"";
    assert_eq!(scan(cmd, Dialect::Bash).as_deref(), Some(cmd));
}

#[test]
fn scanner_does_not_read_heredoc_markers_in_comments() {
    let cmd = "# <<'X'\njj git push\nX\n";
    assert_eq!(scan(cmd, Dialect::Bash).as_deref(), Some(cmd));
}

#[test]
fn scanner_gives_up_on_partly_quoted_delimiters() {
    assert_eq!(scan("cat <<'E'OF\nbody\nEOF\n", Dialect::Bash), None);
    let misread_as_e = "cat <<'E'OF\nbody\nE\njj git push\nEOF\n";
    assert_eq!(scan(misread_as_e, Dialect::Bash), None, "実 Bash の区切り語は EOF で、E の行は本文");
}

#[test]
fn scanner_keeps_double_quoted_here_string_bodies() {
    let cmd = "@\"\n$(jj git push)\n\"@ | jj describe --stdin";
    assert_eq!(scan(cmd, Dialect::PowerShell).as_deref(), Some(cmd));
}

/// 走査が本物のコマンドの行まで外した文字列を作っても、確認の段で元に戻す。
#[test]
fn a_stripped_text_that_drops_a_command_is_rejected() {
    let command = "cat <<'X'\nbody\nX\njj describe --stdin\n";
    let wrong = "cat <<'X'\nX\n".to_string();
    let original = lex_segments(command, Dialect::Bash);
    assert_eq!(accept_if_same_commands(command, &original, wrong, Dialect::Bash), command);
    let right = "cat <<'X'\nX\njj describe --stdin\n".to_string();
    assert_eq!(accept_if_same_commands(command, &original, right.clone(), Dialect::Bash), right);
}

/// `-m` の値以外の語が変わった文字列も元に戻す。
#[test]
fn a_stripped_text_that_changes_a_non_message_word_is_rejected() {
    let command = "jj rebase '--ignore-immutable' -m 'x'";
    let wrong = "jj rebase '' -m ''".to_string();
    let original = lex_segments(command, Dialect::Bash);
    assert_eq!(accept_if_same_commands(command, &original, wrong, Dialect::Bash), command);
}

/// PowerShell の `&` / `.` 呼び出しは、何を実行するか分からないので外さない。
#[test]
fn keeps_powershell_text_when_the_command_is_called_dynamically() {
    let cmd = "@'\njj git push\n'@ | & $tool";
    assert_eq!(ps(cmd), cmd);
}

/// `if` / `then` などのキーワードや前置きの代入の後ろの語がコマンドの名前になる。
#[test]
fn keeps_text_when_the_command_after_keywords_or_assignments_is_dynamic() {
    kept_bash("if true; then $cmd; fi <<'EOF'\njj git push\nEOF\n");
    kept_bash("X=1 $cmd <<'EOF'\njj git push\nEOF\n");
}
