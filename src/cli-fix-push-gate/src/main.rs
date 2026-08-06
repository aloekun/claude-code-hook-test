//! `cli-fix-push-gate` — Phase B 無人 fix push の直前ゲート (WP-17 PR 2、ADR-067)。
//!
//! GitHub Actions の Phase B job が、fix エージェントの変更を push する **直前** に呼ぶ。
//! ADR-052 自動実行可クラスの 4 軸を 1 回の呼び出しで AND 評価し、1 つでも欠ければ非ゼロで
//! 終了して push を止める (= Phase A 相当の分析コメントのみへ degrade)。
//!
//! | 軸 | 根拠 | 入力 |
//! |---|---|---|
//! | kill-switch | ADR-066 / ADR-052 原則 5 | `--config` + env `AUTONOMY_ENABLED` |
//! | target (隔離 namespace) | ADR-052 原則 2 target 軸 | `--branch` |
//! | 内容 (自動実行可クラス) | ADR-052 原則 2 内容軸 / ADR-035 | `--diff-summary-file` |
//! | scope (injection 防御) | ADR-054 | `--findings-file` + 同 diff |
//!
//! # なぜ単一 exe か
//!
//! `cli-autonomy-gate && cli-fix-push-gate` の連鎖にすると、workflow で `&&` を書き忘れた
//! 瞬間に kill-switch を通り越す。fail-closed 合成を呼び手のミスに依存させないため、
//! kill-switch も本 exe が `lib-autonomy-policy` 経由で内包する。
//!
//! # 使い方
//!
//! ```text
//! cli-fix-push-gate --branch <name> --config <path> \
//!   --diff-summary-file <path> --findings-file <path>
//! ```
//!
//! すべて必須。既定値を設けないのは、CI で `--config` に master ref の写しを渡し忘れた
//! 呼び手が PR ブランチの config を黙って読む事故を防ぐため (ADR-066 § 決定 3)。
//!
//! # exit コード
//!
//! - `0` = push してよい
//! - `1` = 拒否 (ポリシー判定。`empty-fix-diff` = 変更なしも含む)
//! - `2` = 引数不正 / 入力読み取り失敗
//!
//! **呼び手は非ゼロをすべて「push しない」として扱うこと。**
//!
//! # 出力
//!
//! allow は stdout の `[FIX_PUSH_ALLOW]`、deny は stderr の `[FIX_PUSH_DENY]`。どちらも
//! 4 軸すべての状態を含む 1 行を出す (無音 no-op 禁止、ADR-064 と同じ論理)。

mod checks;
mod inputs;

use std::path::PathBuf;

use checks::{DenyReason, GateFacts, Verdict};
use lib_autonomy_policy::{sources, GateInputs, Operation};

const MARKER_ALLOW: &str = "[FIX_PUSH_ALLOW]";
const MARKER_DENY: &str = "[FIX_PUSH_DENY]";

const EXIT_ALLOWED: i32 = 0;
const EXIT_DENIED: i32 = 1;
const EXIT_USAGE: i32 = 2;

const USAGE: &str = "usage: cli-fix-push-gate --branch <name> --config <path> \
--diff-summary-file <path> --findings-file <path>";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

/// コマンドライン設定。4 つとも必須。
struct Cli {
    branch: String,
    config_path: PathBuf,
    diff_summary_path: PathBuf,
    findings_path: PathBuf,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut values: [Option<String>; 4] = [None, None, None, None];
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let slot = match flag {
            "--branch" => 0,
            "--config" => 1,
            "--diff-summary-file" => 2,
            "--findings-file" => 3,
            other => return Err(format!("未知の引数です: {other:?}")),
        };
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} の値がありません"))?;
        values[slot] = Some(value.clone());
        index += 2;
    }
    let take = |slot: usize, name: &str| {
        values[slot]
            .clone()
            .ok_or_else(|| format!("{name} が必要です"))
    };
    Ok(Cli {
        branch: take(0, "--branch")?,
        config_path: PathBuf::from(take(1, "--config")?),
        diff_summary_path: PathBuf::from(take(2, "--diff-summary-file")?),
        findings_path: PathBuf::from(take(3, "--findings-file")?),
    })
}

fn run(args: Vec<String>) -> i32 {
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => return usage_error(&message),
    };
    let allowlist = match inputs::read_allowlist(&cli.findings_path) {
        Ok(a) => a,
        Err(message) => return usage_error(&message),
    };
    let diff_summary = match inputs::read_diff_summary(&cli.diff_summary_path) {
        Ok(d) => d,
        Err(message) => return usage_error(&message),
    };
    let external = sources::read_external_raw();
    let autonomy = lib_autonomy_policy::evaluate(GateInputs {
        repo_config_enabled: sources::read_repo_config(&cli.config_path).enabled,
        external_raw: external.as_deref(),
        open_draft_prs: None,
        max_open_draft_prs: None,
        operation: Operation::FixPush,
    });
    let facts = GateFacts {
        autonomy,
        branch: &cli.branch,
        fix_diff_summary: &diff_summary,
        allowlist: &allowlist,
    };
    report(&cli, &facts)
}

/// 引数不正・入力読み取り失敗。deny と同じ marker で出し、run log の grep 対象を 1 つに保つ。
fn usage_error(message: &str) -> i32 {
    eprintln!("{MARKER_DENY} 入力不正のため push しません: {message}");
    eprintln!("{USAGE}");
    EXIT_USAGE
}

fn report(cli: &Cli, facts: &GateFacts<'_>) -> i32 {
    let config_display = cli.config_path.display().to_string();
    let axes = checks::describe_axes(facts);
    match checks::evaluate(facts) {
        Verdict::Allowed { changed_files } => {
            println!(
                "{MARKER_ALLOW} branch={} files={} config={config_display} {axes}",
                cli.branch,
                changed_files.len()
            );
            EXIT_ALLOWED
        }
        Verdict::Denied(reason) => {
            eprintln!(
                "{MARKER_DENY} branch={} reason={} config={config_display} {axes}",
                cli.branch,
                reason.code()
            );
            eprintln!(
                "{MARKER_DENY} {}",
                reason.describe(sources::EXTERNAL_ENV, &config_display)
            );
            record_deny(&reason);
            EXIT_DENIED
        }
    }
}

/// deny を ADR-055 テレメトリへ記録する (fail-open)。理由コードのみで、パス・生値は載せない。
fn record_deny(reason: &DenyReason) {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "cli-fix-push-gate",
        kind: lib_telemetry::FiringKind::Hook,
        id: reason.code(),
        decision: lib_telemetry::Decision::Block,
        session_id: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_string()).collect()
    }

    fn full_args() -> Vec<String> {
        args(&[
            "--branch",
            "claude/x",
            "--config",
            "a.toml",
            "--diff-summary-file",
            "d.txt",
            "--findings-file",
            "f.json",
        ])
    }

    #[test]
    fn parses_all_four_required_flags() {
        let cli = parse_args(&full_args()).expect("parse");
        assert_eq!(cli.branch, "claude/x");
        assert_eq!(cli.config_path, PathBuf::from("a.toml"));
        assert_eq!(cli.diff_summary_path, PathBuf::from("d.txt"));
        assert_eq!(cli.findings_path, PathBuf::from("f.json"));
    }

    #[test]
    fn flag_order_does_not_matter() {
        let cli = parse_args(&args(&[
            "--findings-file", "f.json",
            "--diff-summary-file", "d.txt",
            "--config", "a.toml",
            "--branch", "claude/y",
        ]))
        .expect("parse");
        assert_eq!(cli.branch, "claude/y");
        assert_eq!(cli.findings_path, PathBuf::from("f.json"));
    }

    /// どの 1 つを落としても引数不正。省略で軸が無検査になる fail-open を作らない。
    #[test]
    fn omitting_any_flag_is_a_usage_error() {
        let all = full_args();
        for drop_flag in ["--branch", "--config", "--diff-summary-file", "--findings-file"] {
            let mut kept = Vec::new();
            let mut i = 0;
            while i < all.len() {
                if all[i] != drop_flag {
                    kept.push(all[i].clone());
                    kept.push(all[i + 1].clone());
                }
                i += 2;
            }
            assert!(parse_args(&kept).is_err(), "{drop_flag} 省略は引数不正であるべき");
        }
    }

    #[test]
    fn dangling_and_unknown_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--branch"])).is_err());
        assert!(parse_args(&args(&["--force", "1"])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    /// 入力ファイルが無い場合は exit 2。0 (許可) に倒れないことを固定する。
    #[test]
    fn missing_input_files_exit_non_zero() {
        assert_eq!(run(full_args()), EXIT_USAGE);
        assert_ne!(EXIT_USAGE, EXIT_ALLOWED);
        assert_ne!(EXIT_DENIED, EXIT_ALLOWED);
    }
}
