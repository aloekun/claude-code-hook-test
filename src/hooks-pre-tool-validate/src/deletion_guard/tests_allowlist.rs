//! 許可リスト (ADR-086 決定 5) と、許可リストに載せたコマンドの削除の機能の回帰テスト。
//! 補助関数は `tests.rs` と共有せず複製する (ADR-084)。

use super::*;

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

fn blocked(command: &str, dialect: Dialect) -> bool {
    !violations(command, dialect, &ctx()).is_empty()
}

fn asked(command: &str, dialect: Dialect) -> bool {
    !inspect(command, dialect, &ctx(), &[]).unlisted.is_empty()
}

/// 確認なしでは通らないか (止めるか、確認する)。判定できない削除と、コマンドの中に書いたコードの
/// 実行は確認する (ADR-086 決定 7)。
fn stopped(command: &str, dialect: Dialect) -> bool {
    let found = inspect(command, dialect, &ctx(), &[]);
    !found.violations.is_empty() || !found.undetermined.is_empty() || !found.inline_code.is_empty()
}

/// 許可リストに無いコマンドは、削除の機能を持つか分からないので確認する。前置きや中継の後ろも同じ。
#[test]
fn commands_not_in_the_list_are_asked() {
    assert!(asked("docker rm -f web", Dialect::Bash));
    assert!(asked("timeout 5 docker rm -f web", Dialect::Bash));
    assert!(asked("busybox ls", Dialect::Bash));
    assert!(asked(r#""$EXE" --check"#, Dialect::Bash));
    assert!(!blocked(r#""$EXE" --check"#, Dialect::Bash), "名前が分からないだけなら止めずに確認する");
    assert!(asked(r#"bash -c "docker rm web""#, Dialect::Bash), "入れ子のシェルの中も読む");
    assert!(asked("Clear-Content x.txt", Dialect::PowerShell));
}

/// 日常のコマンドは確認しない。
#[test]
fn everyday_commands_are_not_asked() {
    assert!(!asked("grep -n x f.rs | head -5 && jj log -r @ --no-graph", Dialect::Bash));
    assert!(!asked("cd src && cargo test -p x 2>&1 | tail -3", Dialect::Bash));
    assert!(!asked("for f in *.md; do wc -l \"$f\"; done", Dialect::Bash));
    assert!(!asked("timeout 590 pnpm build:all", Dialect::Bash));
    assert!(!asked("Get-ChildItem x | Select-Object Name | Sort-Object", Dialect::PowerShell));
    assert!(!asked(r#"powershell -Command "Get-Item x | Select-Object Length""#, Dialect::Bash));
    assert!(!asked("git status", Dialect::PowerShell));
}

/// PowerShell の式 (変数・文字列・型・メンバー参照で始まる文) はコマンドではない。
#[test]
fn powershell_expressions_are_not_commands() {
    assert!(!asked("$items.Count", Dialect::PowerShell));
    assert!(!asked(r#""text" | Write-Output"#, Dialect::PowerShell));
    assert!(!asked("[IO.Path]::GetFileName($p)", Dialect::PowerShell));
    assert!(!asked("(Get-Item x).Length", Dialect::PowerShell));
    assert!(!asked("1..3 | ForEach-Object { $_ }", Dialect::PowerShell));
}

/// 同じコマンドの中で定義した関数の呼び出しは確認しない (本体は定義の位置で読む)。
#[test]
fn functions_defined_in_the_command_are_listed() {
    assert!(!asked("f() { echo hi; }; f", Dialect::Bash));
    assert!(!asked("function g { ls; }; g", Dialect::Bash));
    assert!(!asked("function Show-X { Write-Output 1 }; Show-X", Dialect::PowerShell));
    assert!(blocked("f() { rm -rf /outside; }; f", Dialect::Bash));
}

/// コマンドではない語の並び (case のパターン、配列の値、行の継続、プロセス置換の後ろ) を、
/// コマンドと読まない。
#[test]
fn words_that_are_not_commands_are_not_asked() {
    let case = r#"case "$x" in a|b) echo ab;; "c d"*) echo c;; esac"#;
    assert!(!asked(case, Dialect::Bash));
    assert!(!asked(r#"ARGS=(); ARGS+=(--x "$y"); echo "${ARGS[@]}""#, Dialect::Bash));
    assert!(!asked("grep x f.txt | \\\n  head -3", Dialect::Bash));
    assert!(!asked("diff <(sort a.txt) b.txt", Dialect::Bash));
    assert!(blocked("diff <(rm -rf /outside) b.txt", Dialect::Bash), "プロセス置換の中は読む");
    assert!(asked("diff <(docker ps) b.txt", Dialect::Bash));
}

/// 設定 (`allowed_commands`) で足したコマンドは確認しない。`*` で終われば前方一致。
#[test]
fn configured_commands_are_listed() {
    let extra = vec!["docker".to_string(), "hooks-*".to_string()];
    let unlisted = |c: &str| inspect(c, Dialect::Bash, &ctx(), &extra).unlisted;
    assert!(unlisted("docker ps").is_empty());
    assert!(unlisted("./target/debug/hooks-stop-quality.exe < in.json").is_empty());
    assert_eq!(unlisted("podman ps"), vec!["podman".to_string()]);
}

/// 許可リストに載せたコマンドの削除の機能: 移動元 (`mv` / `Move-Item` / cmd の `move`)、
/// パッケージのキャッシュ、git の worktree、`trap` に渡すコード。
///
/// 移動元と移動先が同じ種類の場所ならファイルは残るので通す。違えば元の場所から消えるので、
/// 再帰の削除として扱う (SEC-NEW-deletions-rs-L275: `mv <project-dir> /outside` が `rm -rf` と
/// 同じ結果になるのに通っていた)。scratchpad へ移すのも、移した後に自由に消せるので消すのと同じ。
#[test]
fn deletions_by_listed_commands_are_read() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert!(blocked("mv /outside/x ./x", Dialect::Bash));
    assert!(blocked("mv ./project-dir /outside/escape", Dialect::Bash));
    assert!(blocked(&format!("mv ./project-dir {scratch}/x"), Dialect::Bash));
    assert!(blocked("mv -t /outside a.txt", Dialect::Bash));
    assert!(blocked(r#"mv ./project-dir "$DEST""#, Dialect::Bash), "移動先が分からなければ移すものとする");
    assert!(!blocked("mv a.txt b.txt", Dialect::Bash));
    assert!(!blocked("mv -t dest a.txt b.txt", Dialect::Bash));
    assert!(!blocked("mv src/old docs/new", Dialect::Bash));
    assert!(!blocked(&format!("mv {scratch}/a.txt {scratch}/b.txt"), Dialect::Bash));
    assert!(blocked(r"Move-Item C:\important\x .\x", Dialect::PowerShell));
    assert!(blocked(r"mv -Path C:\important\x -Destination .\x", Dialect::PowerShell));
    assert!(blocked(r"Move-Item .\src C:\temp\x", Dialect::PowerShell));
    assert!(!blocked(r"Move-Item a.txt b.txt", Dialect::PowerShell));
    assert!(!blocked(r"Move-Item -Destination .\b.txt -Path .\a.txt", Dialect::PowerShell));
    assert!(blocked(r"move C:\important\x .\x", Dialect::Cmd));
    assert!(blocked(r"move .\src C:\temp", Dialect::Cmd));
    assert!(!blocked(r"move a.txt b.txt", Dialect::Cmd));
    assert!(stopped("pnpm store prune", Dialect::Bash));
    assert!(stopped("npm cache clean --force", Dialect::Bash));
    assert!(!stopped("pnpm install", Dialect::Bash));
    assert!(blocked("git worktree remove ../other", Dialect::Bash));
    assert!(stopped("trap 'rm -rf /outside' EXIT", Dialect::Bash));
    assert!(!blocked("trap 'echo done' EXIT", Dialect::Bash));
}

/// `gh` は確認なし allowlist (`DEVELOPMENT_TOOLS`) に入っているが、削除系サブコマンド
/// (`<noun> delete`) は GitHub 上のリソースを消す経路を持つため、場所を判定できないものとして
/// 確認する (SIM-NEW-allowlist-rs-L28、ADR-086 決定 7)。中継コマンド越しでも同じ経路で確認する。
/// `gh pr view` / `gh pr list` のような読み取り系は通す。
/// `-t`/`--title`/`-l`/`--add-label` のような値取りオプションの値に `delete` を含むだけの
/// 自由記述 (PR タイトル・ラベル名) は確認しない (SIM-NEW-deletions-rs-L291)。
#[test]
fn gh_delete_subcommands_are_asked_but_reads_pass() {
    for command in [
        "gh repo delete owner/repo --yes",
        "gh release delete v1.0.0",
        "gh secret delete FOO",
        "gh variable delete FOO --org myorg",
        "gh -R owner/repo repo delete",
        "timeout 5 gh repo delete owner/repo --yes",
        "gh api -X DELETE repos/o/r/git/refs/heads/x",
        "gh api --method=delete repos/o/r/releases/1",
        "gh extension remove owner/gh-x",
        "gh release delete-asset v1.0.0 asset.zip",
        "gh repo deploy-key delete 123",
        "gh project item-delete 1 --id x",
        "gh pr merge 1 --squash --delete-branch",
        "gh issue comment 1 --delete-last --yes",
    ] {
        assert!(stopped(command, Dialect::Bash), "{command}");
    }
    for command in [
        r#"gh issue create -t "delete stale files" -b x"#,
        "gh pr edit 1 --remove-label wip",
        "gh pr create --title fix-delete-stale-branches",
        "gh pr edit 1 --add-label delete-candidate",
        "gh api repos/o/r/pulls/1 --jq .title",
        "gh api -X POST repos/o/r/issues/1/comments -f body=x",
        "gh pr view 1",
        "gh pr list",
        "gh repo view owner/repo",
    ] {
        assert!(!stopped(command, Dialect::Bash), "{command}");
    }
    assert!(!asked("gh repo delete owner/repo --yes", Dialect::Bash), "allowlist には残るので、確認の理由は削除の方");
}

/// `curl` は削除の経路 (`-X DELETE`、FTP の `-Q "DELE …"`、`-o` の上書き) が多く、確かめきれないので
/// 許可リストに載せない。使うときは確認する (SIM-NEW-allowlist-rs-L29)。
#[test]
fn curl_is_not_in_the_allowlist() {
    assert!(asked("curl https://api.example.com/items/1", Dialect::Bash));
    assert!(asked("curl -X DELETE https://api.example.com/items/1", Dialect::Bash));
}

/// コピーと `ln -f` の書き込み先は、既にあるファイルが置き換わる。あるかは文字列から分からないので、
/// プロジェクトの外や判定できない場所への書き込みは止めずに確認する。中への書き込みは何もしない。
#[test]
fn overwriting_outside_the_project_is_asked() {
    let overwrites = |c: &str, d: Dialect| inspect(c, d, &ctx(), &[]).overwrites;
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert_eq!(overwrites("cp new.txt /outside/old.txt", Dialect::Bash), vec!["/outside/old.txt".to_string()]);
    assert!(!blocked("cp new.txt /outside/old.txt", Dialect::Bash), "上書きは止めずに確認する");
    assert!(!overwrites("cp --remove-destination new.txt /outside/old.txt", Dialect::Bash).is_empty());
    assert!(!overwrites(r#"cp src/lib.rs "$TEMP/lib.rs.bak""#, Dialect::Bash).is_empty());
    assert!(!overwrites("ln -sf target /outside/link", Dialect::Bash).is_empty());
    assert!(overwrites("ln -s target /outside/link", Dialect::Bash).is_empty(), "-f が無ければ置き換わらない");
    assert!(overwrites("cp a.txt b.txt", Dialect::Bash).is_empty());
    assert!(overwrites(&format!("cp -r src {scratch}/backup"), Dialect::Bash).is_empty());
    assert!(overwrites(&format!("cp /outside/x.txt {scratch}/x.txt"), Dialect::Bash).is_empty(), "コピー元は読むだけ");
    assert!(!overwrites(r"Copy-Item .\a.txt C:\important\a.txt", Dialect::PowerShell).is_empty());
    assert!(overwrites(r"Copy-Item -Path .\a.txt -Destination .\b.txt", Dialect::PowerShell).is_empty());
    assert!(!overwrites(r"copy a.txt C:\important\a.txt", Dialect::Cmd).is_empty());
}

/// `mv`/`Move-Item`/cmd の `move` の移動先も、`cp` と同じく既にあるファイルを無条件に置き換える。
/// `overwrites()` の対象コマンド一覧に無く、移動先が一切チェックされていなかった
/// (SEC-NEW-mod-rs-L222)。移動元が scratchpad に分類されて移動元側の保護 ([`leaves_its_place`])
/// が効かない場合でも、移動先はここで独立にチェックされて確認が入る。
#[test]
fn moving_outside_the_project_overwrites_the_destination() {
    let overwrites = |c: &str, d: Dialect| inspect(c, d, &ctx(), &[]).overwrites;
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert_eq!(
        overwrites(&format!("mv {scratch}/payload /outside/.ssh/authorized_keys"), Dialect::Bash),
        vec!["/outside/.ssh/authorized_keys".to_string()]
    );
    assert!(
        !blocked(&format!("mv {scratch}/payload /outside/.ssh/authorized_keys"), Dialect::Bash),
        "上書きは即ブロックでなく確認する"
    );
    assert!(!overwrites(r"Move-Item .\a.txt C:\important\a.txt", Dialect::PowerShell).is_empty());
    assert!(!overwrites(r"move a.txt C:\important\a.txt", Dialect::Cmd).is_empty());
    assert!(overwrites("mv a.txt b.txt", Dialect::Bash).is_empty(), "プロジェクトの中どうしは何もしない");
    assert!(overwrites(&format!("mv a.txt {scratch}/b.txt"), Dialect::Bash).is_empty());
}

/// CodeRabbit の 3 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_third_review_findings() {
    assert!(asked("npm exec some-deleter -- x", Dialect::Bash), "npm exec の後ろはパッケージ");
    assert!(asked("npm x some-deleter", Dialect::Bash));
    assert!(!asked("npm exec biome check", Dialect::Bash));
    assert!(!asked("npm run build", Dialect::Bash));
    assert!(!blocked(r"find . -name '*.txt' -exec cp {} __out/ \;", Dialect::Bash), "find -exec は止めずに確認する");
    assert!(stopped(r"find . -name '*.tmp' -exec rm {} \;", Dialect::Bash));
    assert!(blocked(r"<# note #> Remove-Item -Recurse -Force C:\important", Dialect::PowerShell));
    assert!(!blocked("<# note #> Get-ChildItem", Dialect::PowerShell));
    assert!(blocked("bash <<< 'rm -rf ~'", Dialect::Bash), "here-string の本文はシェルのスクリプト");
    assert!(stopped(r#"bash <<< "$SCRIPT""#, Dialect::Bash), "中身が分からなければ確認する");
    assert!(stopped("sh < script.sh", Dialect::Bash), "< で渡したスクリプトは見えない");
    assert!(!blocked("bash <<< 'echo hi'", Dialect::Bash));
    assert!(!blocked("grep x < input.txt", Dialect::Bash));
    assert!(!blocked("bash scripts/x.sh < input.txt", Dialect::Bash));
}

/// `Start-Process -WorkingDirectory` は子プロセスの作業ディレクトリ。値を取るほかのパラメータは
/// 値ごと読み飛ばす (`-Verb RunAs` の `RunAs` を FilePath と読まない)。
#[test]
fn start_process_working_directory_and_valued_parameters() {
    let gitconfig = r"Start-Process pwsh -WorkingDirectory C:\Users\u -ArgumentList '-Command Remove-Item .gitconfig'";
    assert!(blocked(gitconfig, Dialect::PowerShell));
    let runas = r"Start-Process -Verb RunAs pwsh -ArgumentList '-Command Remove-Item -Recurse C:\x'";
    assert!(blocked(runas, Dialect::PowerShell));
    let hidden = r"Start-Process -WindowStyle Hidden -FilePath pwsh -ArgumentList '-Command Get-ChildItem'";
    assert!(!blocked(hidden, Dialect::PowerShell));
    assert!(!asked(hidden, Dialect::PowerShell));
}

/// 許可リストに載せたコマンドの上書き (`tee` / `sed -i` / `Tee-Object` / `New-Item -Force`) と、
/// グローバルのアンインストール。上書きはプロジェクトの外なら確認し、アンインストールは止める。
#[test]
fn other_overwrites_and_global_uninstalls() {
    let overwrites = |c: &str, d: Dialect| inspect(c, d, &ctx(), &[]).overwrites;
    assert!(!overwrites("echo x | tee /outside/f.txt", Dialect::Bash).is_empty());
    assert!(overwrites("echo x | tee -a /outside/f.txt", Dialect::Bash).is_empty(), "追記は中身を消さない");
    assert!(!blocked("echo x | tee -a /outside/f.txt", Dialect::Bash));
    assert!(overwrites("echo x | tee out.txt", Dialect::Bash).is_empty());
    assert!(!overwrites("sed -i '1,$d' /outside/.bashrc", Dialect::Bash).is_empty());
    assert!(!overwrites("sed -i.bak -e 's/a/b/' /outside/f", Dialect::Bash).is_empty());
    assert!(overwrites("sed -i 's/a/b/' src/x.rs", Dialect::Bash).is_empty());
    assert!(overwrites("sed -n '1,5p' /outside/f", Dialect::Bash).is_empty(), "-i が無ければ読むだけ");
    assert!(stopped("sed -i '1e rm -rf /outside' f", Dialect::Bash), "シェルの実行は確認する");
    assert!(!overwrites(r"Get-Date | Tee-Object -FilePath C:\important\log.txt", Dialect::PowerShell).is_empty());
    assert!(!overwrites(r"New-Item -Force C:\important\x.txt", Dialect::PowerShell).is_empty());
    assert!(overwrites(r"New-Item C:\important\x.txt", Dialect::PowerShell).is_empty(), "-Force が無ければ上書きしない");
    assert!(!overwrites(r"New-Item -Path C:\important -Name x.txt -Force", Dialect::PowerShell).is_empty());
    assert!(stopped("npm uninstall -g typescript", Dialect::Bash));
    assert!(stopped("pnpm remove --global biome", Dialect::Bash));
    assert!(!blocked("pnpm remove biome", Dialect::Bash), "プロジェクトの中の依存の削除は通す");
}

/// CodeRabbit の 4 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_fourth_review_findings() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert!(stopped(&format!("cd {scratch} && echo ok || rm -rf ."), Dialect::Bash), "|| の後ろは cd が失敗したときに動く");
    assert!(!stopped(&format!("cd {scratch} && rm -rf x || echo failed"), Dialect::Bash));
    assert!(stopped(&format!("if [ -d {scratch} ]; then cd {scratch}; fi; rm -rf ."), Dialect::Bash));
    assert!(blocked("if cd /outside; then rm important; fi", Dialect::Bash), "条件の中の cd も追跡する");
    assert!(stopped(&format!("for d in a b; do cd {scratch}; done; rm -rf ."), Dialect::Bash));
    assert!(!blocked(&format!("if true; then rm -rf {scratch}/x; fi"), Dialect::Bash));
    assert!(!blocked(&format!("cd {scratch}; if true; then echo; fi; rm -rf x"), Dialect::Bash), "ブロックの外の cd はそのまま");
    let overwrites = |c: &str| inspect(c, Dialect::PowerShell, &ctx(), &[]).overwrites;
    assert!(!overwrites(r"Set-Content C:\Users\u\.gitconfig ''").is_empty());
    assert!(overwrites(r"Set-Content .\out.txt 'x'").is_empty());
}

/// PowerShell の `if (…) { … }` の `if` は前置きではない (条件は `(…)` の中で読む)。
#[test]
fn powershell_if_is_not_a_prefix() {
    assert!(!asked(r"if (Test-Path x) { Get-ChildItem x } else { Write-Output none }", Dialect::PowerShell));
    assert!(blocked(r"if (Test-Path x) { Remove-Item -Recurse C:\important }", Dialect::PowerShell));
}

/// `node` / `python` に、コマンドの中に書いたコードを実行させるなら確認する (中身の削除は読めない)。
/// スクリプトファイルの実行は既知の限界のまま確認しない。
#[test]
fn inline_interpreter_code_is_asked() {
    let inline = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).inline_code;
    assert_eq!(inline("python -c 'import shutil; shutil.rmtree(\"/x\")'"), vec!["python".to_string()]);
    assert!(!inline("python -I - <<'PY'\nprint(1)\nPY").is_empty());
    assert!(!inline("python <<'PY'\nprint(1)\nPY").is_empty(), "スクリプト無しで heredoc から読む");
    assert!(!inline("echo 'print(1)' | python3").is_empty());
    assert!(!inline("node -e 'require(\"fs\").rmSync(\"/x\")'").is_empty());
    assert!(!inline("node --print 1+1").is_empty());
    assert!(inline("python scripts/x.py --apply").is_empty());
    assert!(inline("python -m pytest tests").is_empty());
    assert!(inline("node scripts/lint.mjs < input.txt").is_empty(), "スクリプトのファイルを渡せば標準入力はデータ");
    assert!(inline("cat x.json | node scripts/fmt.mjs").is_empty());
    assert!(!blocked("python -c 'print(1)'", Dialect::Bash), "止めずに確認する");
}

/// CodeRabbit の 5 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_fifth_review_findings() {
    assert!(blocked("rd /s/q src", Dialect::Cmd), "連結したスイッチ /s/q も再帰");
    assert!(blocked("del /s/q *.tmp", Dialect::Cmd));
    assert!(blocked("cmd /c rd /S/Q src", Dialect::Bash));
    assert!(!blocked("del /q old.txt", Dialect::Cmd), "/s が無ければ単一の削除");
    assert!(blocked(r#"cmd /c"rd /s /q C:\important""#, Dialect::Bash), "/c の直後に空白が無くても読む");
    assert!(stopped(r"cmd /crd /s /q C:\important", Dialect::Bash));
    assert!(blocked("echo $(echo ')'); rm -rf /outside", Dialect::Bash), "引用の中の ) で置換を閉じない");
    assert!(blocked(r#"echo $(echo "a)b"); rm -rf /outside"#, Dialect::Bash));
    assert!(blocked(r"echo $(echo \)); rm -rf /outside", Dialect::Bash));
    assert!(blocked(r#"echo "$(echo "$(rm -rf /outside)")""#, Dialect::Bash), "引用の中の入れ子の置換もたどる");
    assert!(!blocked("echo $(echo ')')", Dialect::Bash));
    assert!(blocked(r"Write-Output $(Write-Output ')'); Remove-Item -Recurse C:\important", Dialect::PowerShell));
}

/// CodeRabbit の 6 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_sixth_review_findings() {
    assert!(stopped("cargo --config build.target-dir=/outside clean", Dialect::Bash));
    assert!(stopped(r#"cargo clean --config 'build.target-dir="/outside"'"#, Dialect::Bash));
    assert!(blocked("CARGO_BUILD_TARGET_DIR=/outside cargo clean", Dialect::Bash));
    assert!(stopped("cargo -C ../other clean", Dialect::Bash));
    assert!(!blocked("cargo --color never build", Dialect::Bash), "値を読み飛ばした後の build は削除ではない");
    assert!(blocked(r"Start-Process pwsh '-c', 'Remove-Item -Recurse -Force C:\important'", Dialect::PowerShell));
    assert!(blocked("echo ${UNSET:-$(rm -rf ~/important)}", Dialect::Bash));
    assert!(blocked("echo ${X:-$(printf '}')}; rm -rf /outside", Dialect::Bash));
    assert!(!blocked(r#"echo "${HOME}/x" ${#ARGS[@]}"#, Dialect::Bash));
    assert!(blocked("Write-Output @\"\n$(Remove-Item -Recurse -Force C:/important)\n\"@", Dialect::PowerShell));
    assert!(
        !blocked("Write-Output @'\n$(Remove-Item -Recurse -Force C:/important)\n'@", Dialect::PowerShell),
        "単一引用の here-string は展開しない"
    );
    assert!(blocked(r"$h = @{ a = Remove-Item -Recurse -Force C:\important }", Dialect::PowerShell));
    assert!(!blocked(r"$h = @{ Name = 'x'; Count = 3 }", Dialect::PowerShell));
}

/// cmd の `%VAR%` に対になる `%` が無い場合、語の区切り (空白・`&`・`\|`) で走査を止める。
/// 止めずに入力の終端まで吸収すると、後続のコマンド (`del` など) が語として一度も現れず
/// 未検査のまま通過していた (SEC-NEW-lexer-mod-rs-L613)。
#[test]
fn cmd_unmatched_percent_stops_at_a_delimiter() {
    assert!(
        blocked(r"echo %FOO & del C:\important\x", Dialect::Cmd),
        "対合しない % の後ろの del も検査する"
    );
    assert!(
        blocked(r"echo %FOO | del C:\important\x", Dialect::Cmd),
        "パイプの後ろも同じく検査する"
    );
    assert!(!blocked(r"echo %FOO & echo ok", Dialect::Cmd), "del で無ければ止めない");
    assert!(stopped(r"del %TEMP%\important\x", Dialect::Cmd), "対になる % は変わらず Opaque");
    assert!(!blocked(r"echo %FOO%", Dialect::Cmd), "対になる % が最後まであれば従来通り");
}

/// プロジェクトのルートがファイルシステムやドライブのルートなら、どのパスもプロジェクトの中とは見なさない。
#[test]
fn a_filesystem_or_drive_root_project_contains_nothing() {
    let at = |project: &str| Context {
        cwd: Some(project.to_string()),
        project: Some(project.to_string()),
        scratch: None,
        memory: None,
        home: None,
    };
    assert!(!violations("rm /etc/passwd", Dialect::Bash, &at("/")).is_empty());
    assert!(!violations(r"Remove-Item C:\Windows\win.ini", Dialect::PowerShell, &at("C:/")).is_empty());
    assert!(!violations(r"Remove-Item C:\Windows\win.ini", Dialect::PowerShell, &at(r"C:\")).is_empty());
}

/// `find -exec ln -f {} <outside> \;` はプロジェクト外の任意ファイルを上書きできる。`-exec` の
/// 後ろのコマンドを読んでいた頃は `-f` を読み落としていた (SIM-NEW-deletions-rs-L341)。いまは
/// `-exec` 系を中身に関わらず確認する (ADR-086 決定 7)。
#[test]
fn bash_find_exec_ln_dash_f_overwrite_is_asked() {
    assert!(
        !inspect(r"find . -name '*.tmp' -exec ln -f {} /outside/payload \;", Dialect::Bash, &ctx(), &[])
            .inline_code
            .is_empty(),
        "find -exec ln -f は確認されるべき"
    );
}

/// `root` が最初から空ではなく、末尾の `/` を trim した結果 POSIX のファイルシステム
/// ルート (`"/"`) 相当で空文字列に *なる* ケース。trim 前のチェックだけでは防げず、
/// `path.starts_with(format!("{root}/"))` が `path.starts_with("/")` に退化してあらゆる
/// 絶対パスにマッチしてしまっていた (SEC-NEW-paths-rs-L111)。
#[test]
fn within_does_not_treat_a_trimmed_root_of_filesystem_root_as_matching_every_path() {
    assert!(!paths::within("/etc/passwd", "/"));
    assert!(!paths::within("/", "/"));
}

/// CodeRabbit の 7 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_seventh_review_findings() {
    let scratch_ps = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    let ps_if = format!(r"if (Test-Path {scratch_ps}\x) {{ Set-Location {scratch_ps}\x }}; Remove-Item -Recurse -Force .");
    assert!(stopped(&ps_if, Dialect::PowerShell), "PowerShell の if ブロックの中の Set-Location");
    assert!(stopped(&format!(r"function f {{ Set-Location {scratch_ps} }}; Remove-Item -Recurse ."), Dialect::PowerShell));
    assert!(stopped(&format!("f() {{ cd {scratch}/x; }}; rm -rf ."), Dialect::Bash), "呼ばれていない関数の本体の cd");
    assert!(stopped(&format!("function f {{ cd {scratch}/x; }}; rm -rf ."), Dialect::Bash));
    assert!(!blocked(&format!(r"Set-Location {scratch_ps}; if ($true) {{ Write-Output x }}; Remove-Item -Recurse x"), Dialect::PowerShell));
    assert!(stopped(&format!("if [ -d {scratch}/x ]; then cd {scratch}/x; else rm -rf .; fi"), Dialect::Bash), "else は cd の無いときに動く");
    assert!(stopped(&format!("case $x in a) cd {scratch}/x;; b) rm -rf .;; esac"), Dialect::Bash));
    assert!(!stopped(&format!("if true; then rm -rf {scratch}/x; else echo no; fi"), Dialect::Bash));
    assert!(stopped(&format!(r"Set-Location {scratch_ps}; Write-Output $(Set-Location C:\Users\u); Remove-Item -Recurse -Force x"), Dialect::PowerShell), "PowerShell の部分式は同じスコープ");
    assert!(blocked(r"$x += Remove-Item -Recurse -Force C:\important", Dialect::PowerShell));
    assert!(blocked(r"$h.k = Remove-Item -Recurse -Force C:\important", Dialect::PowerShell));
    assert!(blocked(r"[void]$r = Remove-Item -Recurse -Force C:\important", Dialect::PowerShell));
    assert!(!blocked(r"$x += 1", Dialect::PowerShell));
    assert!(blocked(r"pwsh -cwa 'Remove-Item -Recurse -Force C:\important'", Dialect::Bash));
    assert!(stopped(r"pwsh -CommandWithArgs 'Remove-Item -Recurse $args[0]' C:\x", Dialect::Bash));
    assert!(blocked(r#"cmd /c 'echo ^" & rd /s /q C:\important'"#, Dialect::Bash), "cmd の ^ はエスケープ");
    assert!(blocked(r"r^d /s /q C:\important", Dialect::Cmd));
}

/// simplicity レビュー (SIM-NEW-lexer-mod-rs-L146) の回帰テスト。case がサブシェルの中にネストして
/// いると、パターンを閉じる `)` の時点で `paren_depth` が 0 でないため、旧実装 (`paren_depth == 0`
/// だけで判定) はこれをサブシェルの閉じと誤認して深さを早まって 0 に戻してしまい、その後の本物の
/// サブシェルの閉じ括弧が今度は (`paren_depth == 0` に戻っているため) case のパターン境界と誤判定
/// されて、直前の `rm -rf /important` の segment 全体が `case_pattern` としてコマンド解析から
/// スキップされていた (`handled_before_the_command` の早期 return)。
#[test]
fn case_nested_inside_a_subshell_does_not_desync_the_paren_depth() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert!(
        blocked(&format!("(case $x in a) cd {scratch}/x;; esac; rm -rf /important)"), Dialect::Bash),
        "case をサブシェルにネストしても、サブシェルを閉じた後ろの rm は見落とされない"
    );
    for command in [
        "(case $x in (a) echo;; esac; rm -rf /important)",
        "case $x in a) case $y in b) echo;; esac;; esac; rm -rf /important",
        "( (case $x in a) echo;; esac); rm -rf /important)",
        "case $x in a) rm -rf /important;; esac",
    ] {
        assert!(blocked(command, Dialect::Bash), "{command}");
    }
    assert!(!asked("case $x in a) echo a;; b|c) echo bc;; esac", Dialect::Bash), "パターンをコマンドと読まない");
}

/// simplicity レビュー (SIM-NEW-lexer-mod-rs-L311) の回帰テスト。PowerShell の単独の `&` は呼び出し
/// 演算子だが、それが成り立つのは segment の先頭にあるときだけ。旧実装は位置を問わず区切りにしな
/// かったため、PowerShell 7.4+ のバックグラウンド演算子として語の後ろに現れた `&` の後ろのコマンド
/// 全体が前のコマンドの引数としてサイレントに吸収され、削除コマンドとして一度も解析されなかった。
/// `ls` は [`commands::deletions::passes_words_as_data`] に含まれるため、未知コマンドの引数から
/// 削除コマンド名を探す `relayed_deletion` の安全網も素通りする — segment を正しく割ることだけが
/// 唯一の防御線になる組み合わせ。
#[test]
fn powershell_trailing_background_ampersand_still_splits_the_next_command() {
    assert!(
        blocked(r"ls x & Remove-Item -Recurse C:\important", Dialect::PowerShell),
        "末尾の & の後ろの Remove-Item も別 segment として解析される"
    );
    for command in [
        r"Get-ChildItem x & & Remove-Item -Recurse C:\important",
        r"Get-ChildItem x | & Remove-Item -Recurse C:\important",
        r"Get-ChildItem x && Remove-Item -Recurse C:\important",
        r"Get-ChildItem x; & Remove-Item -Recurse C:\important",
    ] {
        assert!(blocked(command, Dialect::PowerShell), "{command}");
    }
    assert!(!stopped(r"& .\build.ps1 -Release", Dialect::PowerShell), "先頭の & は呼び出し演算子のまま");
}

/// 作業ディレクトリを変える前置き (`env -C` / `sudo -D` / `unshare -w`) の後ろの削除は、その場所で解決する。
/// 変えた作業ディレクトリは、その segment だけに効く。
#[test]
fn prefixes_that_change_the_working_directory() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    assert!(blocked("env --chdir=/outside rm important", Dialect::Bash));
    assert!(blocked(&format!("cd {scratch} && env -C /outside rm -rf x"), Dialect::Bash));
    assert!(blocked("sudo -D /outside rm important", Dialect::Bash));
    assert!(blocked("unshare --wd=/outside rm important", Dialect::Bash));
    assert!(stopped(r#"env -C "$DIR" rm important"#, Dialect::Bash), "行き先が分からなければ判定できない");
    assert!(!blocked(&format!("env -C {scratch} rm -rf x"), Dialect::Bash));
    assert!(!blocked(&format!("env -C {scratch} true; rm important"), Dialect::Bash), "次の segment には効かない");
    assert!(!asked("env -C src ls", Dialect::Bash), "-C の値をコマンドと読まない");
}

/// `=` を使わずに変数の値を変えるコマンドの後は、その変数の値が分からない。
#[test]
fn commands_that_rebind_variables_make_them_unknown() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    let scratch_ps = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";
    assert!(stopped(&format!(r#"d={scratch}/x; for d in /outside; do rm -rf "$d"; done"#), Dialect::Bash));
    assert!(stopped(&format!(r#"d={scratch}/x; unset d; rm -rf "$d/y""#), Dialect::Bash));
    assert!(stopped(&format!(r#"d={scratch}/x; read -r d < f; rm -rf "$d""#), Dialect::Bash));
    assert!(stopped(&format!(r#"d={scratch}/x; printf -v d '%s' /outside; rm -rf "$d""#), Dialect::Bash));
    assert!(stopped(&format!(r#"d={scratch}/x; mapfile -t d < f; rm -rf "$d""#), Dialect::Bash));
    assert!(!stopped(&format!(r#"d={scratch}/x; for f in a b; do echo "$f"; done; rm -rf "$d""#), Dialect::Bash), "別の変数は変わらない");
    let foreach = format!(r"$d = '{scratch_ps}\x'; foreach ($d in @('C:\outside')) {{ Remove-Item -Recurse $d }}");
    assert!(stopped(&foreach, Dialect::PowerShell));
    let removed = format!(r"$d = '{scratch_ps}\x'; Remove-Variable d; Remove-Item -Recurse $d");
    assert!(stopped(&removed, Dialect::PowerShell));
}

/// cmd / PowerShell から呼ぶ coreutils の `unlink` / `shred` / `tee` も読む (Git for Windows が PATH にある環境)。
#[test]
fn coreutils_called_from_cmd_and_powershell() {
    assert!(blocked(r"unlink C:\outside\x", Dialect::Cmd));
    assert!(blocked(r"shred -u C:\outside\x", Dialect::PowerShell));
    assert!(!inspect(r"echo x | tee C:\outside\x", Dialect::Cmd, &ctx(), &[]).overwrites.is_empty());
    assert!(blocked(r#"cmd /c "rm -rf C:\outside""#, Dialect::Bash));
}

/// scratchpad から移す `mv` も、移動先がプロジェクトの外か分からないなら上書きとして確認する。
#[test]
fn moving_out_of_the_scratchpad_to_an_unknown_place_is_asked() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    let overwrites = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    assert!(!overwrites(&format!(r#"mv {scratch}/payload "$DEST""#)).is_empty());
    assert!(!overwrites(&format!("mv {scratch}/payload ~/.ssh/authorized_keys")).is_empty());
    assert!(overwrites(&format!("mv {scratch}/a {scratch}/b")).is_empty());
}

/// 許可リストに載せたコマンドの出力ファイル (`sort -o` / `uniq` / `split` / `xxd` / `iconv -o` / sed の `w`)
/// は上書きとして確認する。プロジェクトの中への出力は何もしない。
#[test]
fn output_files_of_listed_commands_are_overwrites() {
    let overwrites = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    assert!(!overwrites("sort -o ~/.bashrc x.txt").is_empty());
    assert!(!overwrites("sort --output=/outside/f x.txt").is_empty());
    assert!(!overwrites("uniq -f 1 in.txt /outside/f").is_empty());
    assert!(!overwrites("split -l 10 in.txt /outside/part").is_empty());
    assert!(!overwrites("xxd -r dump.hex /outside/bin").is_empty());
    assert!(!overwrites("iconv -f utf-8 -t sjis -o /outside/f in.txt").is_empty());
    assert!(!overwrites("sed -n 'w /outside/f' in.txt").is_empty());
    assert!(!overwrites("sed 's/a/b/w /outside/f' in.txt").is_empty());
    assert!(overwrites("sort -o sorted.txt x.txt").is_empty(), "プロジェクトの中への出力");
    assert!(overwrites("sort -k 2 x.txt").is_empty());
    assert!(overwrites("uniq -c in.txt").is_empty());
    assert!(overwrites("sed -n '/word/p' in.txt").is_empty());
    assert!(!overwrites("mypy --junit-xml /outside/r.xml src").is_empty());
    assert!(!overwrites("mypy --html-report=/outside/html src").is_empty());
    assert!(!overwrites("mypy --cache-dir ~/.cache/x src").is_empty());
    assert!(overwrites("mypy --junit-xml r.xml --cache-dir .mypy_cache src").is_empty(), "プロジェクトの中への出力");
    assert!(overwrites("mypy --strict src").is_empty());
}

/// 上書きのリダイレクト (`> file`) の書き込み先がプロジェクトの外か判定できない場所なら、上書きとして確認する。
/// 追記 (`>>`)・ストリームの付け替え (`2>&1`)・捨てる先 (`/dev/null` / `nul` / `$null`) は確認しない。
#[test]
fn overwriting_redirections_outside_the_project_are_asked() {
    let scratch = "C:/Users/u/AppData/Local/Temp/claude/p/s/scratchpad";
    let bash = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    let ps = |c: &str| inspect(c, Dialect::PowerShell, &ctx(), &[]).overwrites;
    assert!(!bash("echo x > ~/.ssh/authorized_keys").is_empty());
    assert!(!bash("cargo test 2> /outside/err.log").is_empty());
    assert!(!bash("echo x >| /outside/f").is_empty());
    assert!(!bash(r#"echo x > "$TARGET""#).is_empty(), "書き込み先が分からなければ確認する");
    assert!(!ps(r"Get-Date > C:\important\x.txt").is_empty());
    assert!(bash("echo x >> ~/.bashrc").is_empty(), "追記は中身を消さない");
    assert!(bash("cargo test 2>&1 | tail -3").is_empty());
    assert!(bash("ls x 2>/dev/null").is_empty());
    assert!(bash("echo x > out.txt").is_empty(), "プロジェクトの中");
    assert!(bash(&format!("echo x > {scratch}/log.txt")).is_empty());
    assert!(bash(&format!(r#"S={scratch}; echo x > "$S/log.txt""#)).is_empty());
    assert!(ps("Get-Date > $null").is_empty());
    assert!(inspect("dir > nul", Dialect::Cmd, &ctx(), &[]).overwrites.is_empty());
    assert!(!blocked("echo x > ~/.bashrc", Dialect::Bash), "上書きは止めずに確認する");
}

/// CodeRabbit の 11 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_eleventh_review_findings() {
    let ps = |c: &str| inspect(c, Dialect::PowerShell, &ctx(), &[]).overwrites;
    let bash = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    assert!(!ps(r"New-Item -f C:\important\x.txt").is_empty(), "-f は -Force の省略形");
    assert!(!bash("sed -n '/x/w /outside/f' in.txt").is_empty(), "正規表現のアドレス付きの w");
    assert!(bash("sed -n '/word/p' in.txt").is_empty());
    assert!(!bash("echo x >& /outside/f").is_empty(), "Bash の >& file は &> file と同じ");
    assert!(bash("echo x >&2").is_empty(), ">&2 はストリームの付け替え");
    assert!(bash("cmd 2>&1 | tail").is_empty());
}

/// PowerShell の実行ファイルの `-Command` は、`--command` / `/Command` / `-Command:'…'` の形でも読む。
#[test]
fn powershell_command_flag_spellings() {
    assert!(blocked(r#"pwsh --command "Remove-Item -Recurse -Force C:\important""#, Dialect::Bash));
    assert!(blocked(r#"powershell /c "Remove-Item -Recurse -Force C:\important""#, Dialect::Bash));
    assert!(blocked(r"pwsh -Command:'Remove-Item -Recurse -Force C:\important'", Dialect::Bash));
    assert!(!blocked("pwsh /tmp/x.ps1", Dialect::Bash), "スクリプトのパスはフラグではない");
}

/// `-EncodedCommand` は `:` でつないだ形 / `/` で始める形でも、中身の分からないものとして確認する。
#[test]
fn encoded_command_spellings_are_unknown() {
    assert!(stopped("pwsh -EncodedCommand:AAAA", Dialect::Bash));
    assert!(stopped("powershell /enc AAAA", Dialect::Bash));
    assert!(stopped("powershell /ec:AAAA", Dialect::Bash));
}

/// CodeRabbit の 12 回目のレビュー (PR #587) の指摘の回帰テスト。
#[test]
fn coderabbit_twelfth_review_findings() {
    let bash = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    assert!(!bash("rustc x.rs -o /outside/bin").is_empty());
    assert!(!bash("tsc --outFile /outside/out.js").is_empty());
    assert!(!bash("git archive -o /outside/a.zip HEAD").is_empty());
    assert!(!bash("git format-patch -o /outside/patches HEAD~3").is_empty());
    assert!(!bash("git bundle create /outside/repo.bundle --all").is_empty());
    assert!(bash("rustc x.rs -o target/bin").is_empty());
    assert!(bash("git log --oneline").is_empty());
    assert!(!bash(r#"sed -n "/$pat/w /outside/f" in.txt"#).is_empty(), "変数を含む sed スクリプトの w");
    let scratch = r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad";
    let continued = format!("Remove-Item -Recurse -Force {scratch}/x `\n    -ErrorAction SilentlyContinue");
    assert!(!blocked(&continued, Dialect::PowerShell), "PowerShell の行継続で空の語を作らない");
    assert!(!blocked("Remove-Item -Recurse -Force C:/work/proj/__tmp `\r\n  -ErrorAction SilentlyContinue", Dialect::PowerShell));
    assert!(blocked("rm -rf /c/outside/FileSystem::/c/work/proj/__tmp", Dialect::Bash), "途中の FileSystem:: は外さない");
    assert!(!blocked(&format!(r"Remove-Item -Recurse FileSystem::{scratch}\x"), Dialect::PowerShell));
}

/// CodeRabbit の 13 回目のレビュー (PR #587) の指摘の回帰テスト: sed の `{ … }` の中のコマンドと、空白なしの `w`。
#[test]
fn sed_blocks_and_glued_write_commands() {
    let bash = |c: &str| inspect(c, Dialect::Bash, &ctx(), &[]).overwrites;
    assert!(stopped("sed '1{e rm -rf /outside\n}' f", Dialect::Bash));
    assert!(stopped("sed '1{s|x|rm -rf /outside|e\n}' f", Dialect::Bash));
    assert!(!bash("sed -n '1{w /outside/f\n}' f").is_empty());
    assert!(!bash("sed -n 'w/outside/f' in.txt").is_empty());
    assert!(!bash("sed -n '1w/outside/f' in.txt").is_empty());
    assert!(bash("sed -n '1{p\n}' f").is_empty());
}
