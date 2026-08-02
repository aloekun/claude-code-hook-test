//! `cli-autonomy-gate` — 自律実行の全体 kill-switch ゲート (WP-17 PR 1、ADR-066)。
//!
//! 自律 actor (GitHub Actions の無人 fix push、夜間 todo 消化ループ、cloud routine) が
//! ADR-052 の自動実行可クラスの操作を行う **直前** に呼び、許可されているかを exit コードで
//! 受け取るための決定論ゲート。run 冒頭で 1 回だけ判定するのではなく操作境界ごとに呼ぶ
//! (ADR-052 停止手順「フラグを OFF にすると次の自律実行判定から無効化される」を満たすため)。
//!
//! # 使い方
//!
//! ```text
//! cli-autonomy-gate --operation <fix-push|draft-pr> --config <path>
//! ```
//!
//! # exit コード
//!
//! - `0` = 許可
//! - `1` = 拒否 (ポリシー判定)
//! - `2` = 引数不正
//!
//! **呼び手は非ゼロをすべて拒否として扱うこと。** `1` だけを拒否とみなして `2` を通すと、
//! 引数を間違えた瞬間に fail-open する。
//!
//! # 出力
//!
//! 判定は必ず loud に出す (無音 no-op 禁止)。allow は stdout の `[AUTONOMY_ALLOW]`、deny は
//! stderr の `[AUTONOMY_OFF]` で、どちらも全ソースの状態と読み取り先 config パスを含む。
//! 「何もしなかった run」の原因が run log だけで切り分けられることを要件とする
//! (ADR-064 の silent-success 排除と同じ論理)。ADR-060 の `CLOUD_HARNESS` は無効時無音を
//! 選んだが、あれはローカル常時発火のノイズ対策という個別事情で本 exe には適用しない。

mod decision;
mod sources;

use std::path::PathBuf;

use decision::{Decision, DenyReason, GateInputs, Operation};

/// 許可時の grep マーカー (stdout)。
const MARKER_ALLOW: &str = "[AUTONOMY_ALLOW]";
/// 拒否時の grep マーカー (stderr)。run log 監査の主キー。
const MARKER_DENY: &str = "[AUTONOMY_OFF]";

const EXIT_ALLOWED: i32 = 0;
const EXIT_DENIED: i32 = 1;
const EXIT_USAGE: i32 = 2;

const USAGE: &str = "usage: cli-autonomy-gate --operation <fix-push|draft-pr> --config <path>";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

/// コマンドライン設定。既定値は設けない — 両方とも明示必須。
///
/// `--config` を省略可能にして cwd から推測すると、CI で master ref の写しを渡し忘れた
/// 呼び手が PR ブランチの config を黙って読む (上記 [`sources`] の信頼境界)。省略を
/// 引数不正として弾くことで、呼び手にパスの出所を必ず意識させる。
struct Cli {
    operation: Operation,
    config_path: PathBuf,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut operation = None;
    let mut config_path = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        match flag {
            "--operation" => {
                let raw = value.ok_or_else(|| "--operation の値がありません".to_string())?;
                operation = Some(
                    Operation::parse(raw)
                        .ok_or_else(|| format!("未知の operation です: {raw:?}"))?,
                );
                index += 2;
            }
            "--config" => {
                let raw = value.ok_or_else(|| "--config の値がありません".to_string())?;
                config_path = Some(PathBuf::from(raw));
                index += 2;
            }
            other => return Err(format!("未知の引数です: {other:?}")),
        }
    }
    Ok(Cli {
        operation: operation.ok_or_else(|| "--operation が必要です".to_string())?,
        config_path: config_path.ok_or_else(|| "--config が必要です".to_string())?,
    })
}

fn run(args: Vec<String>) -> i32 {
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("{MARKER_DENY} 引数不正のため停止します: {message}");
            eprintln!("{USAGE}");
            return EXIT_USAGE;
        }
    };
    let external = sources::read_external_raw();
    let inputs = GateInputs {
        repo_config_enabled: sources::read_repo_config_enabled(&cli.config_path),
        external_raw: external.as_deref(),
        operation: cli.operation,
    };
    report(&cli, inputs)
}

/// 判定と loud 出力。allow / deny のどちらでも全ソースの状態を 1 行目に出す。
fn report(cli: &Cli, inputs: GateInputs<'_>) -> i32 {
    let config_display = cli.config_path.display();
    let state = decision::describe_sources(inputs, sources::EXTERNAL_ENV);
    let operation = cli.operation.as_str();
    match decision::evaluate(inputs) {
        Decision::Allowed => {
            println!("{MARKER_ALLOW} operation={operation} config={config_display} {state}");
            EXIT_ALLOWED
        }
        Decision::Denied(reason) => {
            eprintln!(
                "{MARKER_DENY} operation={operation} reason={} config={config_display} {state}",
                reason.code()
            );
            eprintln!(
                "{MARKER_DENY} {}",
                reason.describe(sources::EXTERNAL_ENV, &config_display.to_string())
            );
            record_deny(&reason);
            EXIT_DENIED
        }
    }
}

/// deny を ADR-055 テレメトリへ記録する (fail-open)。
///
/// allow は「発火」ではないため記録しない (ADR-055 の firing は block/warn 事象を数える器)。
/// 記録するのは理由コードのみで、config パスや env 生値は載せない (ADR-055 プライバシー原則)。
fn record_deny(reason: &DenyReason) {
    lib_telemetry::record(&lib_telemetry::Firing {
        hook: "cli-autonomy-gate",
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

    #[test]
    fn parses_both_required_flags() {
        let cli = parse_args(&args(&["--operation", "fix-push", "--config", "a.toml"]))
            .expect("parse");
        assert_eq!(cli.operation, Operation::FixPush);
        assert_eq!(cli.config_path, PathBuf::from("a.toml"));
    }

    #[test]
    fn order_of_flags_does_not_matter() {
        let cli = parse_args(&args(&["--config", "b.toml", "--operation", "draft-pr"]))
            .expect("parse");
        assert_eq!(cli.operation, Operation::DraftPr);
        assert_eq!(cli.config_path, PathBuf::from("b.toml"));
    }

    #[test]
    fn missing_required_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--operation", "fix-push"])).is_err());
        assert!(parse_args(&args(&["--config", "a.toml"])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    #[test]
    fn dangling_and_unknown_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--operation"])).is_err());
        assert!(parse_args(&args(&["--config"])).is_err());
        assert!(parse_args(&args(&["--force"])).is_err());
        assert!(parse_args(&args(&["--operation", "merge", "--config", "a.toml"])).is_err());
    }

    /// 引数不正の exit コードは 1 (deny) ではなく 2。呼び手契約「非ゼロは全部拒否」を
    /// 崩さないため、どちらも非ゼロであることを固定する。
    #[test]
    fn usage_errors_exit_non_zero() {
        assert_eq!(run(args(&["--operation", "fix-push"])), EXIT_USAGE);
        assert_ne!(EXIT_USAGE, EXIT_ALLOWED);
        assert_ne!(EXIT_DENIED, EXIT_ALLOWED);
    }
}
