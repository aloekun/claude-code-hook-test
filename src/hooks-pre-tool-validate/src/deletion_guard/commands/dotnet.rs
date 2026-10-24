//! PowerShell の .NET 呼び出しによる削除 (`[IO.Directory]::Delete` / `.Delete()` / 文字列から作るコード)。
//! [`super::deletions`] から分けた (ADR-080、振る舞いは変えない)。

use super::super::lexer::{Dialect, Word};
use super::super::paths;
use super::State;
use regex::Regex;
use std::sync::LazyLock;

/// `$false` / `0` / 空と断定できる場合だけ false。他の値 (`$true` / `1` / それ以外の文字列) や
/// 判定できない値は recursive=true として扱う — `"$true"` への厳密一致だけを recursive とみなす
/// と、`Delete("src", 1)` のように数値の truthy 値を渡す書き方で非再帰と誤認し、プロジェクト内
/// 再帰削除を fail-open で通してしまう (SIM-NEW-commands-rs-L603)。
fn is_definitely_false(value: &str) -> bool {
    let v = value.trim();
    v.is_empty() || v.eq_ignore_ascii_case("$false") || v == "0"
}

/// PowerShell の .NET 呼び出し (`[IO.Directory]::Delete(path, $true)` / `[IO.File]::Delete(path)`)
/// と、オブジェクトの `.Delete(...)` / `.DeleteFolder(...)` / `.DeleteFile(...)` / `.Invoke(...)`。
/// 対象が文字列リテラルでなければ判定できない。
///
/// `.DeleteFolder`/`.DeleteFile` は `New-Object -ComObject Scripting.FileSystemObject` が持つ
/// 標準的な削除メソッドで、名前が `Delete` と一致しないため元の `OBJECT_CALL` (`\.delete\s*\(`)
/// には引っからず、`STATIC_CALL` (`[IO.Directory]`/`[IO.File]` 限定) にも先頭コマンド名ベースの
/// `deletion()`/`relayed_deletion()` にも引っからないまま無条件に通っていた
/// (SEC-NEW-commands-rs-L748)。メソッド名の判定を allowlist で拡張する。
///
/// `.Invoke(...)` も同じ allowlist に加える。`[scriptblock]::Create($x).Invoke()` /
/// `$x.Invoke()` は `&`/`.` を使わずに動的なコードを実行できるが、`mod.rs::resolve_command_name`
/// は PowerShell の変数始まりの式を「式だから止めない」例外にしているため、先頭コマンド名ベースの
/// 判定だけでは通過してしまう (SEC-NEW-commands-mod-rs-L293)。`OBJECT_CALL` はコマンド名の解決に
/// 依存しないテキスト単位の検出なので、ここで `.Invoke` を捕まえれば迂回経路を塞げる。
/// 呼び出し方は `.Invoke()` のほかにも `Invoke-Command $sb` / `Start-Job $sb` などがあって尽くせない
/// ので、文字列からコードを作るところ (`[scriptblock]::Create(...)`、`$ExecutionContext.InvokeCommand`
/// の `NewScriptBlock` / `InvokeScript`) も判定できない削除として確認する。
///
/// 変数チェーンの直後に配列/プロパティのインデクサ `[...]` が挟まる (`$arr[0].Delete()` /
/// `(Get-Item "$HOME\.ssh")[0].Delete($true)`) と、旧パターンは `)`/`$var` の直後にいきなり
/// `.Delete` を要求していたため一致せず、`resolve_command_name` 側も変数始まりの式を無条件に
/// 素通りさせるので、どの検知経路にも引っからず無確認・無ブロックで実行されていた
/// (SEC-NEW-dotnet-rs-L49)。チェーンに `[...]` インデクサも許すことで塞ぐ。
///
/// `using namespace System.IO` を併用すると、PowerShell は `[IO.Directory]`/`[IO.File]` を
/// 名前空間を省いた `[Directory]`/`[File]` で参照できる。旧 `STATIC_CALL` は `io\.` を必須にしていた
/// ため `using namespace System.IO; [Directory]::Delete("C:\Users\victim\.ssh", $true)` に一致せず、
/// `::` 呼び出しは `OBJECT_CALL` (`.Delete` のメンバ呼び出し限定) にも引っからないため block/ask
/// 無しでプロジェクト外の再帰削除が実行されていた (SEC-NEW-dotnet-rs-L51)。`io\.` を省略可能にし、
/// 名前空間を省いた `[Directory]`/`[File]` も捕まえる。
///
/// `text` 全体を `mod.rs::analyze` の segment 処理 (`cd` 追跡) より前に一括で読むため、同じ
/// コマンド内で .NET 呼び出しより手前にある `cd` を無視し、相対パスを呼び出し時点の
/// `state.cwd` (まだ `cd` が反映されていない) で誤って解決し得る (SIM-NEW-mod-rs-L625)。
/// 手前に `cd` 相当の語があるとき、相対パスは `state.cwd` を信用せず判定不能とする。
pub(crate) fn dotnet_deletes(text: &str, state: &mut State) {
    static STATIC_CALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)\[(?:system\.)?(?:io\.)?(directory|file)\]::delete\s*\(\s*(?:'([^']*)'|"([^"$]*)"|([^,)]*))\s*(?:,\s*([^)]*))?\)"#,
        )
        .expect("固定の正規表現")
    });
    static OBJECT_CALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:\)|\$[\w:]+)(?:\.\w+|\[[^\[\]]*\])*\.(?:delete|deletefolder|deletefile|invoke\w*)\s*\(")
            .expect("固定の正規表現")
    });
    static DYNAMIC_CODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\[(?:system\.management\.automation\.)?scriptblock\]::create\s*\(|\.(?:newscriptblock|invokescript)\s*\(")
            .expect("固定の正規表現")
    });
    for cap in STATIC_CALL.captures_iter(text) {
        let directory = cap[1].eq_ignore_ascii_case("directory");
        let recursive = directory && cap.get(5).is_some_and(|m| !is_definitely_false(m.as_str()));
        let whole = cap.get(0).expect("group 0 は必ず match する");
        match cap.get(2).or_else(|| cap.get(3)) {
            Some(literal) if path_depends_on_untracked_cd(literal.as_str(), text, whole.start()) => {
                state.push_unknown(&cap[0], recursive);
            }
            Some(literal) => state.push_target(&Word::lit(literal.as_str()), recursive, Dialect::PowerShell),
            None => state.push_unknown(&cap[0], recursive),
        }
    }
    for cap in OBJECT_CALL.captures_iter(text).chain(DYNAMIC_CODE.captures_iter(text)) {
        state.push_unknown(&cap[0], true);
    }
}

/// `literal` が相対パス (`cwd` が無いと場所を決められないパス) で、かつ `text[..before]`
/// (.NET 呼び出しより手前) に `cd` 相当の語があるか。両方成り立つとき、呼び出し時点の
/// `state.cwd` は `cd` 済みかどうかが分からず信用できない (SIM-NEW-mod-rs-L625)。絶対パスは
/// `cwd` に関わらず同じ場所を指すので対象にしない。
///
/// `[Environment]::CurrentDirectory = "..."` / `[IO.Directory]::SetCurrentDirectory(...)` も
/// `File`/`Directory` 静的メソッドの相対パス解決が参照する実行時カレントディレクトリを変える
/// 正規の経路だが、`cd`/`Set-Location` 系の固定キーワードだけでは拾えず、相対パスが初期 cwd
/// (プロジェクトルート) で誤って解決されて `Place::Project` に誤分類されていた
/// (SEC-NEW-dotnet-rs-L78)。`currentdirectory` を追加して拾う (先頭 `\b` を付けないのは、
/// `SetCurrentDirectory` のように直前に別の単語が連結していて単語境界が立たない呼び名も
/// 拾うため)。
fn path_depends_on_untracked_cd(literal: &str, text: &str, before: usize) -> bool {
    static DIRECTORY_CHANGE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(?:cd|pushd|chdir|sl|set-location|push-location)\b|currentdirectory\b").expect("固定の正規表現")
    });
    let relative = paths::normalize(literal, None, None).is_none();
    relative && DIRECTORY_CHANGE.is_match(&text[..before])
}

/// `dotnet.rs` 単体の回帰テスト (SEC-NEW-dotnet-rs-L49 / L78)。補助関数は `tests.rs` と
/// 共有せず複製する (ADR-084)。
#[cfg(test)]
mod tests {
    use super::super::super::{violations, Context};
    use super::super::super::lexer::Dialect;

    fn ctx() -> Context {
        Context {
            cwd: Some("C:/work/proj".to_string()),
            project: Some("C:/work/proj".to_string()),
            scratch: Some(r"C:\Users\u\AppData\Local\Temp\claude\p\s\scratchpad".to_string()),
            memory: None,
            home: Some("C:/Users/u".to_string()),
        }
    }

    fn blocked(command: &str, dialect: Dialect) -> bool {
        !violations(command, dialect, &ctx()).is_empty()
    }

    /// どこを消すか判定できない削除として確認するか (ADR-086 決定 7)。
    fn undetermined(command: &str, dialect: Dialect) -> bool {
        !crate::deletion_guard::inspect(command, dialect, &ctx(), &[]).undetermined.is_empty()
    }

    /// 変数チェーンの直後にインデクサ `[...]` が挟まると、旧 `OBJECT_CALL` (`)`/`$var` の直後に
    /// いきなり `.Delete` を要求するパターン) は一致せず、`resolve_command_name` も変数始まりの
    /// 式を「式だから止めない」として無条件に素通りさせるため、どの検知経路にも引っからず無確認
    /// で実行されていた (SEC-NEW-dotnet-rs-L49)。チェーンに `[...]` インデクサを許すことで塞いだ。
    #[test]
    fn an_indexer_between_the_object_chain_and_the_delete_call_is_still_caught() {
        for command in [
            r"$arr[0].Delete()",
            r#"(Get-Item "$HOME\.ssh")[0].Delete($true)"#,
            r"$Table.Rows[0].Delete()",
        ] {
            assert!(undetermined(command, Dialect::PowerShell), "確認するべき: {command}");
        }
    }

    /// `[Environment]::CurrentDirectory = "..."` / `[IO.Directory]::SetCurrentDirectory(...)` も
    /// `File`/`Directory` 静的メソッドの相対パス解決が参照する実行時カレントディレクトリを変える
    /// 正規の経路だが、旧 `DIRECTORY_CHANGE` は `cd`/`Set-Location` 系の固定キーワードしか見ておらず、
    /// 相対パスが初期 cwd (プロジェクトルート) で誤って解決され `Place::Project` に誤分類されていた
    /// (SEC-NEW-dotnet-rs-L78)。`currentdirectory` を追加して塞いだ。
    #[test]
    fn a_currentdirectory_assignment_before_a_dotnet_relative_delete_is_unknown() {
        assert!(undetermined(
            r#"[Environment]::CurrentDirectory = "C:\Users\victim"; [IO.File]::Delete(".ssh\id_rsa")"#,
            Dialect::PowerShell
        ));
        assert!(undetermined(
            r#"[IO.Directory]::SetCurrentDirectory("C:\Users\victim"); [IO.File]::Delete(".ssh\id_rsa")"#,
            Dialect::PowerShell
        ));
    }

    /// `using namespace System.IO` を併用すると、`[IO.Directory]`/`[IO.File]` を名前空間を省いた
    /// `[Directory]`/`[File]` で参照できる。旧 `STATIC_CALL` は `io\.` を必須にしていたため、
    /// この省略形には一致せず、`::` 呼び出しは `OBJECT_CALL` にも引っからないまま無確認で
    /// 実行されていた (SEC-NEW-dotnet-rs-L51)。`io\.` を省略可能にして塞いだ。
    #[test]
    fn a_using_namespace_bare_directory_delete_is_still_caught() {
        for command in [
            r#"using namespace System.IO; [Directory]::Delete("C:\Users\victim\.ssh", $true)"#,
            r#"using namespace System.IO; [File]::Delete("C:\Users\victim\.ssh\id_rsa")"#,
        ] {
            assert!(blocked(command, Dialect::PowerShell), "止めるべき: {command}");
        }
    }
}
