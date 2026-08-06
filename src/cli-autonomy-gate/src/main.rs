//! `cli-autonomy-gate` — 自律実行の全体 kill-switch ゲート (WP-17 PR 1、ADR-066)。
//!
//! 判定そのものは [`lib_autonomy_policy`] が持つ。本 exe はその CLI 面 (引数解析・loud 出力・
//! exit コード・telemetry) だけを担う汎用ゲートで、`pnpm autonomy-status` と drill が呼ぶ。
//!
//! Phase B の fix push 直前ゲートは `cli-fix-push-gate` を使うこと (ADR-067)。あちらは
//! kill-switch に加えて ADR-052 の target / 内容軸と ADR-054 scope guard も 1 回で評価する。
//! 本 exe と `&&` で連鎖させる運用は**しない** — 連鎖の書き忘れで kill-switch を通り越す
//! 合成ミスを構造的に防ぐため、fix push 経路は単一 exe に閉じる方針とする。
//!
//! # 使い方
//!
//! ```text
//! cli-autonomy-gate --operation <fix-push|draft-pr> --config <path> \
//!   [--open-draft-prs <count>]
//! ```
//!
//! `--open-draft-prs` は `draft-pr` の背圧入力 (ADR-071)。呼び手 (workflow step) が
//! `gh api` で数えた **未マージ draft PR (`claude/` prefix) の実測件数**を渡す。省略すると
//! 背圧未接続として deny に倒れるため、`draft-pr` では実質必須。`fix-push` の背圧は
//! cli-pr-monitor の有界 retry が担うので本フラグは判定に影響しない。
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
//! # bounded lifetime (ADR-066)
//!
//! decision trigger: Phase B 稼働後の自律 fix push 3〜5 run で (a) 有効時に通ること、
//! (b) いずれかのフラグを倒すと次の操作境界で止まること、(c) deny 理由が run log だけで
//! 切り分けられること、を確認したら本採用。2026-11-02 までに未判定なら延長 / 却下を判断する。
//!
//! # 出力
//!
//! 判定は必ず loud に出す (無音 no-op 禁止)。allow は stdout の `[AUTONOMY_ALLOW]`、deny は
//! stderr の `[AUTONOMY_OFF]` で、どちらも全ソースの状態と読み取り先 config パスを含む。

use std::path::PathBuf;

use lib_autonomy_policy::{describe_sources, evaluate, Decision, DenyReason, GateInputs, Operation};
use lib_autonomy_policy::sources;

/// 許可時の grep マーカー (stdout)。
const MARKER_ALLOW: &str = "[AUTONOMY_ALLOW]";
/// 拒否時の grep マーカー (stderr)。run log 監査の主キー。
const MARKER_DENY: &str = "[AUTONOMY_OFF]";

const EXIT_ALLOWED: i32 = 0;
const EXIT_DENIED: i32 = 1;
const EXIT_USAGE: i32 = 2;

const USAGE: &str = "usage: cli-autonomy-gate --operation <fix-push|draft-pr> --config <path> \
[--open-draft-prs <count>]";

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

/// コマンドライン設定。`--operation` / `--config` に既定値は設けない — 明示必須。
///
/// `--config` を省略可能にして cwd から推測すると、CI で master ref の写しを渡し忘れた
/// 呼び手が PR ブランチの config を黙って読む (ADR-066 § 決定 3 の信頼境界)。省略を
/// 引数不正として弾くことで、呼び手にパスの出所を必ず意識させる。
///
/// `--open-draft-prs` だけは省略可能。`fix-push` では判定に使わないためで、`draft-pr` で
/// 省略した場合は `None` = 背圧未接続として deny に倒れる (省略が許可へ倒れることはない)。
struct Cli {
    operation: Operation,
    config_path: PathBuf,
    open_draft_prs: Option<u32>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut operation = None;
    let mut config_path = None;
    let mut open_draft_prs = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        let take = || value.ok_or_else(|| format!("{flag} の値がありません"));
        match flag {
            "--operation" => {
                let raw = take()?;
                operation = Some(
                    Operation::parse(raw)
                        .ok_or_else(|| format!("未知の operation です: {raw:?}"))?,
                );
            }
            "--config" => config_path = Some(PathBuf::from(take()?)),
            "--open-draft-prs" => {
                let raw = take()?;
                open_draft_prs = Some(raw.parse::<u32>().map_err(|_| {
                    format!("--open-draft-prs は 0 以上の整数である必要があります: {raw:?}")
                })?);
            }
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    Ok(Cli {
        operation: operation.ok_or_else(|| "--operation が必要です".to_string())?,
        config_path: config_path.ok_or_else(|| "--config が必要です".to_string())?,
        open_draft_prs,
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
    let repo_config = sources::read_repo_config(&cli.config_path);
    let inputs = GateInputs {
        repo_config_enabled: repo_config.enabled,
        external_raw: external.as_deref(),
        open_draft_prs: cli.open_draft_prs,
        max_open_draft_prs: repo_config.max_open_draft_prs,
        operation: cli.operation,
    };
    report(&cli, inputs)
}

/// 判定と loud 出力。allow / deny のどちらでも全ソースの状態を 1 行目に出す。
fn report(cli: &Cli, inputs: GateInputs<'_>) -> i32 {
    let config_display = cli.config_path.display();
    let state = describe_sources(inputs, sources::EXTERNAL_ENV);
    let operation = cli.operation.as_str();
    match evaluate(inputs) {
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
        assert_eq!(cli.open_draft_prs, None, "省略時は背圧未接続 (= 停止側)");
    }

    #[test]
    fn parses_the_open_draft_count() {
        let cli = parse_args(&args(&[
            "--operation", "draft-pr",
            "--config", "a.toml",
            "--open-draft-prs", "0",
        ]))
        .expect("parse");
        assert_eq!(cli.open_draft_prs, Some(0));
    }

    /// 数えられなかった結果を空文字や負値で渡されても、`None` (= 0 件扱いになりうる形) では
    /// なく引数不正として弾く。呼び手の `gh api` が失敗した形が黙って通らないようにする。
    #[test]
    fn non_numeric_open_draft_counts_are_usage_errors() {
        for raw in ["", "-1", "3.0", "three", "1 "] {
            let parsed = parse_args(&args(&[
                "--operation", "draft-pr",
                "--config", "a.toml",
                "--open-draft-prs", raw,
            ]));
            assert!(parsed.is_err(), "{raw:?} が引数不正として弾かれない");
        }
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
