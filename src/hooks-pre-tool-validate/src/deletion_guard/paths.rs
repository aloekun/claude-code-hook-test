//! 削除の対象パスを正規化し、どこにあるか (一時ファイルの置き場 / プロジェクトの中 /
//! プロジェクトの外 / 判定できない) に分類する。I/O はしない — ファイルシステムを見ずに、
//! 文字列だけで決める (存在しないパスやシンボリックリンクは解決しない)。

/// 削除の対象がある場所。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// このセッションの scratchpad の中、またはプロジェクト直下の `__` で始まる名前の下。
    Scratch,
    /// プロジェクトの中 (一時ファイルの置き場を除く)。プロジェクトのルート自体も含む。
    Project,
    /// このプロジェクトの Claude の memory ディレクトリ (`<transcript のディレクトリ>/memory/`)。
    /// 古くなった記憶を消すのは正規の作業なので、単一ファイルの削除は通す。
    Memory,
    /// プロジェクトの外。
    Outside,
    /// 変数・コマンド置換などで、どこを指すか判定できない。
    Unknown,
    /// ファイルではない、揮発性の PowerShell のプロバイダ (`Env:` / `Variable:` など)。
    NotFile,
}

/// 分類に使う基準のパス。どれも [`normalize`] 済み。
#[derive(Clone, Debug, Default)]
pub(crate) struct Roots {
    pub(crate) project: Option<String>,
    pub(crate) scratch: Option<String>,
    pub(crate) memory: Option<String>,
    pub(crate) home: Option<String>,
}

/// パスを絶対パスに正規化する。区切りは `/`、Windows のドライブは小文字の `c:`、
/// Git Bash の `/c/...` は `c:/...` に直し、`.` と `..` を畳む。`~` は `home` で展開する。
/// 相対パスは `cwd` で解決する。解決できなければ `None`。
///
/// `**` (Bash の globstar、同じコマンドで `shopt -s globstar` を有効化すると使える) の直後の `..`
/// は畳まない — `**` は 0 階層にもマッチできるため、実行時に `proj/**/../outside` が
/// `proj/../outside` (プロジェクトの外) に展開され得るのに対し、ここで無条件に 1 階層分とみなして
/// `proj/**/..` → `proj` (プロジェクトの中) に畳むと、両者が食い違う。単一の `*`/`?`/`[...]` は
/// 常に実在する 1 階層にしかマッチしない (ゼロ幅になれない) ため、この畳み込みは `**` に特有の
/// 問題であり、それ以外のワイルドカードは従来通り畳んでよい (SEC-NEW-paths-rs-L52)。
pub(crate) fn normalize(raw: &str, cwd: Option<&str>, home: Option<&str>) -> Option<String> {
    let mut path = raw.replace('\\', "/");
    if let Some(rest) = path.strip_prefix('~') {
        if !(rest.is_empty() || rest.starts_with('/')) {
            return None;
        }
        path = format!("{}{rest}", home?);
    }
    let path = git_bash_drive(&path);
    let (root, rest) = match split_absolute(&path) {
        Some(parts) => parts,
        None if is_drive_relative(&path) => return None,
        None => {
            let joined = format!("{}/{path}", cwd?);
            return normalize(&joined, None, None);
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_some_and(|p| p == "**") {
                    return None;
                }
            }
            p => parts.push(p),
        }
    }
    let joined = format!("{root}/{}", parts.join("/"));
    Some(if root.is_empty() { joined } else { joined.to_lowercase() })
}

/// `C:foo` / `C:..\x` のようなドライブ相対パス。ドライブ `C:` のカレントディレクトリ基準で
/// 解決されるが、その場所は分からないので判定できない (CodeRabbit の指摘: cwd 基準で解決して
/// プロジェクトの中と誤っていた)。
fn is_drive_relative(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// `/c/Users` → `c:/Users`。`//server` のような UNC は触らない。
fn git_bash_drive(path: &str) -> String {
    let b = path.as_bytes();
    let drive_form = b.len() >= 2
        && b[0] == b'/'
        && b[1].is_ascii_alphabetic()
        && (b.len() == 2 || b[2] == b'/');
    if drive_form {
        format!("{}:{}", (b[1] as char).to_ascii_lowercase(), &path[2..])
    } else {
        path.to_string()
    }
}

/// 絶対パスなら (ルート, 残り) を返す。ルートは Windows ならドライブ (`c:`)、POSIX なら空。
fn split_absolute(path: &str) -> Option<(String, &str)> {
    let b = path.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'/') {
        return Some((path[..2].to_ascii_lowercase(), &path[2..]));
    }
    path.starts_with('/').then(|| (String::new(), path))
}

/// `path` が `root` そのものか、その下か。呼び出し元 (`classify`) は `roots.scratch` /
/// `roots.memory` / `roots.project` が `None` のケースを既に除外しているので、ここでは
/// `root` が実在するパスであることを前提にする。`root` が最初から空文字列、または末尾の
/// `/` を trim した結果 POSIX のファイルシステムルート (`"/"`) 相当で空文字列になった
/// 場合、無条件の一致として扱うとどのパスでもマッチしてしまい、ADR-086 の fail-closed
/// 原則に反する (SIM-NEW-paths-rs-L89 / SEC-NEW-paths-rs-L111 — trim 後に空になるケースを
/// 見落としていた)。Windows のドライブのルート (`c:/` → `c:`) も同じ。どれも「一致しない」として扱う。
pub(crate) fn within(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    let filesystem_root = root.is_empty() || (root.len() == 2 && root.ends_with(':'));
    if filesystem_root {
        return false;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

/// 既知の、消しても実害のない揮発性の PowerShell 組み込みプロバイダ名 (小文字)。`New-PSDrive` は
/// このガードの対象コマンドに入っておらず、`New-PSDrive -Name xx -PSProvider FileSystem
/// -Root <任意のパス>` で作ったドライブは「2 文字以上の英数字 + `:`」という形だけでは
/// 組み込みプロバイダと区別できない。allowlist 外の `名前:` を無条件に `Place::NotFile`
/// (常に許可) と断定せず `Place::Unknown` (判定できない→確認する) に倒す
/// (SIM-NEW-paths-rs-L96)。
///
/// レジストリ (`HKLM:` 等) / 証明書ストア (`Cert:`) / WS-Management 設定 (`WSMan:`) は、`Env:`/
/// `Variable:` のような揮発性のプロバイダとは違い実体が永続的に残る。かつてこの一覧に同列で
/// 入っており、`Remove-Item -Recurse -Force HKLM:\...` のような破壊的操作が `Place::NotFile`
/// (常に許可) として block/ask 無しで素通りしていた (SEC-NEW-paths-rs-L129)。これらは
/// `VOLATILE_PROVIDERS` から外し、allowlist 外のプロバイダと同じ `Place::Unknown`
/// (fail-closed、下の `strip_provider` の `else` 分岐) に倒す。
const VOLATILE_PROVIDERS: &[&str] = &["env", "variable", "function", "alias"];

/// PowerShell のプロバイダ付きのパスを読む。先頭の `FileSystem::` (`Microsoft.PowerShell.Core\FileSystem::`)
/// はファイルなので外す。途中の `FileSystem::` は POSIX ではふつうのディレクトリ名になりうるので外さない
/// (CodeRabbit の指摘: `/outside/FileSystem::/work/repo/__tmp` を `/work/repo/__tmp` と読んでいた)。既知の揮発性の
/// プロバイダ (`Env:` / `Variable:` など) はファイルではない。レジストリ・証明書ストアなどの
/// 永続的なプロバイダ (`HKLM:` / `Cert:` など) と、2 文字以上の名前 + `:` の形をしていても
/// allowlist 外のものは、ファイルシステム上の実パスを指す `New-PSDrive` の可能性を排除できない
/// ので、判定できないものとして確認する。
pub(crate) fn strip_provider(raw: &str) -> Result<&str, Place> {
    let lower = raw.to_ascii_lowercase();
    for prefix in ["microsoft.powershell.core\\filesystem::", "filesystem::"] {
        if lower.starts_with(prefix) {
            return Ok(&raw[prefix.len()..]);
        }
    }
    let provider_len = raw.find(':').unwrap_or(0);
    let looks_like_provider = provider_len >= 2
        && raw[..provider_len]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '\\');
    if !looks_like_provider {
        return Ok(raw);
    }
    if VOLATILE_PROVIDERS
        .iter()
        .any(|p| raw[..provider_len].eq_ignore_ascii_case(p))
    {
        Err(Place::NotFile)
    } else {
        Err(Place::Unknown)
    }
}

/// 正規化済みのパスを分類する。`glob` はワイルドカードを含む語だったか。
pub(crate) fn classify(path: &str, glob: bool, roots: &Roots) -> Place {
    if glob {
        return classify_glob(path, roots);
    }
    if roots.scratch.as_deref().is_some_and(|s| within(path, s)) {
        return Place::Scratch;
    }
    if roots.memory.as_deref().is_some_and(|m| within(path, m)) {
        return Place::Memory;
    }
    let Some(project) = roots.project.as_deref() else {
        return Place::Outside;
    };
    if !within(path, project) {
        return Place::Outside;
    }
    let relative = path[project.trim_end_matches('/').len()..].trim_start_matches('/');
    if relative.split('/').next().is_some_and(|first| first.starts_with("__")) {
        Place::Scratch
    } else {
        Place::Project
    }
}

/// Bash のブレース展開 (`{a,b}` / `{1..3}`)。展開後の候補は別々のパスになり、絶対パスも
/// 書ける (`<scratch>/{a,/c/Users/u}`) ので、固定部分では場所を決められない (CodeRabbit の指摘)。
/// 区切りの無い `{}` / `{x}` は展開されない。正規化の前のパスで判定する — 正規化で `..` を畳むと
/// `{a,/../x}` のブレースの片側が消え、展開と分からなくなる。
pub(crate) fn has_brace_expansion(path: &str) -> bool {
    path.match_indices('{').any(|(open, _)| {
        let body = &path[open + 1..];
        body.find('}')
            .is_some_and(|close| body[..close].contains(',') || body[..close].contains(".."))
    })
}

/// ワイルドカードを含むパスは、ワイルドカードより前の固定部分で分類する。
/// プロジェクト直下で `__` から始まるパターン (`__*`) は一時ファイルの置き場として扱う。
fn classify_glob(path: &str, roots: &Roots) -> Place {
    let parts: Vec<&str> = path.split('/').collect();
    let wild = parts
        .iter()
        .position(|p| p.contains(['*', '?', '[', '{']))
        .unwrap_or(parts.len());
    let base = parts[..wild].join("/");
    let base = if base.is_empty() || base.ends_with(':') { format!("{base}/") } else { base };
    let base_place = classify(&base, false, roots);
    let pattern_is_scratch = parts.get(wild).is_some_and(|p| p.starts_with("__"));
    let at_project_root = roots
        .project
        .as_deref()
        .is_some_and(|p| p.trim_end_matches('/') == base.trim_end_matches('/'));
    if base_place == Place::Project && at_project_root && pattern_is_scratch {
        Place::Scratch
    } else {
        base_place
    }
}
