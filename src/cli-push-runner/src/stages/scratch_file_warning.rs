//! Scratch file warning stage — 順位 1 (PR #85 T1-4)
//!
//! `@` commit に scratch-pattern ファイル (default pattern: `__*`) が含まれていないか
//! 検査し、検出時は warning + block で push を停止する。jj は auto-snapshot で
//! working tree を即 commit に取り込むため、`.gitignore` 漏れがあると scratch
//! ファイルが PR に意図せず混入する (PR #85 で `__parse_transcripts.ps1` 実例)。
//!
//! ADR-039 (Experimental feature 標準パターン) 準拠の 3 点セット:
//! - **Config opt-in**: 試験運用のため default `enabled = false`、`[scratch_file_warning]`
//!   section で明示的に `enabled = true` にしないと検査は走らない。section 不在 /
//!   enabled 未指定の場合も skip (= 完全 no-op)。
//! - **Kill-switch**: `enabled = false` (TOML) または env override
//!   `SCRATCH_FILE_WARNING_OVERRIDE=1` で意図的バイパス可能。
//! - **Bounded lifetime**: 3-5 PR の dogfood で false positive / 検出効果を観測後、
//!   default-ON 昇格 or 却下を判定 (詳細は push-runner-config.toml の
//!   `[scratch_file_warning]` section コメント参照)。
//!
//! Stage 配置: `run_pipeline` の最早期 (quality_gate より前)。検出時は quality_gate
//! や takt review を無駄に走らせず即停止する。
//!
//! Config-driven pattern: `[scratch_file_warning]` section で `patterns` を拡張可能。
//! 順位 5 (AI 生成一時スクリプト pattern の pre-push 検出) は本 stage の patterns
//! 拡張で補完的に実装する設計 (Bundle 3 で `_tmp_*` 追加済)。
//!
//! # deny-list (pattern 列挙) の限界 — 順位 322 で実測
//!
//! **名前の列挙で AI が付ける名前を先回りするのは原理的に不可能。** post-merge-feedback
//! の takt run が repo root へ残した `analyze_transcript.py` は `__*` / `_tmp_*` の
//! どちらにも一致せず素通りした (near-miss)。
//!
//! さらに実測 (jj 0.42) で構造的な穴が判明した: `.gitignore` に `__*` があるため
//! `__foo.py` は **`jj file list -r @` に現れない**。本 stage は @ commit のファイルを
//! 検査するので、**`__*` パターンは事実上デッド**である。実際に効いていたのは
//! `_tmp_*` だけだった。
//!
//! ```text
//! __probe.py         -> jj file list -r @ に出ない (gitignore __*) -> 検出不可
//! _tmp_probe.txt     -> 出る -> 検出される
//! analyze_probe.py   -> 出る -> pattern に一致せず素通り (これが順位 322)
//! ```
//!
//! 対策として [`root_script_violations`] の**配置ベース判定**を第 2 層に足した。
//! 名前ではなく「repo root 直下 + スクリプト拡張子」で見るため、命名を先回りする
//! 必要が無い。pattern 層は subdirectory の `__scratch.rs` 等を拾うため残す。
//!
//! # 拡張子なし / 0 バイトの層 — PR #517 の `setTimeout(d` 混入
//!
//! `node -e "...setTimeout(d => ...)"` の `>` を cmd.exe がリダイレクトと解釈し、
//! repo root に **0 バイトで拡張子の無い** `setTimeout(d` が生まれて PR に混入した。
//! 第 2 層はスクリプト拡張子しか見ないので素通りし、止めたのは LLM の simplicity
//! review だけだった。#520 は発生元 (`node -e`) を塞いだが、シェルの誤リダイレクト
//! 全般は同じ形の残骸を作る。そこで形で見る層を 2 つ足した:
//!
//! - 第 3 層 [`root_extensionless_violations`]: root 直下の拡張子なしファイル
//!   (本 repo の root 直下は全ファイルが拡張子を持つ。`LICENSE` 等の定番は組み込み例外)
//! - 第 4 層 [`empty_file_violations`]: 場所を問わず 0 バイトのファイル
//!   (`.gitkeep` 等の定番は組み込み例外)
//!
//! どちらもリダイレクトで作られたファイルを対象にするため、Edit/Write を契機に走る
//! PostToolUse の custom_lint_rule では捕まらない (シェルが直接作ったファイルは hook を
//! 通らない)。push 前にファイルの実在を見る本 stage が受け持つ。
//!
//! ADR-007 (custom linter layer boundary) との関係:
//! - 本 stage = pre-push 時点で `@` commit 内の file path を `jj file list -r @` で
//!   列挙して basename match で検査 (= push 直前の最終防衛層)
//! - ADR-007 § custom_lint_rule = PostToolUse hook で AI が edit/write した瞬間に
//!   text 内容を regex で検査 (= 編集時の即時検出層)
//!
//! 両者は異なる timing / 検査対象で動作し、scratch file 検出は本 stage に集約。
//! scratch file は通常 .gitignore 対象で text content 検査の対象外のため、
//! file existence 検査である本 stage に責務を分離している。

use std::process::Command;

use crate::config::ScratchFileWarningConfig;
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;
const OVERRIDE_ENV_VAR: &str = "SCRATCH_FILE_WARNING_OVERRIDE";
const DEFAULT_PATTERN: &str = "__*";

/// `[scratch_file_warning]` config の有無に応じて検査を実行し、
/// push を続行してよいか (= violation なし or override active) を返す。
///
/// ADR-039 § 1 Config opt-in 準拠: default `enabled = false` (試験運用)。
/// section 不在 / `c.enabled = None` / `c.enabled = Some(false)` のいずれも skip。
/// 明示的に `c.enabled = Some(true)` のときのみ検査を実行。
///
/// fail-open: jj 不調 (timeout / 起動失敗) 時は warning ログのみで true を返し、
/// push 自体は止めない。
pub(crate) fn run_scratch_file_warning(config: Option<&ScratchFileWarningConfig>) -> bool {
    let enabled = config.and_then(|c| c.enabled).unwrap_or(false);
    if !enabled {
        return true;
    }
    let patterns = effective_patterns(config);
    let files = match list_files_in_at() {
        Ok(f) => f,
        Err(e) => {
            log_info(&format!(
                "scratch_file_warning: jj file list 失敗、検査を skip して push を続行します: {}",
                e
            ));
            return true;
        }
    };
    let empty_files = zero_byte_files(&files);
    let violations = all_violations(
        &files,
        &empty_files,
        &patterns,
        &effective_root_allowlist(config),
    );
    if violations.is_empty() {
        log_stage("scratch", "scratch ファイル検出なし");
        return true;
    }
    report_violations(&violations)
}

/// 検出結果をログに出し、override env が立っていれば続行 (`true`) を返す。
fn report_violations(violations: &[String]) -> bool {
    log_stage(
        "scratch",
        &format!(
            "scratch ファイル候補 ({} 件) が @ commit に含まれます:",
            violations.len()
        ),
    );
    for v in violations {
        log_info(&format!("  - {}", v));
    }
    let raw = std::env::var(OVERRIDE_ENV_VAR).ok();
    if parse_override_env(raw.as_deref()) {
        log_info(&format!(
            "  {}={} により続行します (意図的バイパス)",
            OVERRIDE_ENV_VAR,
            raw.as_deref().unwrap_or("")
        ));
        true
    } else {
        log_info(&format!(
            "  対処:\n  \
             (a) `.gitignore` に該当 pattern を追加 + `jj abandon @ && jj new` で再記述\n  \
             (b) ファイル自体を削除\n  \
             (c) root 直下のスクリプト / 拡張子なし / 0 バイトとして検出された正当なファイルなら\n  \
                 push-runner-config.toml の root_script_allowlist にファイル名を追加\n  \
                 (`__*` / `_tmp_*` 等の pattern 一致には効かない。pattern 側を見直すか (a)(b)(d) で対処)\n  \
             (d) 今回だけ意図的に commit するなら env {}=1 を設定して再実行",
            OVERRIDE_ENV_VAR
        ));
        false
    }
}

fn effective_patterns(config: Option<&ScratchFileWarningConfig>) -> Vec<String> {
    config
        .and_then(|c| c.patterns.as_ref())
        .map(|patterns| {
            patterns
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|patterns| !patterns.is_empty())
        .unwrap_or_else(|| vec![DEFAULT_PATTERN.to_string()])
}

fn list_files_in_at() -> Result<Vec<String>, String> {
    let output = run_jj_file_list_at()?;
    Ok(parse_file_list_output(&output))
}

fn parse_file_list_output(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|line| line.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}

/// 簡易 glob: `*` (任意長文字列、空マッチ含む) のみサポート。`?` 等は未対応。
/// パターンに `*` が含まれない場合は完全一致。
fn matches_glob(name: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    match_pattern_parts(name, &parts)
}

fn match_pattern_parts(name: &str, parts: &[&str]) -> bool {
    let Some(after_prefix) = consume_prefix(name, parts.first().copied().unwrap_or("")) else {
        return false;
    };
    let middle_parts = pattern_middle_slice(parts);
    let Some(after_middle) = consume_middle(after_prefix, middle_parts) else {
        return false;
    };
    if parts.len() > 1 {
        let suffix = parts.last().copied().unwrap_or("");
        check_suffix(after_middle, suffix)
    } else {
        true
    }
}

fn pattern_middle_slice<'a>(parts: &'a [&'a str]) -> &'a [&'a str] {
    if parts.len() > 2 {
        &parts[1..parts.len() - 1]
    } else {
        &[]
    }
}

fn consume_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        Some(name)
    } else {
        name.strip_prefix(prefix)
    }
}

fn consume_middle<'a>(name: &'a str, middle_parts: &[&str]) -> Option<&'a str> {
    let mut remaining = name;
    for part in middle_parts {
        if part.is_empty() {
            continue;
        }
        let idx = remaining.find(part)?;
        remaining = &remaining[idx + part.len()..];
    }
    Some(remaining)
}

fn check_suffix(name: &str, suffix: &str) -> bool {
    suffix.is_empty() || name.ends_with(suffix)
}

/// repo root 直下で「本 repo の構成では現れないはず」のスクリプト拡張子 (順位 322)。
///
/// 本 repo は Rust + TypeScript 構成で、root 直下にこれらの拡張子のファイルは置かない
/// (`scripts/` 配下の `.mjs` / `.ts` / `.sh` は対象外 = root 直下だけを見る)。
pub(crate) const ROOT_SCRIPT_EXTENSIONS: &[&str] = &["py", "sh", "ps1", "rb", "pl"];

/// [`ROOT_SCRIPT_EXTENSIONS`] に該当しても許可する root 直下のファイル名。
pub(crate) const DEFAULT_ROOT_SCRIPT_ALLOWLIST: &[&str] = &[];

/// **配置ベースの scratch 検出** (順位 322)。
///
/// ## deny-list (pattern 列挙) の限界
///
/// 既存の `patterns` は `__*` / `_tmp_*` のような**名前の列挙**で、
/// **AI が付ける名前を先回りするのは原理的に不可能**。実際 post-merge-feedback の
/// takt run が repo root に残した `analyze_transcript.py` はどの pattern にも
/// 一致せず素通りした (順位 322 の near-miss)。
///
/// さらに実測で判明した構造的な穴: `.gitignore` に `__*` があるため、
/// `__foo.py` は **`jj file list -r @` に現れない**。本 stage は @ commit の
/// ファイルを検査するので、**`__*` パターンは事実上デッド**である
/// (jj 0.42 実測: `__probe.py` は列挙されず、`_tmp_probe.txt` /
/// `analyze_probe.py` は列挙される)。つまり pattern 層で実際に効いていたのは
/// `_tmp_*` だけだった。
///
/// ## 配置ベースにした理由
///
/// 名前ではなく**置き場所と拡張子**で判定する。本 repo は Rust + TypeScript 構成で
/// root 直下にスクリプトを置かない (`scripts/` 配下に集約) ため、
/// 「root 直下の `.py` / `.sh` / `.ps1` 等」は高確度で一時ファイルである。
/// 名前を先回りする必要が無く、AI が新しい命名を使っても捕まる。
///
/// allow-list は config で拡張できる (正当な root スクリプトが増えた場合)。
pub(crate) fn root_script_violations(files: &[String], allowlist: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| is_root_level(file))
        .filter(|file| {
            let name = extract_basename(file);
            has_script_extension(name) && !allowlist.iter().any(|a| a == name)
        })
        .cloned()
        .collect()
}

/// repo root 直下か (サブディレクトリを含まないか)。
/// **両方の区切り文字を見る。** Windows の jj 0.42 は `jj file list -r @` の出力に
/// `\` を使う (実測: `sub\f.py` / `src\cli-push-runner\...`)。`/` だけを見ると
/// **サブディレクトリのファイルを root 直下と誤判定**して誤検知になる。既存の
/// [`extract_basename`] も両区切りを見ており、それと揃えている。
///
/// 逆に POSIX では `\` はファイル名に使える文字なので、`analyze\transcript.py` の
/// ような名前を「サブディレクトリ」と読む理論上の誤判定がある (CodeRabbit #432 指摘)。
/// **見送った** — 本 repo は Windows + WSL 運用でその名前は現れず、現れても結果は
/// scratch 警告が出るだけの安全側で、`\` 判定を外したときの Windows 側の誤検知の方が
/// 実害が大きい。OS で分岐させる案も、ADR-065 の CI matrix で両 OS を回している以上
/// 「振る舞いが OS で変わる」形になるため採らない。
fn is_root_level(path: &str) -> bool {
    !path.contains('/') && !path.contains('\\')
}

/// [`ROOT_SCRIPT_EXTENSIONS`] のいずれかの拡張子を持つか (大文字小文字非依存)。
fn has_script_extension(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            let ext = ext.to_ascii_lowercase();
            ROOT_SCRIPT_EXTENSIONS.contains(&ext.as_str())
        }
        None => false,
    }
}

/// 第 3 層で組み込みの例外にする、root 直下に置かれるのが普通な拡張子なしファイル名。
const WELL_KNOWN_EXTENSIONLESS: &[&str] = &["LICENSE", "Makefile", "Dockerfile", "Justfile"];

/// 第 4 層で組み込みの例外にする、中身が空であること自体に意味があるファイル名。
const WELL_KNOWN_EMPTY: &[&str] = &[".gitkeep", ".keep", "__init__.py", "py.typed"];

/// **root 直下の拡張子なしファイル** (第 3 層、PR #517 の `setTimeout(d`)。
///
/// 本 repo の root 直下は `Cargo.toml` / `.gitignore` 等すべて `.` を含む名前なので、
/// `.` を含まない名前は誤リダイレクトの残骸と見なす。`LICENSE` 等の定番と
/// `allowlist` に載せた名前は許可する。
pub(crate) fn root_extensionless_violations(files: &[String], allowlist: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| is_root_level(file))
        .filter(|file| {
            let name = extract_basename(file);
            !name.contains('.') && !is_allowed(name, WELL_KNOWN_EXTENSIONLESS, allowlist)
        })
        .cloned()
        .collect()
}

/// **0 バイトのファイル** (第 4 層)。`empty_files` は [`zero_byte_files`] が
/// 実ファイルのサイズから選んだパス。ここでは例外の除外だけを行う (I/O を持たない)。
pub(crate) fn empty_file_violations(empty_files: &[String], allowlist: &[String]) -> Vec<String> {
    empty_files
        .iter()
        .filter(|file| !is_allowed(extract_basename(file), WELL_KNOWN_EMPTY, allowlist))
        .cloned()
        .collect()
}

fn is_allowed(name: &str, well_known: &[&str], allowlist: &[String]) -> bool {
    well_known.contains(&name) || allowlist.iter().any(|a| a == name)
}

/// pattern 層 (deny-list)・配置ベース層・拡張子なし層・0 バイト層を適用し、
/// 重複を除いて返す (順位 322 / PR #517)。
///
/// 層を重ねるのは、どれか 1 つでは取りこぼすため:
/// - pattern 層だけ: AI が付ける新しい名前 (`analyze_transcript.py` 等) を先回りできない
/// - 配置ベース層だけ: サブディレクトリに置かれた `__scratch.rs` 等を拾えない
/// - 上の 2 つだけ: 誤リダイレクトが作る拡張子なしの空ファイル (`setTimeout(d`) を拾えない
pub(crate) fn all_violations(
    files: &[String],
    empty_files: &[String],
    patterns: &[String],
    root_allowlist: &[String],
) -> Vec<String> {
    let mut violations = find_violations(files, patterns);
    let layered = root_script_violations(files, root_allowlist)
        .into_iter()
        .chain(root_extensionless_violations(files, root_allowlist))
        .chain(empty_file_violations(empty_files, root_allowlist));
    for file in layered {
        if !violations.contains(&file) {
            violations.push(file);
        }
    }
    violations
}

/// `files` のうち実ファイルが 0 バイトのものを返す。
///
/// `jj file list -r @` のパスは cwd 相対で、`@` は working copy なのでディスク上の
/// サイズがそのまま `@` の内容に一致する。metadata が取れないファイル (列挙後に
/// 消えた等) は判定できないので skip する (本 stage の fail-open 方針に揃える)。
///
/// **シンボリックリンクはたどらない** (`symlink_metadata`)。リンクの中身はリンク先の
/// パスで空ではないため、空ファイルを指すリンクを 0 バイトと報告すると誤検知になる
/// (PR #525 CodeRabbit 指摘)。リンクは `is_file()` が偽になり対象外になる。
fn zero_byte_files(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| {
            std::fs::symlink_metadata(file.as_str())
                .map(|m| m.is_file() && m.len() == 0)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// config の allow-list (未設定なら [`DEFAULT_ROOT_SCRIPT_ALLOWLIST`])。
fn effective_root_allowlist(config: Option<&ScratchFileWarningConfig>) -> Vec<String> {
    let configured: Vec<String> = config
        .and_then(|c| c.root_script_allowlist.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if configured.is_empty() {
        DEFAULT_ROOT_SCRIPT_ALLOWLIST
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        configured
    }
}

fn find_violations(files: &[String], patterns: &[String]) -> Vec<String> {
    let mut violations = Vec::new();
    for file in files {
        let name = extract_basename(file);
        for pattern in patterns {
            if matches_glob(name, pattern) {
                violations.push(file.clone());
                break;
            }
        }
    }
    violations
}

fn parse_override_env(raw: Option<&str>) -> bool {
    let Some(value) = raw else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// `jj file list` の stdout を**打ち切らずに**読む。
///
/// 旧実装はログ表示用の `drain_pipe_capped(.., MAX_LINES = 40)` で読んでおり、
/// 先頭 40 ファイルしか検査していなかった (超過分は無言で捨てられる)。本 repo の
/// tracked ファイルは 551 件 (2026-09-27 実測) で、root 直下の `_tmp_guard.txt`
/// (44 行目) や `setTimeout(d` (196 行目) は検査範囲外になり push を素通りした。
/// 一覧は判定の入力なので全件が要る。
fn drain_file_list(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    lib_subprocess::drain_pipe_unlimited(pipe)
}

fn run_jj_file_list_at() -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(["file", "list", "-r", "@"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj file list 起動失敗: {}", e))?;

    let stdout_handle = drain_file_list(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = lib_subprocess::drain_pipe_capped(
        child.stderr.take().expect("stderr must be piped"),
        crate::runner::MAX_LINES,
    );

    let status =
        lib_subprocess::wait_with_timeout_basic("jj file list", &mut child, JJ_TIMEOUT_SECS)
            .map_err(|e| format!("jj file list wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj file list タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

// test module は別ファイルへ分離している (本体 800 行ガイドライン、順位 147)。
// 分割方式は lock.rs 等と同じ `#[path]` 方式に揃えた。
#[cfg(test)]
#[path = "scratch_file_warning/tests.rs"]
mod tests;
