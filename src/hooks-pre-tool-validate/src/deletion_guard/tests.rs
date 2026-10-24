//! deletion_guard のテスト。場所の基準は固定の値にし、実行環境のパスに依存させない。

use super::*;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: Some(MEMORY.to_string()),
        home: Some("C:/Users/u".to_string()),
    }
}

const MEMORY: &str = "C:/Users/u/.claude/projects/c--work-proj/memory";

#[test]
fn single_files_in_the_claude_memory_can_be_deleted_but_not_the_whole_directory() {
    assert!(!blocked(&format!("rm {MEMORY}/old-note.md"), Dialect::Bash));
    assert!(!blocked(
        r"Remove-Item C:\Users\u\.claude\projects\c--work-proj\memory\old-note.md",
        Dialect::PowerShell
    ));
    assert!(blocked(&format!("rm -rf {MEMORY}"), Dialect::Bash));
    assert!(blocked("rm C:/Users/u/.claude/projects/other-proj/memory/x.md", Dialect::Bash));
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

fn assert_all_stopped(commands: &[&str], dialect: Dialect) {
    for command in commands {
        assert!(stopped(command, dialect), "止めるか確認するべき: {command}");
    }
}

fn assert_all(commands: &[&str], dialect: Dialect, expect_blocked: bool) {
    for command in commands {
        assert_eq!(
            blocked(command, dialect),
            expect_blocked,
            "{} であるべき: {command}",
            if expect_blocked { "止める" } else { "通す" }
        );
    }
}

#[test]
fn bash_deletions_inside_the_scratchpad_are_allowed() {
    assert_all(
        &[
            r#"rm -rf "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad/x""#,
            "rm -rf /c/Users/u/AppData/Local/Temp/claude/p/s/scratchpad/probe",
            r#"S="C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad"; rm -rf "$S/x""#,
            r#"S='/c/Users/u/AppData/Local/Temp/claude/p/s/scratchpad' && rm -rf "$S"/*"#,
        ],
        Dialect::Bash,
        false,
    );
}

/// `VAR=value cmd` (セミコロン・`&&` を挟まない前置きの代入) と、値を別の語で取る前置きの
/// オプション (`sudo -u user` / `nice -n 10` / `xargs -n 1` / `env -u NAME` / `timeout 5`) で、
/// 後ろの `rm` が解析から脱落しないことを固定する (pre-push review の指摘)。
#[test]
fn bash_prefixes_with_assignments_or_option_values_are_not_a_bypass() {
    assert_all_stopped(
        &[
            r#"A=/tmp/x rm -rf "$A""#,
            r#"A=1 B=/tmp/x rm -rf "$B""#,
            "sudo -u root rm -rf /var",
            "sudo --user=root rm -rf /var",
            "nice -n 10 rm -rf /var",
            "timeout 5 rm -rf /var",
            "env -u FOO rm -rf /var",
            "ls | xargs -n 1 rm -rf",
            "sudo -u root env A=1 nice -n 5 rm -rf /var",
        ],
        Dialect::Bash,
    );
}

/// 前置きの代入は、同じコマンドの引数の展開には効かない (Bash では `$S` は代入より前に外側の
/// 値で展開される)。外側で未定義の `$S` を scratchpad と読み違えて通してはならない。
/// pre-push の fix step はここを逆に「通す」と固定していた (`S=<scratchpad> rm -rf "$S/x"` は
/// 実際には `rm -rf "/x"` になる)。
#[test]
fn a_prefix_assignment_does_not_resolve_the_same_commands_arguments() {
    assert!(stopped(
        r#"S="C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad" rm -rf "$S/x""#,
        Dialect::Bash
    ));
    assert!(!blocked(
        r#"S="C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad"; A=1 rm -rf "$S/x""#,
        Dialect::Bash
    ));
}

#[test]
fn bash_deletions_of_double_underscore_temp_files_are_allowed() {
    assert_all(
        &[
            "rm -rf __tmp_dir",
            "rm -rf ./__x",
            "rm __dump.json",
            "rm -f __*",
            "cd /c/work/proj; rm -rf __x 2>&1 | tail -1",
            "rm -f __x > /dev/null",
        ],
        Dialect::Bash,
        false,
    );
}

#[test]
fn bash_single_file_deletions_inside_the_project_are_allowed() {
    assert_all(
        &["rm .tmp-pr-body.md", "rm docs/old.md", "rm -f *.log", "rmdir __empty"],
        Dialect::Bash,
        false,
    );
}

#[test]
fn bash_commands_that_only_mention_deletion_are_allowed() {
    assert_all(
        &[
            r#"grep -rn "rm -rf /" src/"#,
            r#"echo "rm -rf ~""#,
            r#"jj describe -m "rm -rf の説明""#,
            "ls | grep x",
            "cargo test -p hooks-pre-tool-validate",
        ],
        Dialect::Bash,
        false,
    );
}

#[test]
fn bash_deletions_outside_the_project_are_blocked() {
    assert_all(
        &[
            "rm -rf /tmp/x",
            "rm -r ~/x",
            "rm ~/.bashrc",
            "rm C:/Users/u/x.txt",
            "rm -rf ../other",
            "rm -rf src/../../x",
            "rm -rf /tmp/*",
            "sudo rm -rf /var",
            "rm -rf /c/Users/u/AppData/Local/Temp/claude/p/other/scratchpad",
        ],
        Dialect::Bash,
        true,
    );
}

#[test]
fn bash_recursive_deletions_inside_the_project_are_blocked() {
    assert_all(
        &["rm -rf src", "rm -r src", "rm -rf .", "rm -rf ..", "find . -delete", "npx rimraf node_modules"],
        Dialect::Bash,
        true,
    );
}

/// `git rm` / `git clean` は `git_deletion` が subcommand を直接見て検知するので、`git` 自身は
/// 中継の走査対象 (`passes_words_as_data`) から外さなくても直接呼び出し・`busybox git rm ...`
/// 越しのどちらも見逃さない。外すと `git log --grep rm` のような無関係な subcommand でも
/// 誤検知する (SIM-NEW-deletions-rs-L61)。
#[test]
fn git_rm_and_git_clean_are_recognized_as_deletions() {
    assert!(!blocked("git rm old.txt", Dialect::Bash));
    assert!(blocked("git rm -r src", Dialect::Bash));
    assert!(blocked("git rm -rf /outside", Dialect::Bash));
    assert!(blocked("git clean -fdx", Dialect::Bash));
    assert!(blocked("busybox git rm -rf /outside", Dialect::Bash));
    assert!(!blocked("git log --grep rm", Dialect::Bash));
}

/// Bash の `*>out.txt` (空白なしのリダイレクト) は、glob 対象 `*` ではなく fd 番号の `*>` 記法
/// (PowerShell 限定) として誤って食われてはならない — 食われると `rm -rf *>out.txt` の対象が
/// 空になり素通りする (SIM-NEW-lexer-rs-L252)。
#[test]
fn bash_redirect_after_glob_star_does_not_eat_the_glob_target() {
    assert!(blocked("rm -rf *>out.txt", Dialect::Bash));
}

/// どこを消すか判定できない削除は確認する (ADR-086 決定 7)。
#[test]
fn bash_deletions_with_unknown_targets_are_asked() {
    assert_all_stopped(
        &[
            r#"rm -rf "$HOME/x""#,
            "rm -rf $S/x",
            "rm -rf $(pwd)/x",
            "rm -rf `pwd`/x",
            "ls | xargs rm",
            r#"cd "$D" && rm x"#,
            "find / -name x -delete",
            "find . -name '*.tmp' -exec rm {} +",
        ],
        Dialect::Bash,
    );
}

/// `find -exec` は起動するコマンドに関わらず確認する (ADR-086 決定 7)。`-exec` の後ろの `shred` を
/// 削除と認識できずに、`find` の探すパスを見落としていたこともあった (SEC-NEW-commands-rs-L472)。
#[test]
fn bash_find_exec_shred_is_asked() {
    assert!(stopped(r"find /outside -exec shred -u {} \;", Dialect::Bash));
}

/// `{}` は後から実際のパスに置き換わるプレースホルダなので、削除の対象として読んだら判定できない。
#[test]
fn a_placeholder_target_is_unknown() {
    assert!(stopped("parallel rm -f {} ::: a b", Dialect::Bash));
    assert!(stopped("ls | xargs -I {} rm {}", Dialect::Bash));
}

/// `track_directory` は `push-location` を `cd` 相当として追跡するが、前置き (`sudo` /
/// `env` など) の後ろで気にすべきコマンドを探す `is_watched` には元々 `push-location` が
/// 無く、前置き付きの呼び出しで cwd 追跡が丸ごと抜けていた (SIM-NEW-commands-rs-L211)。
#[test]
fn push_location_behind_a_prefix_is_still_tracked_for_cwd() {
    assert!(!blocked(&format!(r"env Push-Location {SCRATCH}; Remove-Item -Recurse x"), Dialect::PowerShell));
}

#[test]
fn cd_is_followed_when_resolving_relative_targets() {
    assert!(blocked("cd /tmp && rm -rf x", Dialect::Bash));
    assert!(blocked("cd /tmp && rm x", Dialect::Bash));
    assert!(!blocked("cd /c/work/proj && rm -rf __x", Dialect::Bash));
    assert!(!blocked(
        "cd /c/Users/u/AppData/Local/Temp/claude/p/s/scratchpad && rm -rf probe",
        Dialect::Bash
    ));
}

#[test]
fn nested_shells_are_analyzed_with_their_own_syntax() {
    assert_all(
        &[
            r#"bash -c "rm -rf /tmp/x""#,
            r#"pwsh -c "Remove-Item -Recurse C:\Users\u\x""#,
            r#"cmd /c "rd /s /q C:\x""#,
        ],
        Dialect::Bash,
        true,
    );
    assert!(stopped(r#"powershell -Command "Get-ChildItem src | Remove-Item""#, Dialect::Bash));
    assert_all(
        &[r#"cmd /c "rd /s /q __x""#, r#"bash -c "rm -rf __x" ignored-arg0"#],
        Dialect::Bash,
        false,
    );
}

/// `bash -c'...'` のようにクォートが `-c` に空白無しで直接続く書き方は、POSIX の getopt 式
/// 短縮オプション (`-cVALUE` ≡ `-c VALUE`) により、空白を挟んだ `bash -c "..."` と完全に
/// 等価に動作する。字句解析ではこの形が `-c` と中身を 1 つの語に連結するので、連結を見ずに
/// 次の語だけを script として読むと削除コマンドが一度も解析されず通過してしまう
/// (SEC-NEW-commands-rs-L348)。`pwsh` の `-Command` はこの getopt 式の値連結をサポートしない
/// ので対象外 (実際の pwsh バイナリはこの書き方を解釈できず、別の経路でエラーになる)。
#[test]
fn bash_c_with_no_space_before_the_quoted_script_is_still_analyzed() {
    assert!(blocked(r#"bash -c'rm -rf /tmp/important'"#, Dialect::Bash));
    assert!(blocked(r#"bash -c"rm -rf /tmp/important""#, Dialect::Bash));
    assert!(!blocked(r#"bash -c'echo hi'"#, Dialect::Bash));
    assert!(blocked(r#"bash -xc'rm -rf /tmp/important'"#, Dialect::Bash), "結合した短縮オプションでも読む");
}

/// `bash -c`/`pwsh -c`/`Start-Process` は子プロセスを spawn するため、実シェルでは内部の
/// `cd`/変数代入が外側に漏れない。`nested_shell`/`start_process` が `state.cwd`/`state.vars` を
/// save/restore せず共有していたため、子プロセス内の変化が外側に残ったまま後続コマンドを
/// 誤った場所 (scratch 配下) と取り違えて fail-open に通してしまっていた
/// (SIM-NEW-commands-rs-L503)。`cd /tmp && bash -c "cd <scratch>"` の直後の `rm -rf x` は、
/// 実シェルでは `/tmp` 基準で解決される (scratch への漏れが起きれば fail-open で通ってしまう)。
#[test]
fn subprocess_state_changes_do_not_leak_into_the_outer_state() {
    let scratch_fwd = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert!(blocked(&format!(r#"cd /tmp && bash -c "cd {scratch_fwd}" && rm -rf x"#), Dialect::Bash));
    assert!(blocked(
        &format!(r#"cd /tmp; Start-Process powershell -ArgumentList "-Command cd {scratch_fwd}"; Remove-Item -Recurse x"#),
        Dialect::PowerShell
    ));
    assert!(!blocked(
        &format!(r#"X="{scratch_fwd}"; bash -c "X=/tmp/outside"; rm -rf "$X""#),
        Dialect::Bash
    ));
}

/// Bash の `(...)` サブシェルは fork された子プロセスで動くため、中の `cd` は `)` を抜けた外側の
/// シェルに伝わらない。lexer が `(`/`)` を `;` と同じ単純な区切りとして扱い、`cd` の追跡を
/// サブシェル境界で save/restore していなかったため、`(cd <scratchpad>); rm -rf .` の `rm -rf .`
/// が scratchpad 配下と誤認され fail-open で通ってしまっていた (SEC-NEW-lexer-rs-L178)。同じ理由で
/// 単発のバックグラウンドジョブ (`cmd &`) も fork されるため、同様に scope する。
#[test]
fn subshell_and_background_job_cwd_changes_do_not_leak_to_the_following_command() {
    assert!(blocked(&format!(r#"(cd "{SCRATCH}"); rm -rf ."#), Dialect::Bash));
    assert!(blocked(&format!(r#"cd "{SCRATCH}" & rm -rf ."#), Dialect::Bash));
    assert!(!blocked(&format!(r#"(cd "{SCRATCH}" && rm -rf x)"#), Dialect::Bash));
}

/// コマンド名が解決できないとき、Bash では何を実行するか分からないので確認する (ADR-086 決定 5)。
/// PowerShell では変数で始まる式がふつうに書かれるので確認しない。
#[test]
fn an_unresolvable_command_name_is_asked_in_bash_but_not_in_powershell() {
    assert!(asked("`echo rm` -rf /tmp/important", Dialect::Bash));
    assert!(asked("$X -rf /tmp/important", Dialect::Bash));
    assert!(!blocked("$env:PATH -split ';'", Dialect::PowerShell));
    assert!(!blocked("$items.Count", Dialect::PowerShell));
}

/// 呼び出し演算子 `&` は明示的なコマンド呼び出しであり、裸の変数参照の式とは違う。先頭語が
/// 解決できないとき「式だから止めない」例外を適用すると、動的な呼び出し (`& $x ...`) が
/// 未検知で削除を実行してしまう (SEC-NEW-commands-rs-L123)。
#[test]
fn a_call_operator_with_an_unresolvable_command_name_is_asked_in_powershell() {
    assert!(asked(r#"$x = "Remove" + "-Item"; & $x -Recurse C:\important"#, Dialect::PowerShell));
    assert!(asked(r"& $x -Recurse C:\important", Dialect::PowerShell));
    assert!(!blocked("$env:PATH -split ';'", Dialect::PowerShell), "呼び出し演算子の無い式は止めない");
}

/// ドットソーシング演算子 `.` も `&` と同じく明示的なコマンド呼び出しであり、裸の変数参照の式
/// とは違う。`&` 版は検知済みだが、`.` 版は lexer が `.` を普通のリテラル語として読んでいたため
/// `segment.called` が立たず、動的な呼び出し (`. $x ...`) が未検知で削除を実行してしまう
/// (SEC-NEW-lexer-rs-L247)。空白無しで続く `.\foo` のような相対パス呼び出しは対象にしない
/// (この演算子ではない)。
#[test]
fn a_dot_source_operator_with_an_unresolvable_command_name_is_asked_in_powershell() {
    assert!(asked(r#"$x = "Remove" + "-Item"; . $x -Recurse C:\important"#, Dialect::PowerShell));
    assert!(asked(r"$cmd = $env:SOME_VAR; . $cmd -Recurse C:\project\src", Dialect::PowerShell));
    assert!(asked(r". $x -Recurse C:\important", Dialect::PowerShell));
    assert!(
        !blocked(r".\build.ps1 -Recurse C:\important", Dialect::PowerShell),
        "空白無しで続く相対パスはドットソーシング演算子ではない"
    );
}

/// `Start-Process` は新しいプロセスへの引数文字列をまるごと 1 語として渡す。`-ArgumentList` の
/// 値に、起動先シェルの `-Command` と削除コマンドが埋め込まれていても、`nested_shell` の対象
/// (先頭コマンド名と個々の ARGV 語の一致判定) をすり抜ける (SEC-NEW-commands-rs-L327)。
/// `-FilePath`/`-ArgumentList` を合成し、起動されるコマンドラインとして改めて読み直す。
#[test]
fn start_process_with_an_embedded_delete_command_is_blocked() {
    assert!(blocked(
        r#"Start-Process powershell -ArgumentList "-Command Remove-Item -Recurse -Force C:\important""#,
        Dialect::PowerShell
    ));
    assert!(blocked(
        r#"Start-Process -FilePath cmd -ArgumentList "/c rd /s /q C:\important""#,
        Dialect::PowerShell
    ));
    assert!(stopped(
        r#"Start-Process -ArgumentList "-Command Remove-Item -Recurse -Force C:\important" -FilePath powershell"#,
        Dialect::PowerShell
    ));
    assert!(
        blocked(r#"saps powershell -ArgumentList "-Command Remove-Item -Recurse C:\important""#, Dialect::PowerShell),
        "Start-Process の別名 saps でも読む"
    );
    assert!(
        blocked(r#"start powershell "-Command Remove-Item -Recurse C:\important""#, Dialect::PowerShell),
        "Start-Process の別名 start でも読む"
    );
    assert!(
        !blocked(r#"Start-Process powershell -ArgumentList "-Command Get-ChildItem""#, Dialect::PowerShell),
        "削除を含まない起動は通す"
    );
    assert!(
        !blocked(r#"Start-Process notepad.exe -ArgumentList "C:\notes.txt""#, Dialect::PowerShell),
        "シェル以外の起動は中身を解析できないので通す (ADR-086 の既知の限界)"
    );
}

/// heredoc の本文はコマンドではなく標準入力のデータ。ファイルに書く本文 (Markdown のバッククォート
/// や `rm -rf` の説明を含む) では止めない。シェルに渡したとき (`bash <<EOF`) だけ本文を読む。
#[test]
fn heredoc_bodies_are_data_unless_fed_to_a_shell() {
    let write_doc = "cat > __notes.md <<'EOF'\n`rm -rf /` は危険\n- `gh pr view` を使う\nEOF\necho done";
    assert!(!blocked(write_doc, Dialect::Bash));
    let write_after_pipe = "cat <<EOF | grep x\nrm -rf /tmp/x\nEOF";
    assert!(!blocked(write_after_pipe, Dialect::Bash));
    let run_script = "bash <<'EOF'\nrm -rf /tmp/x\nEOF";
    assert!(blocked(run_script, Dialect::Bash));
    let tab_stripped = "sh <<-EOF\n\trm -rf ~/x\n\tEOF\necho after";
    assert!(blocked(tab_stripped, Dialect::Bash));
    let after_heredoc = "cat > __a.txt <<'EOF'\nhello\nEOF\nrm -rf /tmp/y";
    assert!(blocked(after_heredoc, Dialect::Bash), "heredoc の後のコマンドは読む");
}

#[test]
fn powershell_deletions_in_temp_places_and_single_project_files_are_allowed() {
    assert_all(
        &[
            r#"Remove-Item -Recurse -Force "C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad\x""#,
            r#"$S = "C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad"; Remove-Item -Recurse "$S\x""#,
            "Remove-Item __dump.json",
            "Remove-Item .tmp-pr-body.md",
            r"ri -r __x",
            r"Remove-Item -Recurse:$false docs\x.md",
            "Remove-Item Env:FOO",
            r"Remove-Item -Recurse C:\x -WhatIf",
        ],
        Dialect::PowerShell,
        false,
    );
}

#[test]
fn powershell_commands_that_do_not_delete_are_allowed() {
    assert_all(
        &[
            "Get-ChildItem .claude -Filter *.toml | Select-Object -ExpandProperty Name; jj st",
            r#"Select-String -Pattern "Remove-Item -Recurse C:\" -Path src"#,
            r#"Get-ChildItem "C:\Users\owner\work\x\reports" -ErrorAction SilentlyContinue | Select-Object Name"#,
        ],
        Dialect::PowerShell,
        false,
    );
}

#[test]
fn powershell_recursive_deletions_and_their_aliases_are_blocked() {
    assert_all(
        &[
            "Remove-Item -Recurse -Force src",
            "rm -r -fo src",
            "ri src -rec",
            r"del -Recurse C:\Users\u\x",
            r"rd C:\x -Recurse",
            "& Remove-Item -Recurse src",
        ],
        Dialect::PowerShell,
        true,
    );
}

#[test]
fn powershell_deletions_outside_or_unknown_are_blocked() {
    assert_all(
        &[
            r"Remove-Item -Path C:\x\y.txt",
            r#"Remove-Item -LiteralPath "C:\x""#,
            r"Remove-Item -Path:C:\x",
            r"Remove-Item -Recurse ~\x",
            r"Remove-Item a,C:\x",
            r"cmd /c rd /s /q C:\x",
        ],
        Dialect::PowerShell,
        true,
    );
    assert_all_stopped(
        &[r"Remove-Item -Recurse $env:TEMP\x", "Get-ChildItem -Recurse src | Remove-Item", "gci | ri"],
        Dialect::PowerShell,
    );
}

/// `-WhatIf` 単体や `-WhatIf:$true` は dry run なので対象にしないが、`-WhatIf:$false` は
/// dry run を明示的に無効化しており実削除になる。`-Recurse:$false` とは非対称に、値を見ず
/// 無条件で `return None` していた fail-open を固定する (SEC-NEW-commands-rs-L475)。
/// `-WhatIf:0` のように `$false` という文字列に一致しない値も、真と断定できない以上は
/// dry run と見なさず実削除として扱う (SIM-NEW-commands-rs-L520)。
#[test]
fn what_if_false_is_a_real_delete_not_a_dry_run() {
    assert!(blocked(r"Remove-Item -Recurse C:\x -WhatIf:$false", Dialect::PowerShell));
    assert!(blocked(r"Remove-Item -Recurse C:\x -WhatIf:0", Dialect::PowerShell));
    assert!(!blocked(r"Remove-Item -Recurse C:\x -WhatIf:$true", Dialect::PowerShell));
    assert!(!blocked(r"Remove-Item -Recurse C:\x -WhatIf", Dialect::PowerShell));
}

/// 中継コマンド (`flock` / `setsid` / `strace` / `chrt` / `taskset` など、本体を変えずそのまま
/// 実行するもの) は `is_prefix` / `is_watched` のどちらにも無く、後ろに続く削除コマンドの
/// 引数を一切検査せず素通りしていた (SEC-NEW-commands-rs-L204)。
#[test]
fn relay_commands_do_not_hide_the_deletion_behind_them() {
    assert_all(
        &[
            "flock /tmp/x.lock rm -rf /tmp/x",
            "setsid rm -rf /tmp/x",
            "strace -f -o /tmp/log.txt rm -rf /tmp/x",
            "chrt -f 99 rm -rf /tmp/x",
            "taskset 0x1 rm -rf /tmp/x",
            "sudo flock /tmp/x.lock rm -rf /tmp/x",
        ],
        Dialect::Bash,
        true,
    );
    assert!(!blocked("flock /tmp/x.lock ls -la", Dialect::Bash));
}

/// 中継コマンドは名前の一覧では尽くせないので、知らないコマンドの引数に削除コマンドの名前が
/// 現れたら、そこから先を削除として読む。語をデータとして扱うコマンドは対象にしない。
#[test]
fn unknown_relays_are_caught_by_the_deletion_word_in_their_arguments() {
    assert_all(
        &[
            "busybox rm -rf /tmp/x",
            "wsl rm -rf /",
            "watch -n 5 rm -rf /tmp/x",
            "systemd-run --user rm -rf /tmp/x",
            "node tool.js rm -rf /tmp/x",
        ],
        Dialect::Bash,
        true,
    );
    assert!(blocked(r"Start-Job { } ; Invoke-Command -ScriptBlock { Remove-Item -Recurse C:\x }", Dialect::PowerShell));
    assert_all(
        &[
            "echo rm -rf /",
            "printf '%s' rm -rf /",
            "git log --grep rm",
            "jj describe -m rm",
            "grep -rn rm src/",
            "busybox ls -la",
        ],
        Dialect::Bash,
        false,
    );
}

/// `find` は空引数では削除と判定できない (`-delete`/`-exec rm` を実引数で見るまで判断不能)。
/// `relayed_deletion` が候補位置の判定に空引数 (`&[]`) を渡していたため、中継コマンド越しの
/// `find ... -delete` / `find ... -exec rm` は常に判定不能側に落ちて素通りしていた
/// (SEC-NEW-commands-rs-L611)。候補位置より後ろの実引数を渡して判定するよう修正した。
#[test]
fn unknown_relays_catch_find_only_once_its_real_arguments_are_seen() {
    assert!(blocked("busybox find /important -delete", Dialect::Bash));
    assert!(stopped(r"busybox find /outside -exec rm {} \;", Dialect::Bash));
    assert!(!blocked("busybox find /important -name x", Dialect::Bash));
}

/// `relayed_deletion` は削除コマンド名が独立した語として現れる場合しか検知しない。削除コマンドが
/// `bash -c "…"` のようにクォートで包まれたスクリプト文字列の中に隠れると、スクリプト全体が
/// 1 語になって中身の `rm` を `command_name` 経由では読めない。かつ `nested_shell` 自体は segment
/// 先頭が `bash`/`pwsh`/`cmd` のときしか呼ばれないため、未列挙の中継コマンド (`caffeinate` 等) 越しだと
/// 両経路を素通りしていた (SEC-NEW-deletions-rs-L41)。
#[test]
fn unknown_relays_reach_into_nested_shell_invocations() {
    assert_all_stopped(
        &[
            r#"caffeinate bash -c "rm -rf /important""#,
            r#"caffeinate pwsh -c "Remove-Item -Recurse C:\important""#,
            r#"caffeinate cmd /c "rd /s /q C:\important""#,
            r#"caffeinate bash -ce "rm -rf /important""#,
            r#"caffeinate powershell "Remove-Item -Recurse C:\important""#,
            r#"caffeinate pwsh -EncodedCommand AAAA"#,
            "echo 'rm -rf ~' | caffeinate bash",
        ],
        Dialect::Bash,
    );
    assert!(!blocked("caffeinate bash x.sh", Dialect::Bash));
    assert!(!blocked("npm run build -- --shell bash", Dialect::Bash));
}

#[test]
fn powershell_dotnet_deletes_are_checked() {
    assert_all(
        &[r#"[System.IO.Directory]::Delete("C:\x", $true)"#, r"[IO.File]::Delete('C:\Users\u\x.txt')"],
        Dialect::PowerShell,
        true,
    );
    assert_all_stopped(&[r"(Get-Item C:\x).Delete()", r"[IO.Directory]::Delete($p, $true)"], Dialect::PowerShell);
    assert!(!blocked(
        r"[IO.File]::Delete('C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad\x.txt')",
        Dialect::PowerShell
    ));
}

/// `New-Object -ComObject Scripting.FileSystemObject` が持つ `DeleteFolder`/`DeleteFile` は
/// 名前が `Delete` と一致せず、元の `OBJECT_CALL` (`\.delete\s*\(` のみ) にも
/// `STATIC_CALL` ([IO.Directory]/[IO.File] 限定) にも引っからず無条件に通っていた
/// (SEC-NEW-commands-rs-L748)。
#[test]
fn com_object_delete_folder_and_file_methods_are_checked() {
    assert_all_stopped(
        &[
            r#"(New-Object -ComObject Scripting.FileSystemObject).DeleteFolder("C:\important", $true)"#,
            r#"(New-Object -ComObject Scripting.FileSystemObject).DeleteFile("C:\important\x.txt")"#,
            r"$fso.DeleteFolder($p)",
        ],
        Dialect::PowerShell,
    );
}

/// `"$true"` への文字列一致だけを recursive とみなすと、`Delete(path, 1)` のような truthy な
/// 数値の渡し方をプロジェクト内の再帰削除と見抜けず fail-open になる (SIM-NEW-commands-rs-L603)。
/// `$false`/`0`/空だけを「断定できる false」として扱い、それ以外 (`1` を含む) は recursive と
/// みなすことで、プロジェクト内の再帰削除を検知する。
#[test]
fn dotnet_delete_recursive_flag_recognizes_truthy_values_other_than_true() {
    assert!(blocked(r#"[System.IO.Directory]::Delete("src", 1)"#, Dialect::PowerShell));
    assert!(!blocked(r#"[System.IO.Directory]::Delete("src", 0)"#, Dialect::PowerShell));
}

#[test]
fn the_message_names_each_blocked_target_and_why() {
    let found = violations("rm -rf /tmp/x src", Dialect::Bash, &ctx());
    let text = message(&found, &ctx());
    assert!(text.contains("`/tmp/x` — プロジェクトの外"), "{text}");
    assert!(text.contains("`src` — プロジェクトの中の再帰削除"), "{text}");
    assert!(text.contains(SCRATCH), "{text}");
}

#[test]
fn paths_are_normalized_across_shell_spellings() {
    assert_eq!(paths::normalize("/c/Work/Proj", None, None).as_deref(), Some("c:/work/proj"));
    assert_eq!(paths::normalize(r"C:\Work\a\..\b", None, None).as_deref(), Some("c:/work/b"));
    assert_eq!(paths::normalize("x/./y", Some("c:/p"), None).as_deref(), Some("c:/p/x/y"));
    assert_eq!(paths::normalize("~/x", None, Some("c:/users/u")).as_deref(), Some("c:/users/u/x"));
    assert_eq!(paths::normalize("rel", None, None), None);
    assert_eq!(paths::normalize("/tmp/X", None, None).as_deref(), Some("/tmp/X"));
}

/// `名前:` が 2 文字以上の英数字でも、既知の揮発性の PowerShell 組み込みプロバイダでなければ
/// ファイルではないと断定しない。`New-PSDrive` で任意のパスに割り当てられたドライブ名の
/// 可能性を排除できないため (SIM-NEW-paths-rs-L96)。レジストリ・証明書ストアのような永続的な
/// プロバイダは、allowlist 外のものと同じ `Place::Unknown` に倒す (SEC-NEW-paths-rs-L129)。
#[test]
fn strip_provider_only_recognizes_known_non_file_providers() {
    assert_eq!(paths::strip_provider("Env:FOO"), Err(Place::NotFile));
    assert_eq!(paths::strip_provider(r"HKLM:\Software\x"), Err(Place::Unknown));
    assert_eq!(paths::strip_provider(r"xx:\important"), Err(Place::Unknown));
    assert_eq!(paths::strip_provider(r"C:\x"), Ok(r"C:\x"), "ドライブレター (1 文字) はプロバイダではない");
}

/// `New-PSDrive` はこのガードの対象コマンドに入っていないので、作った任意名のドライブへの
/// 削除は「ファイルではない」と誤判定されず、判定できないものとして確認する。
#[test]
fn a_drive_created_by_new_psdrive_is_not_assumed_to_be_a_non_file_provider() {
    assert!(stopped(r"Remove-Item -Recurse xx:\", Dialect::PowerShell));
}

/// `within` は呼び出し元 (`classify`) が `None` を除外した後の実在する root だけを受け取る
/// 前提。root が空文字列 (ファイルシステムのルート相当に正規化された場合) を無条件の一致と
/// 扱うと、どのパスでもその場所区分に一致してしまい fail-closed に反する
/// (SIM-NEW-paths-rs-L89)。
#[test]
fn within_does_not_treat_an_empty_root_as_matching_every_path() {
    assert!(!paths::within("c:/work/proj", ""));
    assert!(paths::within("c:/work/proj/x", "c:/work/proj"));
}

/// Bash の未クォート変数展開は IFS で word-split され、`CMD="rm -rf /outside"; $CMD` は展開後
/// `rm -rf /outside` という複数語になる。解決値をそのまま 1 語のコマンド名として `command_name`
/// に渡すと `"rm -rf /outside"` → 最後の `/` 区切りの `"outside"` に変換され `rm` と一致せず、
/// 検知漏れになっていた (SEC-NEW-commands-rs-L168)。
#[test]
fn bash_variable_expanding_to_multiple_words_is_not_resolved_as_a_single_command_name() {
    assert!(asked(r#"CMD="rm -rf /outside/path"; $CMD"#, Dialect::Bash));
    assert!(asked(r#"CMD="rm -rf /outside/path" && $CMD"#, Dialect::Bash));
    assert!(!blocked(
        r#"CMD="rm"; $CMD -rf "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad/x""#,
        Dialect::Bash
    ));
}

/// `&`/`.` を使わずに動的なコードを実行できる式 (`[scriptblock]::Create($x).Invoke()` /
/// `$x.Invoke()`) は、先頭コマンド名ベースの判定 (`resolve_command_name` の「式だから止めない」
/// 例外) をすり抜けるが、`dotnet_deletes` の `OBJECT_CALL` がテキスト単位で `.Invoke` を
/// 捕まえるので判定できない削除として止まる (SEC-NEW-commands-mod-rs-L293)。他の `$x.Member`
/// のようなメソッド呼び出しを伴わない式は、これまでどおり止めない。
#[test]
fn a_dynamically_invoked_scriptblock_is_blocked_in_powershell() {
    assert!(stopped(
        r#"$x = "Remove-Item -Recurse C:\important"; [scriptblock]::Create($x).Invoke()"#,
        Dialect::PowerShell
    ));
    assert!(stopped(
        r#"$x = "Remove-Item -Recurse C:\important"; $x.Invoke()"#,
        Dialect::PowerShell
    ));
    assert!(!blocked("$env:PATH -split ';'", Dialect::PowerShell), "メソッド呼び出しの無い式は止めない");
    assert!(!blocked("$items.Count", Dialect::PowerShell), "プロパティの参照は呼び出しではない");
}

/// 同じコマンドの中で定義した別名は、呼び出しで展開して読む。展開しないと、削除コマンドの名前の
/// 一覧に無い名前で削除が実行される (SEC-NEW-deletions-rs-L18)。関数の本体は定義の位置で読む。
/// 定義を文字列で探して丸ごと止めると、`grep "function foo"` のような検索まで止まる。
#[test]
fn aliases_defined_in_the_command_are_expanded() {
    assert!(blocked("alias z='rm -rf'; z /outside", Dialect::Bash));
    assert!(stopped("alias z=\"$CMD\"; z /outside", Dialect::Bash));
    assert!(blocked("function z { rm -rf /outside; }", Dialect::Bash));
    assert!(blocked("z() { rm -rf /outside; }; z", Dialect::Bash));
    assert!(blocked(r"Set-Alias z Remove-Item; z -Recurse C:\important", Dialect::PowerShell));
    assert!(blocked(r"New-Alias -Name z -Value Remove-Item; z -Recurse C:\important", Dialect::PowerShell));
    assert!(blocked(r"sal -Scope Global z ri; z -Recurse C:\important", Dialect::PowerShell));
    assert!(!blocked("alias ll='ls -la'; ll src", Dialect::Bash));
    assert!(!blocked("alias ls='ls -la'; ls src", Dialect::Bash), "展開の中の同じ別名は展開しない");
    assert!(!blocked(r#"grep -rn "function handleX\|alias x=" src/"#, Dialect::Bash));
    assert!(!blocked(r#"Select-String -Pattern "Set-Alias" -Path x.ps1"#, Dialect::PowerShell));
}

/// 文字列からコードを作る (`[scriptblock]::Create`、`$ExecutionContext.InvokeCommand`) と、
/// `Invoke-Command $sb` のように呼び出し方を尽くせないので、作るところで止める。
#[test]
fn powershell_code_built_from_strings_is_unknown() {
    assert!(stopped(r#"$sb = [scriptblock]::Create($x); Invoke-Command $sb"#, Dialect::PowerShell));
    assert!(stopped(r#"$ExecutionContext.InvokeCommand.InvokeScript($x)"#, Dialect::PowerShell));
    assert!(stopped(r#"$sb = $ExecutionContext.InvokeCommand.NewScriptBlock($x); Start-Job $sb"#, Dialect::PowerShell));
    assert!(stopped(r#"$sb.InvokeReturnAsIs()"#, Dialect::PowerShell));
}

/// `dotnet_deletes` はテキスト全体を `cd` 追跡より前に一括処理するため、同じコマンド内の先行
/// する `cd` を無視して相対パスを初期 cwd のまま誤って解決し得る (SIM-NEW-mod-rs-L625)。先行
/// する `cd` があれば、.NET の相対パスは判定できないものとして確認する。絶対パスは `cd` の有無に
/// 関わらずそのまま解決する。
#[test]
fn a_dotnet_relative_delete_after_an_unresolved_cd_is_unknown() {
    assert!(stopped(r#"cd C:\important; [IO.Directory]::Delete("x", $true)"#, Dialect::PowerShell));
    assert!(stopped(r#"Set-Location C:\important; [IO.File]::Delete("x")"#, Dialect::PowerShell));
    assert!(
        !blocked(&format!(r#"cd C:\important; [IO.File]::Delete("{SCRATCH}\x.txt")"#), Dialect::PowerShell),
        "絶対パスは cd の有無に関わらずそのまま解決する"
    );
    assert!(
        blocked(r#"[IO.Directory]::Delete("x", $true)"#, Dialect::PowerShell),
        "cd が無い通常の相対パスはこれまでどおり cwd で解決して判定する"
    );
}
