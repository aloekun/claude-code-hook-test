//! Ledger completion stage — 台帳タスクの実装完了を push 前に検証する。
//!
//! commit description の `Ledger-Rank: N` trailer が「この変更は台帳の順位 N を実装した」と
//! 宣言する。本 stage はその宣言を受けて `cli-ledger-cleanup` を呼び、台帳が挙げる成果物
//! すべてが PR 範囲で変更されているかを判定させる。未完了なら push を止める。
//!
//! # なぜ trailer で宣言させるのか
//!
//! 夜間経路はブランチ名 (`claude/nightly-<rank>`) で順位が分かるが、対話 push には
//! そうした signal が無い。ブランチ名規約を人間に強いるより、commit description の
//! trailer にする方が「1 PR で複数順位」も自然に書ける (複数行)。
//!
//! **trailer が無ければ stage 全体を skip する。** 台帳と無関係な push が大半であり、
//! そこで検証を強いても止められる材料が無い。宣言した人だけが検証を受ける。
//!
//! # なぜ push 前なのか
//!
//! [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は不完全な実装が
//! CI green でマージされた。CI は「壊れていないか」を見るが「宣言した成果物が揃ったか」は
//! 見ない。push 前に止めれば、不完全な PR がレビュー対象になること自体を防げる。

use std::process::{Command, Stdio};

use crate::config::LedgerCompletionConfig;
use crate::log::{log_info, log_stage};

const DEFAULT_EXE: &str = ".claude/cli-ledger-cleanup.exe";
const DEFAULT_LEDGER: &str = "docs/claude-code-web-tasks.md";
const TRAILER_PREFIX: &str = "Ledger-Rank:";
const EXE_TIMEOUT_SECS: u64 = 60;

/// commit description から `Ledger-Rank:` trailer の順位を集める。
///
/// 行頭 (前後の空白は許容) の `Ledger-Rank: N` だけを見る。本文中に同じ語が出てきても
/// 拾わないのは、説明文で trailer に言及した文が宣言として解釈されるのを防ぐため。
///
/// 数値として読めない値は**捨てずにエラー**にする。`Ledger-Rank: 順位 203` のような
/// 書き間違いを黙って無視すると、宣言したつもりの人が検証を受けないまま push できる。
pub(crate) fn parse_rank_trailers(description: &str) -> Result<Vec<u32>, String> {
    let mut ranks = Vec::new();
    for line in description.lines() {
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix(TRAILER_PREFIX) else {
            continue;
        };
        let value = value.trim();
        let rank = value.parse::<u32>().map_err(|_| {
            format!("`{TRAILER_PREFIX} {value}` の値を順位 (整数) として読めません")
        })?;
        if !ranks.contains(&rank) {
            ranks.push(rank);
        }
    }
    Ok(ranks)
}

/// stage を実行し、push を続行してよいかを返す。
///
/// fail-closed: exe の起動失敗・timeout・未知の exit コードはいずれも `false` (push 停止)。
/// 「判定できなかった」を「判定は通った」に倒すと、この stage の存在意義が消える
/// ([ADR-043](../../../../docs/adr/adr-043-security-gates-fail-closed.md))。
pub(crate) fn run_ledger_completion(
    config: Option<&LedgerCompletionConfig>,
    description: &str,
    pr_range: &str,
) -> bool {
    let enabled = config.and_then(|c| c.enabled).unwrap_or(false);
    if !enabled {
        return true;
    }
    let ranks = match parse_rank_trailers(description) {
        Ok(ranks) => ranks,
        Err(message) => {
            log_stage("ledger", &format!("trailer を解釈できません: {message}"));
            log_info("  対処: `Ledger-Rank: 203` の形式で書くか、trailer 行を削除してください");
            return false;
        }
    };
    if ranks.is_empty() {
        log_stage(
            "ledger",
            &format!("`{TRAILER_PREFIX}` trailer なし、台帳完了検証を skip します"),
        );
        return true;
    }
    let changed_files_path = match write_changed_files(pr_range) {
        Ok(path) => path,
        Err(message) => {
            log_stage("ledger", &format!("変更一覧を作れません: {message}"));
            log_info("  判定できないため push を止めます (fail-closed)");
            return false;
        }
    };
    verify_declared_ranks(config, &ranks, &changed_files_path.to_string_lossy())
}

/// PR 範囲の全コミット description を連結して返す。
///
/// direct args で `jj log` を呼ぶ (shell 経由にすると revset のクォートが cmd.exe と sh で
/// 割れる — 他 stage と同じ理由)。
pub(crate) fn read_descriptions(pr_range: &str) -> Result<String, String> {
    let mut child = Command::new("jj")
        .args([
            "log",
            "-r",
            pr_range,
            "--no-graph",
            "-T",
            "description ++ \"\\n\"",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj log 起動失敗: {e}"))?;
    let stdout_handle =
        lib_subprocess::drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        lib_subprocess::drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));
    let status = lib_subprocess::wait_with_timeout_basic("jj log", &mut child, EXE_TIMEOUT_SECS)
        .map_err(|e| format!("jj log wait 失敗: {e}"))?;
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    match status {
        None => Err(format!("jj log タイムアウト ({EXE_TIMEOUT_SECS}s)")),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

/// PR 範囲の変更ファイル一覧を一時ファイルへ書き出し、そのパスを返す。
///
/// 取得と解釈は [`super::docs_only_routing::run_jj_diff_summary`] と
/// [`super::diff::parse_summary_paths`] を再利用する。`jj diff --summary` の rename / copy は
/// 共通 prefix を括り出した波括弧形式で出るなど癖が強く、過去 2 度のレビュー指摘を経て
/// fail-closed に固めた解釈がそこにある。ここで書き直すと同じ穴を開け直すことになる。
///
/// ファイル名は `process::id()` で一意化する。固定名だと ADR-045 が支える並行
/// `pnpm push` (別 jj workspace から同一マシン上で同時実行) が互いの一時ファイルを
/// 上書きし合い、write と子プロセスの read の間で台帳完了判定が壊れる
/// (このリポジトリの他の一時ファイル利用箇所と同じ規約)。
fn write_changed_files(pr_range: &str) -> Result<std::path::PathBuf, String> {
    let summary = super::docs_only_routing::run_jj_diff_summary(pr_range)?;
    let paths = super::diff::parse_summary_paths(&summary)?;
    let path = std::env::temp_dir().join(format!(
        "push-runner-ledger-changed-files-{}.txt",
        std::process::id()
    ));
    let body: String = paths
        .into_iter()
        .map(|p| format!("{p}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(&path, body)
        .map_err(|e| format!("変更一覧を書けません ({}): {e}", path.display()))?;
    Ok(path)
}

fn verify_declared_ranks(
    config: Option<&LedgerCompletionConfig>,
    ranks: &[u32],
    changed_files_path: &str,
) -> bool {
    let exe = config.and_then(|c| c.exe.as_deref()).unwrap_or(DEFAULT_EXE);
    let ledger = config
        .and_then(|c| c.ledger.as_deref())
        .unwrap_or(DEFAULT_LEDGER);
    let rank_list = ranks
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    log_stage(
        "ledger",
        &format!("台帳完了検証: 順位 [{rank_list}] (exe={exe})"),
    );
    match run_exe(exe, ledger, &rank_list, changed_files_path) {
        Ok(0) => {
            log_stage("ledger", "宣言された成果物はすべて変更されています");
            true
        }
        Ok(code) => {
            log_info(&format!(
                "  台帳完了検証が exit {code} で停止しました (直前の出力を参照)"
            ));
            false
        }
        Err(message) => {
            log_stage("ledger", &format!("台帳完了検証を実行できません: {message}"));
            log_info("  判定できないため push を止めます (fail-closed)");
            false
        }
    }
}

fn run_exe(
    exe: &str,
    ledger: &str,
    ranks: &str,
    changed_files_path: &str,
) -> Result<i32, String> {
    let mut child = Command::new(exe)
        .args([
            "--ledger",
            ledger,
            "--ranks",
            ranks,
            "--changed-files",
            changed_files_path,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("{exe} の起動に失敗: {e}"))?;
    let status = lib_subprocess::wait_with_timeout_basic(
        "cli-ledger-cleanup",
        &mut child,
        EXE_TIMEOUT_SECS,
    )
    .map_err(|e| format!("wait に失敗: {e}"))?;
    match status {
        None => Err(format!("timeout ({EXE_TIMEOUT_SECS}s)")),
        Some(s) => s
            .code()
            .ok_or_else(|| "シグナルで終了しました".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_a_single_trailer() {
        assert_eq!(
            parse_rank_trailers("feat: x\n\nLedger-Rank: 203\n").expect("parse"),
            vec![203]
        );
    }

    /// 1 PR で複数順位を実装した場合は複数行で宣言できる。
    #[test]
    fn collects_multiple_trailers_in_declaration_order() {
        let description = "feat: x\n\nLedger-Rank: 240\nLedger-Rank: 203\n";
        assert_eq!(parse_rank_trailers(description).expect("parse"), vec![240, 203]);
    }

    #[test]
    fn duplicate_declarations_are_folded() {
        let description = "Ledger-Rank: 203\nLedger-Rank: 203\n";
        assert_eq!(parse_rank_trailers(description).expect("parse"), vec![203]);
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert_eq!(
            parse_rank_trailers("  Ledger-Rank:   203  \n").expect("parse"),
            vec![203]
        );
    }

    /// trailer が無いのが通常の push。空 Vec を返し、呼び手が skip する。
    #[test]
    fn no_trailer_yields_an_empty_list() {
        assert!(parse_rank_trailers("feat: 台帳と無関係な変更\n")
            .expect("parse")
            .is_empty());
    }

    /// 本文中の言及を宣言と誤認しない。行頭のみを trailer として扱う。
    #[test]
    fn a_mid_sentence_mention_is_not_a_declaration() {
        let description = "この PR は Ledger-Rank: 203 の書き方を説明する文書変更です\n";
        assert!(parse_rank_trailers(description).expect("parse").is_empty());
    }

    /// 書き間違いを黙って無視すると、宣言したつもりの人が検証を受けずに push できる。
    #[test]
    fn a_malformed_value_is_an_error_not_a_silent_skip() {
        for description in [
            "Ledger-Rank: 順位 203\n",
            "Ledger-Rank: abc\n",
            "Ledger-Rank:\n",
            "Ledger-Rank: 203,240\n",
        ] {
            assert!(
                parse_rank_trailers(description).is_err(),
                "{description:?} がエラーにならない"
            );
        }
    }

    /// config 不在 / 無効は skip (既存の push は挙動不変)。
    #[test]
    fn a_disabled_stage_allows_the_push() {
        assert!(run_ledger_completion(None, "Ledger-Rank: 203\n", "changed.txt"));
        let disabled = LedgerCompletionConfig {
            enabled: Some(false),
            exe: None,
            ledger: None,
        };
        assert!(run_ledger_completion(
            Some(&disabled),
            "Ledger-Rank: 203\n",
            "changed.txt"
        ));
    }

    /// 有効でも trailer が無ければ通す。宣言した人だけが検証を受ける。
    #[test]
    fn an_enabled_stage_without_a_trailer_allows_the_push() {
        let enabled = LedgerCompletionConfig {
            enabled: Some(true),
            exe: None,
            ledger: None,
        };
        assert!(run_ledger_completion(
            Some(&enabled),
            "feat: 台帳と無関係\n",
            "changed.txt"
        ));
    }

    /// exe が見つからない場合は「判定できなかった」であり、push を通してはならない。
    #[test]
    fn an_unlaunchable_exe_blocks_the_push() {
        let enabled = LedgerCompletionConfig {
            enabled: Some(true),
            exe: Some("__cli-ledger-cleanup-absent__".to_string()),
            ledger: None,
        };
        assert!(!run_ledger_completion(
            Some(&enabled),
            "Ledger-Rank: 203\n",
            "changed.txt"
        ));
    }

    /// trailer が壊れていたら、宣言者の意図が読めないので止める。
    #[test]
    fn a_malformed_trailer_blocks_the_push() {
        let enabled = LedgerCompletionConfig {
            enabled: Some(true),
            exe: None,
            ledger: None,
        };
        assert!(!run_ledger_completion(
            Some(&enabled),
            "Ledger-Rank: あ\n",
            "changed.txt"
        ));
    }
}
