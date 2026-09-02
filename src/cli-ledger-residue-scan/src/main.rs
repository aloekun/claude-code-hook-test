//! cli-ledger-residue-scan — マージ済みなのに台帳に残る順位を洗い出す。
//!
//! 台帳の全順位を、`gh pr list --state merged` の結果と照合する。判定は [`detect`] の
//! 純関数で、本ファイルは I/O (台帳と JSON の読み取り) と報告だけを持つ。
//!
//! # gh を自分で起動しない
//!
//! マージ済み PR の取得は呼び手 (workflow の step / `pnpm` script) が行い、その JSON を
//! `--merged-prs` で渡す。`Count open claude/ PRs` step が `gh pr list --json` を shell で
//! 呼び、判定だけを exe に置いているのと同じ形である — **取得は shell、判定は exe**
//! ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 1)。テストからも
//! ネットワークが消える。
//!
//! # 使い方
//!
//! ```text
//! gh pr list --repo <owner/repo> --state merged --limit 1000 \
//!   --json number,headRefName,mergedAt > merged.json
//! cli-ledger-residue-scan --ledger docs/claude-code-web-tasks.md \
//!   --merged-prs merged.json --limit 1000
//! ```
//!
//! **`--limit` は gh へ渡した値と同じにする。** 取得側と検査側で値がずれると、飽和の
//! 判定が成立しない。値は実測で決めること — 2026-09-02 時点でマージ済み PR は 453 件で、
//! 300 では飽和する。
//!
//! # 出力契約
//!
//! stdout に **必ず 1 行** `ranks=<csv>` を出す (残骸が無ければ `ranks=`)。呼び手はこれを
//! `--exclude-ranks` へ合流させる。人間向けの行は stderr へ出す。
//!
//! # 終了コード
//!
//! - 0 — 走査できた (残骸の有無は `ranks=` で伝える。**残骸ありでも 0**)
//! - 2 — 走査できなかった (**fail-closed**。取得上限に張り付いた場合を含む)
//!
//! 残骸ありを非 0 にしないのは、呼び手が「走査の失敗」と「残骸の発見」を取り違えないため。
//! 色を赤にする判断は `cli-nightly-outcome` が `ranks=` を見て行う。

mod detect;

use std::path::{Path, PathBuf};

use detect::{ranks_csv, residue, MergedPr};

const EXIT_ERROR: u8 = 2;
/// 射程の明示。**「0 件」を「台帳は健全」と読み違えさせない。**
const SCOPE_NOTE: &str = "[NIGHTLY_LEDGER_RESIDUE] 注: 本走査は `claude/nightly-<順位>` の \
    マージ済み PR だけを見ます。人間が別名ブランチで実装して後始末を忘れた分は検出できません。";

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => return fail(&format!("引数エラー: {message}")),
    };
    let ledger_ranks = match read(&cli.ledger).and_then(|markdown| {
        lib_ledger::parse_ledger_ranks(&markdown)
            .map_err(|e| format!("台帳を読めません ({}): {e}", cli.ledger.display()))
    }) {
        Ok(ranks) => ranks,
        Err(message) => return fail(&message),
    };
    let merged = match read(&cli.merged_prs).and_then(|json| parse_merged(&json, cli.limit)) {
        Ok(merged) => merged,
        Err(message) => return fail(&message),
    };

    let found = residue(&ledger_ranks, &merged);
    println!("ranks={}", ranks_csv(&found));
    if found.is_empty() {
        eprintln!("[NIGHTLY_LEDGER_RESIDUE] 台帳に残骸はありません (照合した順位 {} 件)", ledger_ranks.len());
    } else {
        for item in &found {
            eprintln!("{}", item.line());
        }
        eprintln!("{REMEDY}");
    }
    eprintln!("{SCOPE_NOTE}");
    std::process::ExitCode::SUCCESS
}

const REMEDY: &str = "\
[NIGHTLY_LEDGER_RESIDUE] 対処: 該当順位を台帳から消してください —
  cli-ledger-cleanup --ledger docs/claude-code-web-tasks.md --ranks <順位> \\
    --changed-files <変更ファイル一覧> --apply
  (台帳行 / 順位 table 行 / 詳細エントリの 3 点セットを順位で引いて消します)";

fn fail(message: &str) -> std::process::ExitCode {
    eprintln!("[NIGHTLY_LEDGER_RESIDUE_ERROR] {message}");
    std::process::ExitCode::from(EXIT_ERROR)
}

struct Cli {
    ledger: PathBuf,
    merged_prs: PathBuf,
    limit: usize,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut ledger = None;
    let mut merged_prs = None;
    let mut limit = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} の値がありません"))?;
        match flag {
            "--ledger" => ledger = Some(PathBuf::from(value)),
            "--merged-prs" => merged_prs = Some(PathBuf::from(value)),
            "--limit" => {
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| format!("--limit を整数として読めません: {value:?}"))?,
                )
            }
            other => return Err(format!("未知の引数です: {other:?}")),
        }
        index += 2;
    }
    Ok(Cli {
        ledger: ledger.ok_or("--ledger が必要です")?,
        merged_prs: merged_prs.ok_or("--merged-prs が必要です")?,
        limit: limit.ok_or("--limit が必要です")?,
    })
}

/// `gh pr list --json number,headRefName,mergedAt` の出力を読む。
///
/// **取得件数が上限に張り付いていたら失敗させる。** 数え落とした分に残骸があると
/// 「残骸なし」と報告してしまい、検査そのものが false-green になる
/// (`Count open claude/ PRs` step が背圧で同じ判断をしている)。
fn parse_merged(json: &str, limit: usize) -> Result<Vec<MergedPr>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("マージ済み PR の JSON を読めません: {e}"))?;
    let array = value
        .as_array()
        .ok_or("マージ済み PR の JSON が配列ではありません")?;
    if array.len() >= limit {
        return Err(format!(
            "マージ済み PR の取得が上限 ({limit}) に張り付いており、数え落としがありえます \
             (--limit を上げて取得し直してください)"
        ));
    }
    array.iter().map(parse_one).collect()
}

fn parse_one(item: &serde_json::Value) -> Result<MergedPr, String> {
    let field = |name: &str| -> Result<String, String> {
        item.get(name)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("PR の JSON に文字列の {name} がありません: {item}"))
    };
    let number = item
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("PR の JSON に number がありません: {item}"))?;
    Ok(MergedPr {
        number,
        head_ref: field("headRefName")?,
        merged_at: field("mergedAt")?,
    })
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{} を読めません: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_PRS: &str = r#"[
        {"number": 427, "headRefName": "claude/nightly-324", "mergedAt": "2026-08-30T12:22:36Z"},
        {"number": 468, "headRefName": "feat/x", "mergedAt": "2026-09-02T10:03:42Z"}
    ]"#;

    #[test]
    fn the_gh_json_shape_is_parsed() {
        let merged = parse_merged(TWO_PRS, 100).expect("parse");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].number, 427);
        assert_eq!(merged[0].head_ref, "claude/nightly-324");
        assert_eq!(merged[0].merged_at, "2026-08-30T12:22:36Z");
    }

    /// **取得上限に張り付いたら失敗させる。** 数え落としを「残骸なし」と報告しない。
    #[test]
    fn a_saturated_fetch_is_an_error() {
        let error = parse_merged(TWO_PRS, 2).expect_err("should fail");
        assert!(error.contains("上限"), "{error}");
    }

    /// 欠けたフィールドを既定値で埋めない (壊れた入力で緑にしない)。
    #[test]
    fn a_missing_field_is_an_error() {
        let json = r#"[{"number": 1, "headRefName": "claude/nightly-1"}]"#;
        assert!(parse_merged(json, 100).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_merged("not json", 100).is_err());
        assert!(parse_merged(r#"{"a": 1}"#, 100).is_err());
    }

    #[test]
    fn every_flag_is_required() {
        let args: Vec<String> = ["cli-ledger-residue-scan", "--ledger", "x"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&args).is_err());
    }
}
