//! 許可リスト (ADR-086 決定 5) の名前と、削除の機能を読む処理の突き合わせ (#587 の post-merge feedback)。
//!
//! 許可リストに載せた名前は確認なしで実行される。削除・上書きの機能を持つのに読む処理が無いと、
//! プロジェクトの外を消すコマンドが素通りする (#587 の `gawk`: 許可リストにはあったが `deletion()` の
//! 分岐が無かった)。そこで一覧の名前をすべて、次のどちらかに分類させる。
//!
//! - [`DELETES`] — 削除・上書きの機能がある。プロジェクトの外を指すサンプルで、実際に止まることを確かめる
//! - [`NO_DELETION`] — 削除・上書きの機能が無いと確かめた
//!
//! 一覧に名前を足したのに分類しないと、[`every_listed_name_is_classified`] が落ちる。
//!
//! 補助関数は他の test module と共有せず複製する (ADR-084)。

use super::*;
use std::collections::BTreeSet;

const SCRATCH: &str = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";

fn ctx() -> Context {
    Context {
        cwd: Some("C:/work/proj".to_string()),
        project: Some("C:/work/proj".to_string()),
        scratch: Some(SCRATCH.to_string()),
        memory: None,
        home: Some("C:/Users/u".to_string()),
    }
}

/// 確認なしでは通らないか (止めるか、確認する)。上書きは止めずに確認する (ADR-086 決定 1 の補足)。
fn stopped(command: &str, dialect: Dialect) -> bool {
    let found = inspect(command, dialect, &ctx(), &[]);
    !found.violations.is_empty() || found.asks()
}

/// 削除・上書きの機能がある名前と、プロジェクトの外を指すサンプル。
const DELETES: &[(&str, Dialect, &str)] = &[
    // NOTE: ファイル・テキスト系 (Bash で読む。`deletion()` は Cmd / PowerShell でも同じ名前を読む)
    ("rm", Dialect::Bash, "rm -rf C:/outside/dir"),
    ("rmdir", Dialect::Bash, "rmdir C:/outside/dir"),
    ("unlink", Dialect::Bash, "unlink C:/outside/file"),
    ("shred", Dialect::Bash, "shred -u C:/outside/file"),
    ("mv", Dialect::Bash, "mv C:/outside/file x"),
    ("cp", Dialect::Bash, "cp x C:/outside/file"),
    ("ln", Dialect::Bash, "ln -sf x C:/outside/file"),
    ("find", Dialect::Bash, "find C:/outside -delete"),
    ("sed", Dialect::Bash, "sed -i s/a/b/ C:/outside/file"),
    ("awk", Dialect::Bash, r#"awk 'BEGIN { system("rm -rf C:/outside") }'"#),
    ("gawk", Dialect::Bash, r#"gawk 'BEGIN { system("rm -rf C:/outside") }'"#),
    ("tee", Dialect::Bash, "echo x | tee C:/outside/file"),
    ("sort", Dialect::Bash, "sort -o C:/outside/file x"),
    ("uniq", Dialect::Bash, "uniq x C:/outside/file"),
    ("split", Dialect::Bash, "split x C:/outside/part"),
    ("xxd", Dialect::Bash, "xxd x C:/outside/file"),
    ("iconv", Dialect::Bash, "iconv -f a -t b -o C:/outside/file x"),
    ("tree", Dialect::Bash, "tree -o C:/outside/file"),
    // NOTE: 後ろのコマンドをそのまま実行する前置き (`mod.rs::is_prefix`)
    ("xargs", Dialect::Bash, "echo C:/outside | xargs rm -rf"),
    ("timeout", Dialect::Bash, "timeout 5 rm -rf C:/outside/dir"),
    ("nice", Dialect::Bash, "nice rm -rf C:/outside/dir"),
    ("nohup", Dialect::Bash, "nohup rm -rf C:/outside/dir"),
    ("env", Dialect::Bash, "env rm -rf C:/outside/dir"),
    // NOTE: PowerShell
    ("remove-item", Dialect::PowerShell, r"Remove-Item -Recurse C:\outside\dir"),
    ("ri", Dialect::PowerShell, r"ri -Recurse C:\outside\dir"),
    ("del", Dialect::PowerShell, r"del -Recurse C:\outside\dir"),
    ("erase", Dialect::PowerShell, r"erase -Recurse C:\outside\dir"),
    ("rd", Dialect::PowerShell, r"rd -Recurse C:\outside\dir"),
    ("move-item", Dialect::PowerShell, r"Move-Item C:\outside\file x"),
    ("mi", Dialect::PowerShell, r"mi C:\outside\file x"),
    ("move", Dialect::PowerShell, r"move C:\outside\file x"),
    ("rename-item", Dialect::PowerShell, r"Rename-Item C:\outside\file x"),
    ("rni", Dialect::PowerShell, r"rni C:\outside\file x"),
    ("ren", Dialect::PowerShell, r"ren C:\outside\file x"),
    ("copy-item", Dialect::PowerShell, r"Copy-Item x C:\outside\file"),
    ("copy", Dialect::PowerShell, r"copy x C:\outside\file"),
    ("cpi", Dialect::PowerShell, r"cpi x C:\outside\file"),
    ("set-content", Dialect::PowerShell, r"Set-Content C:\outside\file -Value x"),
    ("out-file", Dialect::PowerShell, r"'x' | Out-File C:\outside\file"),
    ("tee-object", Dialect::PowerShell, r"'x' | Tee-Object C:\outside\file"),
    ("tee", Dialect::PowerShell, r"'x' | tee C:\outside\file"),
    ("new-item", Dialect::PowerShell, r"New-Item -Force C:\outside\file"),
    ("ni", Dialect::PowerShell, r"ni -Force C:\outside\file"),
    ("invoke-expression", Dialect::PowerShell, r"Invoke-Expression 'Remove-Item -Recurse C:\outside\dir'"),
    ("iex", Dialect::PowerShell, r"iex 'Remove-Item -Recurse C:\outside\dir'"),
    ("start-process", Dialect::PowerShell, r"Start-Process cmd -ArgumentList '/c rd /s /q C:\outside\dir'"),
    ("saps", Dialect::PowerShell, r"saps cmd -ArgumentList '/c rd /s /q C:\outside\dir'"),
    ("start", Dialect::PowerShell, r"start cmd -ArgumentList '/c rd /s /q C:\outside\dir'"),
    ("start-job", Dialect::PowerShell, r"Start-Job { Remove-Item -Recurse C:\outside\dir }"),
    // NOTE: cmd
    ("rd", Dialect::Cmd, r"rd /s /q C:\outside\dir"),
    ("del", Dialect::Cmd, r"del /q C:\outside\file"),
    ("erase", Dialect::Cmd, r"erase /q C:\outside\file"),
    ("move", Dialect::Cmd, r"move C:\outside\file x"),
    ("copy", Dialect::Cmd, r"copy x C:\outside\file"),
    ("ren", Dialect::Cmd, r"ren C:\outside\file x"),
    ("rename", Dialect::Cmd, r"rename C:\outside\file x"),
    ("call", Dialect::Cmd, r"call rd /s /q C:\outside\dir"),
    ("for", Dialect::Cmd, r"for /d %i in (C:\outside\*) do rd /s /q %i"),
];

/// 削除・上書きの機能が無いと確かめた名前 (dialect ごと)。追記 (`add-content` / `>>` 相当) は既存の
/// 中身を消さないので削除として扱わない (`tee -a` と同じ)。
const NO_DELETION: &[(Dialect, &[&str])] = &[
    (
        Dialect::Bash,
        &[
            // NOTE: 読むだけ・表示するだけ
            "ls", "cat", "head", "tail", "wc", "cut", "tr", "paste", "join", "comm", "diff", "cmp", "grep",
            "egrep", "fgrep", "rg", "date", "basename", "dirname", "realpath", "readlink", "pwd", "stat",
            "du", "df", "file", "which", "whoami", "hostname", "uname", "printenv", "seq", "nl", "fold",
            "column", "od", "hexdump", "md5sum", "sha1sum", "sha256sum", "base64", "tac", "rev", "strings",
            "jq", "less", "more", "sleep", "true", "false", "test", "clear", "cygpath", "wslpath", "bc",
            "where", "dir", "tasklist",
            // NOTE: 作る・属性を変えるだけで、既存の中身は消さない
            "mkdir", "touch", "chmod",
        ],
    ),
    (
        Dialect::PowerShell,
        &[
            // NOTE: 制御構文
            "if", "else", "elseif", "foreach", "for", "while", "do", "until", "switch", "try", "catch",
            "finally", "throw", "param", "function", "filter", "begin", "process", "end", "break",
            "continue", "return", "exit", "trap",
            // NOTE: 読むだけ・表示するだけ (動詞の前方一致を含む)
            "get-*", "test-*", "select-*", "measure-*", "format-table", "format-list", "format-wide",
            "format-custom", "convertto-*", "convertfrom-*", "compare-object", "sort-object", "group-object",
            "where-object", "foreach-object", "write-host", "write-output", "write-error", "write-warning",
            "write-verbose", "write-information", "out-string", "out-null", "out-host", "join-path",
            "split-path", "resolve-path", "%", "?", "where", "select", "sort", "group", "measure", "ft",
            "fl", "gci", "gc", "gi", "gl", "gp", "gm", "dir", "type", "echo", "sls",
            // NOTE: 場所の移動・待機・セッション内の状態 (ファイルを消さない)
            "set-location", "push-location", "pop-location", "cd", "pushd", "popd", "sl", "chdir",
            "start-sleep", "set-alias", "new-alias", "set-variable", "new-object", "sal", "nal", "sv",
            "receive-job", "wait-job", "remove-job", "stop-job",
            // NOTE: 作るだけ・追記するだけ
            "md", "add-content",
        ],
    ),
    (
        Dialect::Cmd,
        &[
            "dir", "type", "echo", "set", "if", "cd", "chdir", "md", "findstr", "where", "exit", "cls", "ver",
            "pushd", "popd", "setlocal", "endlocal", "rem", "goto",
        ],
    ),
];

/// 許可リストの名前は、すべて [`DELETES`] か [`NO_DELETION`] のどちらか一方に分類されている。
#[test]
fn every_listed_name_is_classified() {
    for (names, dialect) in allowlist::command_name_lists() {
        let deletes: BTreeSet<&str> = DELETES.iter().filter(|(_, d, _)| *d == dialect).map(|(n, _, _)| *n).collect();
        let no_deletion: BTreeSet<&str> = NO_DELETION
            .iter()
            .filter(|(d, _)| *d == dialect)
            .flat_map(|(_, names)| names.iter().copied())
            .collect();
        let both: Vec<&str> = deletes.intersection(&no_deletion).copied().collect();
        assert!(both.is_empty(), "{dialect:?}: 両方に分類されている: {both:?}");
        let unclassified: Vec<&str> =
            names.iter().copied().filter(|n| !deletes.contains(n) && !no_deletion.contains(n)).collect();
        assert!(
            unclassified.is_empty(),
            "{dialect:?}: 許可リストにあるのに分類されていない: {unclassified:?}。削除・上書きの機能を確かめ、\
             読む処理があれば DELETES にサンプル付きで、無ければ NO_DELETION に足す"
        );
        let stale: Vec<&str> = deletes.union(&no_deletion).copied().filter(|n| !names.contains(n)).collect();
        assert!(stale.is_empty(), "{dialect:?}: 許可リストに無い名前が分類に残っている: {stale:?}");
    }
}

/// [`DELETES`] のサンプルは、どれも確認なしでは通らない。
#[test]
fn every_deleting_command_is_stopped_outside_the_project() {
    let passed: Vec<&str> =
        DELETES.iter().filter(|(_, dialect, sample)| !stopped(sample, *dialect)).map(|(_, _, sample)| *sample).collect();
    assert!(passed.is_empty(), "プロジェクトの外を指すのに確認なしで通った: {passed:#?}");
}

/// 突き合わせで見つけた穴 (`Rename-Item` / cmd の `ren` / `Out-File` / `tree -o` /
/// `-ErrorAction` の値取り違え / `Rename-Item` の別名 `rni`) を塞いだ分岐の境界。
#[test]
fn the_branches_added_by_the_sync_keep_their_boundaries() {
    assert!(stopped(r"Rename-Item -NewName x C:\outside\file", Dialect::PowerShell), "-NewName が先でも移動元を読む");
    assert!(!stopped(r"Rename-Item C:\work\proj\a.txt b.txt", Dialect::PowerShell), "プロジェクトの中の名前変更は通す");
    assert!(!stopped(r"ren C:\work\proj\a.txt b.txt", Dialect::Cmd), "cmd でもプロジェクトの中は通す");
    assert!(!stopped(r"'x' | Out-File -Append C:\outside\log", Dialect::PowerShell), "追記は中身を消さない");
    assert!(!stopped(r"'x' | Out-File -NoClobber C:\outside\log", Dialect::PowerShell), "既にあれば書かない");
    assert!(
        stopped(r"Out-File -InputObject x -Width 200 C:\outside\file", Dialect::PowerShell),
        "値を取るフラグの後ろの書き込み先を読む"
    );
    assert!(!stopped("tree -L 2", Dialect::Bash), "出力ファイルを取らない tree は確認しない");
    assert!(
        stopped(r"Out-File -ErrorAction Stop C:\outside\file", Dialect::PowerShell),
        "共通パラメータ -ErrorAction の値 (Stop) を書き込み先の位置引数と取り違えない (SEC-NEW-deletions-rs-L406)"
    );
    assert!(
        stopped(r"rni C:\outside\file x", Dialect::PowerShell),
        "Rename-Item の別名 rni も移動元を読む"
    );
}

/// PowerShell のパラメータの書き方 (値付きの共通パラメータ / `-Name:value` のコロン形式 / 明示的に
/// false のスイッチ) を、書き込み先・移動元・スイッチ・別名のどの読み取りでも同じに扱う (#593 の
/// CodeRabbit の指摘 3 件と、同じ系統の残り)。
#[test]
fn powershell_parameter_forms_are_read_the_same_everywhere() {
    let ps = Dialect::PowerShell;
    assert!(stopped(r"Rename-Item -ErrorAction Stop C:\outside\file x", ps), "移動元: 値付きの共通パラメータ");
    assert!(stopped(r"Move-Item -ea Stop C:\outside\file x", ps), "移動元: 共通パラメータの別名");
    assert!(stopped(r"Rename-Item -Path:C:\outside\file -NewName x", ps), "移動元: コロン形式");
    assert!(stopped(r"Move-Item C:\work\proj\x -Destination:C:\outside\dir", ps), "移動先: コロン形式");
    assert!(stopped(r"Out-File -FilePath:C:\outside\file -InputObject x", ps), "書き込み先: コロン形式");
    assert!(stopped(r"Set-Content -ErrorAction:Stop C:\outside\file -Value x", ps), "書き込み先: コロン形式の共通パラメータ");
    assert!(stopped(r"Out-File -InputObject x -Append:$false -FilePath C:\outside\file", ps), "明示的に false の -Append");
    assert!(stopped(r"Out-File -NoClobber:$false C:\outside\file", ps), "明示的に false の -NoClobber");
    assert!(!stopped(r"Out-File -Append:$true C:\outside\log", ps), "明示的に true の -Append は追記");
    assert!(stopped(r"New-Item -Force:$true C:\outside\file", ps), "明示的に true の -Force");
    assert!(!stopped(r"New-Item -Force:$false C:\outside\file", ps), "明示的に false の -Force は上書きしない");
    assert!(stopped(r"Set-Alias -Name:z -Value:Remove-Item; z -Recurse C:\outside\dir", ps), "別名: コロン形式");
}
