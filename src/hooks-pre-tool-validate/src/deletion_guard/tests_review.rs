//! PR #587 の CodeRabbit レビューで見つかった抜け道と、作業中に踏んだ誤検知の回帰テスト。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";
const SCRATCH_FWD: &str = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

/// 許可リストに無いコマンド (名前を解決できないものを含む) として、ユーザーに確認するか。
fn asked(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).unlisted.is_empty()
}

/// 確認なしでは通らないか (止めるか、確認する)。判定できない削除と、コマンドの中に書いたコードの
/// 実行は確認する (ADR-086 決定 7)。
fn stopped(command: &str, dialect: Dialect) -> bool {
    let found = inspect(command, dialect, &ctx(), &[]);
    !found.violations.is_empty() || !found.undetermined.is_empty() || !found.inline_code.is_empty()
}

/// 二重引用符の中の `…$"` の `$` は、ただの `$`。特別な展開と読んで閉じの `"` まで食べると、
/// 引用の対応が崩れて後ろのパイプを誤読し、削除の無いコマンドを止めていた (作業中に実際に踏んだ)。
#[test]
fn a_dollar_before_the_closing_double_quote_is_a_plain_dollar() {
    let command = r#"gh api x --jq '.body' | sed 's/<[^>]*>//g' | grep -v "^\s*$" | grep -E "^\*\*|^(例|\`|-)" | head -18"#;
    assert!(!blocked(command, Dialect::Bash));
    assert!(!blocked(r#"echo "cost: 5$" && ls"#, Dialect::Bash));
}

/// Bash の `$'...'` / `$"..."` を引用として読む。読まないと後ろのコマンドが引用の中に消える。
#[test]
fn ansi_c_and_locale_quotes_do_not_swallow_the_following_command() {
    assert!(blocked("echo $'x'; rm -rf /tmp/outside", Dialect::Bash));
    assert!(blocked(r#"echo $"x"; rm -rf /tmp/outside"#, Dialect::Bash));
}

/// コマンド置換の本文は、外側のシェルが実行するコマンドとして読む。
#[test]
fn command_substitution_bodies_are_analyzed() {
    assert!(blocked(r#"echo "$(rm -rf ~/important)""#, Dialect::Bash));
    assert!(blocked("X=$(rm -rf /outside)", Dialect::Bash));
    assert!(blocked("echo `rm -rf /outside`", Dialect::Bash));
    assert!(blocked(r"Write-Output $(Remove-Item -Recurse C:\important)", Dialect::PowerShell));
    assert!(blocked(r"Write-Output @(Remove-Item -Recurse C:\important)", Dialect::PowerShell));
    assert!(!blocked(r#"echo "$(date)""#, Dialect::Bash));
}

/// PowerShell のコマンドの途中の `(...)` で segment を分けない。分けると対象と `-Recurse` が離れる。
#[test]
fn powershell_inner_parentheses_stay_in_the_command() {
    assert!(stopped(r"Remove-Item (Join-Path C:\ Windows) -Recurse -Force", Dialect::PowerShell));
    assert!(stopped("Remove-Item -Path (Get-Location) -Recurse", Dialect::PowerShell));
    assert!(blocked(r"Remove-Item (Remove-Item -Recurse C:\x)", Dialect::PowerShell));
}

/// PowerShell の代入の右辺はコマンドとして実行される (`$null = …` は出力を捨てる書き方)。
#[test]
fn the_right_side_of_a_powershell_assignment_is_a_command() {
    assert!(blocked(r"$null = Remove-Item -Recurse -Force C:\important", Dialect::PowerShell));
    assert!(!blocked(r#"$x = "C:\important""#, Dialect::PowerShell));
}

/// Windows PowerShell 5.1 は、`-File` の無い位置引数を `-Command` として実行する。
#[test]
fn windows_powershell_positional_arguments_are_a_command() {
    assert!(stopped(r"powershell Remove-Item -Recurse -Force C:\important", Dialect::Bash));
    assert!(stopped(r"powershell -NoProfile -ExecutionPolicy Bypass Remove-Item -Recurse C:\x", Dialect::Bash));
    assert!(!blocked("powershell -File scripts/x.ps1", Dialect::Bash));
    assert!(!blocked("powershell -NoProfile Get-ChildItem", Dialect::Bash));
}

/// `bash -ce '…'` と `bash -c -x '…'` のスクリプトも読む。
#[test]
fn bash_c_with_combined_or_following_options_is_analyzed() {
    assert!(blocked("bash -ce 'rm -rf /tmp/outside'", Dialect::Bash));
    assert!(blocked("bash -ec 'rm -rf /tmp/outside'", Dialect::Bash));
    assert!(blocked("bash -c -x 'rm -rf /tmp/outside'", Dialect::Bash));
    assert!(!blocked("bash -ce 'echo hi' arg0", Dialect::Bash));
}

/// 引用符の外の `$X` が空白を含む値に展開されたら、複数の対象になりうるので 1 つのパスと見なさない。
#[test]
fn unquoted_expansion_with_spaces_is_not_a_single_path() {
    assert!(stopped(&format!("X='{SCRATCH_FWD}/work /outside'; rm -rf $X"), Dialect::Bash));
    assert!(!blocked(&format!(r#"X='{SCRATCH_FWD}/my work'; rm -rf "$X""#), Dialect::Bash));
    assert!(!blocked(&format!("X={SCRATCH_FWD}/work; rm -rf $X"), Dialect::Bash));
}

/// `&&` / `||` / `|` の後の `cd` は実行されないことがある。チェーンを抜けた後の場所は確かでない。
#[test]
fn a_conditional_cd_does_not_carry_past_the_chain() {
    assert!(stopped(&format!("false && cd {SCRATCH_FWD}; rm -rf ."), Dialect::Bash));
    assert!(stopped(&format!("cd {SCRATCH_FWD} || rm -rf ."), Dialect::Bash));
    assert!(stopped(&format!("cd {SCRATCH_FWD} | rm -rf ."), Dialect::Bash));
    assert!(!blocked(&format!("cd {SCRATCH_FWD} && rm -rf x"), Dialect::Bash));
    assert!(!blocked(&format!("cd {SCRATCH_FWD}; rm -rf x"), Dialect::Bash));
}

/// `find` のパスより前のオプションを読み飛ばす。飛ばさないと対象を `.` と誤る。
#[test]
fn find_leading_options_are_skipped_before_the_paths() {
    let in_scratch = format!("cd {SCRATCH_FWD}; ");
    assert!(blocked(&format!("{in_scratch}find -L /important -delete"), Dialect::Bash));
    assert!(blocked(&format!("{in_scratch}find -H -D tree /important -delete"), Dialect::Bash));
    assert!(blocked(&format!("{in_scratch}find -O3 /important -delete"), Dialect::Bash));
    assert!(!blocked(&format!("{in_scratch}find -L . -name '*.tmp' -delete"), Dialect::Bash));
}

/// `C:foo` / `C:..\x` はドライブ `C:` のカレントディレクトリ基準で、場所が分からない。
#[test]
fn drive_relative_paths_are_unknown() {
    assert!(stopped(r"Remove-Item C:..\..\x.txt", Dialect::PowerShell));
    assert!(stopped(r"Remove-Item C:notes.txt", Dialect::PowerShell));
}

/// 見出しは場所を限定しない。プロジェクトの中の再帰削除で止めたときに「外」と書かない。
#[test]
fn the_heading_does_not_claim_the_target_is_outside() {
    let found = violations("rm -rf src", Dialect::Bash, &ctx());
    let text = message(&found, &ctx());
    assert!(text.starts_with("**削除系コマンドがブロックされました**"), "{text}");
}

/// `-ErrorAction SilentlyContinue` などの共通パラメータの値を、削除の対象と読まない。
#[test]
fn common_parameter_values_are_not_targets() {
    let command = format!(r"Remove-Item -Recurse -Force {SCRATCH}\work -ErrorAction SilentlyContinue");
    assert!(!blocked(&command, Dialect::PowerShell));
    assert!(!blocked(&format!(r"Remove-Item -Recurse {SCRATCH}\work -ea 0 -ov removed"), Dialect::PowerShell));
    assert!(blocked(r"Remove-Item -Recurse C:\important -ErrorAction SilentlyContinue", Dialect::PowerShell));
}

/// パイプの後ろのシェルが標準入力をスクリプトとして実行するなら、中身が見えないので確認する。
#[test]
fn a_script_piped_into_a_shell_is_unknown() {
    for command in [
        "echo 'rm -rf ~' | bash",
        "printf 'rm -rf /c/Users/u' | sh -x",
        "echo 'rm -rf ~' | bash -s arg",
        "echo 'rm -rf ~' | bash -o pipefail",
        r"echo 'Remove-Item -Recurse C:\x' | pwsh -NoProfile -Command -",
        r"echo 'Remove-Item -Recurse C:\x' | powershell -NoProfile",
        r"echo rd /s /q C:\x | cmd",
    ] {
        assert!(stopped(command, Dialect::Bash), "{command}");
    }
    assert!(!stopped("echo hi | bash -c 'cat'", Dialect::Bash));
    assert!(!stopped("cat input.txt | bash scripts/x.sh", Dialect::Bash));
    assert!(!stopped("echo hi | pwsh -File scripts/x.ps1", Dialect::Bash));
    assert!(!stopped("bash scripts/x.sh", Dialect::Bash));
}

/// ブレース展開の候補は別々のパスで、絶対パスも書けるので、固定部分では場所を決めない。
#[test]
fn brace_expansion_targets_are_unknown() {
    assert!(stopped(&format!("rm -rf {SCRATCH_FWD}/{{a,/c/Users/u}}"), Dialect::Bash));
    assert!(stopped(&format!("rm -rf {SCRATCH_FWD}/{{a,/../x}}"), Dialect::Bash));
    assert!(stopped("rm -rf __{x,/home/u}", Dialect::Bash));
    assert!(stopped("rm {a,/etc/passwd}", Dialect::Bash));
    assert!(stopped(&format!("rm -rf {SCRATCH_FWD}/run{{1..3}}"), Dialect::Bash));
    assert!(!stopped(&format!("rm -rf {SCRATCH_FWD}/*.tmp"), Dialect::Bash));
}

/// `git -C <dir>` などのグローバルオプションを読み飛ばして `rm` / `clean` を見つける。`-C` は基準の
/// 場所を変えるので、対象は判定できない。ディスクから消さない `--cached` と dry-run は止めない。
#[test]
fn git_global_options_and_non_deleting_forms() {
    assert!(stopped("git -C /outside rm -rf x", Dialect::Bash));
    assert!(stopped("git -C /outside rm x.txt", Dialect::Bash));
    assert!(stopped("git -c core.x=y clean -fdx", Dialect::Bash));
    assert!(stopped("git --work-tree=/outside clean -fd", Dialect::Bash));
    assert!(!blocked("git rm --cached -r src", Dialect::Bash));
    assert!(!blocked("git clean -nd", Dialect::Bash));
    assert!(!blocked("git clean --dry-run -d", Dialect::Bash));
    assert!(!blocked("git -c color.ui=never log --grep rm", Dialect::Bash));
}

/// `git` を中継の走査対象 (`relayed_deletion`) に含めると、`--grep` の検索語としてたまたま
/// 現れた裸の `rm` の直後に pathspec が続くだけで中継呼び出しと誤認し、削除を意図しない
/// 読み取り専用コマンドを block していた (SIM-NEW-deletions-rs-L61)。`git` 自身の `rm`/`clean` は
/// `git_deletion` が subcommand を直接見るので、`git` を中継の走査対象から外しても見逃さない。
#[test]
fn git_log_grep_rm_with_a_trailing_pathspec_is_not_a_relayed_deletion() {
    assert!(!blocked("git log --grep rm -- /outside/file.txt", Dialect::Bash));
    assert!(!blocked("git commit -m rm /outside/file.txt", Dialect::Bash));
}

/// 前置き (`sudo` / `env` / `timeout` など) の後ろで探すコマンドは、削除・シェル・cd・別名を判定する
/// 関数そのものに聞く。名前の一覧を複製すると判定の側とずれて、後ろのコマンドを読み飛ばしていた。
#[test]
fn commands_after_a_prefix_are_found_by_the_same_detectors() {
    assert!(blocked("sudo git clean -fdx /important", Dialect::Bash));
    assert!(stopped("env GIT_DIR=x git -C /outside rm -rf y", Dialect::Bash));
    assert!(stopped(&format!("pushd {SCRATCH_FWD}; env popd; rm -rf x"), Dialect::Bash));
    assert!(blocked("alias z='rm -rf'; sudo z /outside", Dialect::Bash), "sudo の後ろの別名も展開して読む (SEC-NEW-commands-mod-rs-L438)");
    assert!(blocked("alias cat='rm -rf'; timeout 5 cat /outside", Dialect::Bash), "許可リストにある名前への別名差し替えも展開して読む");
    assert!(blocked("timeout 5 find /important -delete", Dialect::Bash));
    assert!(stopped("nice -n 5 eval 'rm -rf /outside'", Dialect::Bash));
    assert!(!blocked("timeout 5 find . -name x", Dialect::Bash));
    assert!(!blocked("sudo git log --oneline", Dialect::Bash));
    assert!(!blocked("alias ll='ls -la'; sudo ll /outside", Dialect::Bash), "削除を含まない別名は前置き越しでも止めない");
}

/// `awk`/`sed` を「引数をデータとして扱うだけのコマンド」として `passes_words_as_data` に
/// 含めていたため、両者が自身の引数 (スクリプト文字列) の中に持つシェル実行プリミティブ
/// (`awk` の `system()`、`sed` の `e` コマンド・フラグ) による迂回が、直接呼び出しでも
/// 中継コマンド越しでも無検知だった (SEC-NEW-deletions-rs-L72)。通常のフィルタ用途
/// (`awk '{print $1}'` / `sed 's/old/new/g'`) は対象にしない。
#[test]
fn awk_system_and_sed_e_command_are_recognized_as_shell_escapes() {
    assert!(stopped(r#"awk 'BEGIN{system("rm -rf /outside")}'"#, Dialect::Bash));
    assert!(stopped("sed '1e rm -rf /outside' /dev/null", Dialect::Bash));
    assert!(stopped(r#"busybox awk 'BEGIN{system("rm -rf /outside")}'"#, Dialect::Bash));
    assert!(stopped("busybox sed '1e rm -rf /outside' /dev/null", Dialect::Bash));
    assert!(stopped("sed 's/x/y/e' /dev/null", Dialect::Bash));
    assert!(!stopped("awk '{print $1}' file.txt", Dialect::Bash));
    assert!(!stopped(r#"gh api x --jq '.body' | sed 's/<[^>]*>//g'"#, Dialect::Bash));
}

/// `gawk` は GNU awk の別バイナリ名で allowlist には awk と並んで確認なし実行可として登録済みだが、
/// `deletion()` の match 分岐が `awk` のみで `gawk` を見ていなかったため `system()` 経由のシェル実行が
/// 無検知で通っていた (SEC-NEW-deletions-rs-L48)。`awk` と同じ入力を `gawk` に変えても同様に検知する
/// ことを固定する。
#[test]
fn gawk_system_call_is_recognized_as_shell_escape_same_as_awk() {
    assert!(stopped(r#"gawk 'BEGIN{system("rm -rf /outside")}'"#, Dialect::Bash));
    assert!(stopped(r#"busybox gawk 'BEGIN{system("rm -rf /outside")}'"#, Dialect::Bash));
    assert!(!blocked("gawk '{print $1}' file.txt", Dialect::Bash));
}

/// awk のパイプでの起動と変数を含むスクリプト、sed のアドレス付きの `e` も読む。sed / awk は語を
/// データとして扱うコマンドのままなので、引数の `rm` という語を削除と読まない。
#[test]
fn awk_and_sed_shell_escapes_in_other_forms() {
    assert!(stopped(r#"awk 'BEGIN{print "rm -rf /outside" | "sh"}'"#, Dialect::Bash));
    assert!(stopped(r#"awk 'BEGIN{"rm -rf /outside" | getline x}'"#, Dialect::Bash));
    assert!(stopped(r#"awk "BEGIN{system(\"$CMD\")}""#, Dialect::Bash));
    assert!(stopped("sed '/x/e rm -rf /outside' f.txt", Dialect::Bash));
    assert!(!blocked("sed -i 's/a/b/' rm", Dialect::Bash));
    assert!(!blocked("awk -F, '{print $2}' rm -r", Dialect::Bash));
    assert!(!blocked("awk '/a|b/ {print}' x.txt", Dialect::Bash));
}

/// awk の文字列の中の `|` は出力する文字で、パイプではない (表の整形でよく使う)。
#[test]
fn a_pipe_character_inside_an_awk_string_is_not_a_pipe() {
    assert!(!blocked(r#"grep x f.md | awk -F'|' '{print $2"|"$3" | "$5}'"#, Dialect::Bash));
    assert!(!blocked(r#"awk '{print "a \"|\" b"}' f.txt"#, Dialect::Bash));
}

/// 算術展開 `$(( … ))` の式はコマンドではない。中のコマンド置換だけを読む。
#[test]
fn arithmetic_expansion_is_not_a_command() {
    assert!(!blocked(r#"start=$(date +%s); echo "elapsed=$(( $(date +%s) - start ))s""#, Dialect::Bash));
    assert!(!blocked("c=0; c=$((c+1)); echo $c", Dialect::Bash));
    assert!(blocked("echo $(( $(rm -rf /outside; echo 1) + 1 ))", Dialect::Bash));
}

/// `$((` を数万個連ねた入力でもスタックオーバーフローせずに終わる (深さの上限を超えたら、それ以上は
/// 中を読まず置換の本文をそのまま返す、SEC-NEW-scan-rs-L72)。実行できることそのものが回帰の検査で、
/// クラッシュしていればこのテスト自体がプロセスの異常終了で落ちる。深さの上限を超えた先は
/// 判定できない対象として止まる (ADR-043 の fail-closed)。
///
/// `"$("` ではなく `"$(("` を連ねる — `$(` だけを繰り返すと `"(("` の隣接が一度も生じず、
/// `command_substitutions_at_depth` の自己再帰 (`body.starts_with('(')`) に一度も入らないまま
/// `balanced_parens` が閉じを見つけられず即座に返ってしまい、このテストは深さ上限を一度も
/// 経由せずに通ってしまっていた (SIM-NEW-tests-review-rs-L256)。
#[test]
fn deeply_nested_arithmetic_substitution_does_not_overflow_the_stack() {
    let nested = "$((".repeat(50_000);
    let command = format!("echo {nested}");
    assert!(stopped(&command, Dialect::Bash));
}

/// 二重引用符とコマンド置換を交互に数万段連ねた入力でもスタックオーバーフローせずに終わる。
/// `matching_close` (`"` を見ると `closing_double_quote` を呼ぶ) と `closing_double_quote`
/// (`$(` を見ると `matching_close` を呼ぶ) の相互再帰には [`MAX_SUBSTITUTION_DEPTH`] の上限が
/// 及ばず残っていた (SEC-NEW-scan-rs-L9)。このテストも実行できること自体が回帰の検査で、
/// クラッシュしていればプロセスの異常終了で落ちる。
#[test]
fn deeply_alternating_quote_and_substitution_does_not_overflow_the_stack() {
    let nested = r#""$("#.repeat(50_000);
    let command = format!("echo $({nested}");
    assert!(stopped(&command, Dialect::Bash));
}

/// 引用符の中の空白はコマンド名の一部 (case のパターンなど)。分割される引用符の外の展開だけを、
/// 判定できないとする。
#[test]
fn a_quoted_command_name_with_spaces_is_one_name() {
    let command = r#####"while read -r l; do case "$l" in "#### Tier"*) t=1;; "| "[0-9]*) echo "$l";; esac; done < f.md"#####;
    assert!(!blocked(command, Dialect::Bash));
    assert!(asked(r#"CMD="rm -rf /outside"; $CMD"#, Dialect::Bash));
}

/// `**` (Bash の globstar) は `shopt -s globstar` 時に 0 階層にもマッチできるため、
/// `proj/**/../outside` は実行時に `proj/../outside` (プロジェクトの外) に展開され得る。
/// `**` の直後の `..` を無条件に 1 階層分として畳むと、常に `proj` (プロジェクトの中) に
/// なってしまい両者が食い違うので、判定できないもの (`None`) として fail-closed に倒す
/// (SEC-NEW-paths-rs-L52)。単一の `*` は常に実在する 1 階層にしかマッチしないので畳んでよい。
#[test]
fn double_star_glob_before_dotdot_is_not_folded_into_a_real_level() {
    assert_eq!(paths::normalize("/proj/**/../outside", None, None), None);
    assert_eq!(paths::normalize("/proj/*/../outside", None, None).as_deref(), Some("/proj/outside"));
}

/// `pwsh -Command:'...'` のように、PowerShell の名前付きパラメータの汎用記法 (`-ParameterName:Value`)
/// で `:` に値を直接続けると、字句解析で 1 語に結合される。元の `Flag::matches` はこの語のどの部分も
/// `-Command` 相当と認識できず、`nested_shell` が中身を一度も解析しないまま素通りしていた
/// (SEC-NEW-nested-rs-L403)。
#[test]
fn powershell_command_colon_glued_value_is_read() {
    assert!(blocked(r"pwsh -Command:'Remove-Item -Recurse -Force C:\important'", Dialect::Bash));
    assert!(blocked(r"pwsh -c:'Remove-Item -Recurse -Force C:\important'", Dialect::Bash));
    assert!(!blocked(r#"pwsh -Command:"Get-ChildItem x""#, Dialect::Bash), "削除の無い中身は止めずに確認する");
}

/// `format-*` の前方一致は、読み取り専用のコマンドレット向けのつもりが、ディスク全消去の実在
/// コマンドレット `Format-Volume` にも一致してしまい、`deletion()` に対応する分岐が無いまま
/// block/ask 無しで素通りしていた (SEC-NEW-allowlist-rs-L57)。
#[test]
fn format_wildcard_no_longer_matches_format_volume() {
    assert!(asked("Format-Volume -DriveLetter D -FileSystem NTFS", Dialect::PowerShell));
    assert!(!asked("Format-Table -AutoSize", Dialect::PowerShell), "読み取り専用の Format-Table は確認しない");
}

/// レジストリ (`HKLM:`/`HKCU:`/`Registry::`) / 証明書ストア (`Cert:`) / WS-Management 設定
/// (`WSMan:`) は、`Env:` のような揮発性のプロバイダと違い実体が永続的に残るのに、同じ
/// `Place::NotFile` (常に許可) として block/ask 無しで素通りしていた (SEC-NEW-paths-rs-L129)。
#[test]
fn persistent_powershell_providers_are_not_assumed_safe_to_delete() {
    assert!(stopped(r"Remove-Item -Recurse -Force HKLM:\Software\x", Dialect::PowerShell));
    assert!(stopped(r"Remove-Item -Recurse -Force HKCU:\Software\x", Dialect::PowerShell));
    assert!(stopped(r"Remove-Item -Recurse -Force Cert:\LocalMachine\My", Dialect::PowerShell));
    assert!(stopped(r"Remove-Item -Recurse -Force WSMan:\localhost\Listener", Dialect::PowerShell));
    assert!(stopped(r"Remove-Item -Recurse -Force Registry::HKEY_LOCAL_MACHINE\Software\x", Dialect::PowerShell));
    assert!(!stopped("Remove-Item Env:FOO", Dialect::PowerShell), "揮発性の Env: は従来通り止めない");
}

/// CPython の `-c'CODE'` / Node の `-e'CODE'`・`-p'CODE'` は、空白を挟まない短縮記法 (`bash -c` と
/// 同じ) で 1 語に結合される。`inline_flag` の完全一致判定はこの連結形を見落とし、確認なしで
/// 通過していた (SEC-NEW-nested-rs-L80)。
#[test]
fn inline_interpreter_flags_glued_to_the_code_are_asked() {
    let inline = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).inline_code;
    assert!(!inline(r#"python -c'import shutil; shutil.rmtree("/x")'"#).is_empty());
    assert!(!inline(r#"node -e'require("fs").rmSync("/x")'"#).is_empty());
    assert!(!inline("node -p'1+1'").is_empty());
    assert!(inline("python -Wignore scripts/x.py").is_empty(), "-W の値が c/e/p で始まっても inline code ではない");
}

/// `-EncodedCommand:<base64>` のように `:` に値を直接続けた短縮形は、スペース区切り形
/// (`-EncodedCommand <base64>`) と同じく中身の分からないコードとして確認するべきだが、
/// `is_encoded_command_flag` が `:` 以降を含めたまま前方一致判定していたため、どの省略形
/// (`-encodedcommand`/`-ec`) にも一致せず無音で通過していた (SEC-NEW-nested-rs-L377)。
#[test]
fn encoded_command_with_colon_attached_value_is_blocked() {
    assert!(stopped("pwsh -EncodedCommand:ZQBjAGgAbwAgAGgAaQA=", Dialect::Bash));
    assert!(stopped("pwsh -enc:ZQBjAGgAbwA=", Dialect::Bash));
    assert!(stopped("pwsh -ec:ZQBjAGgAbwA=", Dialect::Bash));
    assert!(stopped("powershell -EncodedCommand:ZQBjAGgAbwA=", Dialect::Bash));
}

/// Bash の nameref (`declare -n ref=S`) への書き込みは、結んだ実体の変数を書き換える。結び先を
/// 個別に追っていた頃は、2 文に分けた宣言や実体への入れ直しを読み落として確認なしで通していた
/// (SEC-NEW-rebinds-rs-L33、SIM-NEW-rebinds-rs-L45)。nameref を宣言したら、以後はどの変数の値も
/// 追わずに確認する (ADR-086 決定 7)。`-rn` のようにまとめたオプションも nameref。
#[test]
fn after_a_nameref_no_variable_is_tracked() {
    for declare in ["declare -n ref=S", "declare -n ref; ref=S", "local -n ref=S", "declare -rn ref=S", "typeset --nameref ref=S"] {
        for tail in [r#"ref="/outside"; rm -rf "$S""#, r#"rm -rf "$ref""#, r#"S="SCRATCH/work"; read ref <<< /outside; rm -rf "$S""#] {
            let command = format!(r#"S="{SCRATCH_FWD}/work"; {declare}; {}"#, tail.replace("SCRATCH", SCRATCH_FWD));
            assert!(stopped(&command, Dialect::Bash), "{command}");
        }
    }
    assert!(!stopped(&format!(r#"declare -r X=1; S="{SCRATCH_FWD}/work"; rm -rf "$S""#), Dialect::Bash), "nameref でなければ追う");
}
