//! レビュー本文の指摘 (スレッドにならない Outside diff) の件数を、`list` / `show` に 1 行で出す (順位 610)。
//!
//! CodeRabbit は差分の外の行への指摘をレビュー本文に書き、スレッドにしない。スレッドだけを
//! 数えると「未解決 0 件」に見え、本文中の指摘を見落とす (#636 / #649)。
//!
//! 本文の読み取りは `check-ci-coderabbit --list-findings` が持つ (`src/check-ci-coderabbit/src/outside_diff.rs`)。
//! ここでは実行して件数を数えるだけにし、書式の知識を 2 か所に置かない。

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `check-ci-coderabbit` を待つ上限。gh api を数回呼ぶだけなので、超えたらネットワークの停止とみなす。
const LIST_FINDINGS_TIMEOUT_SECS: u64 = 120;

#[derive(Deserialize)]
struct ListFindings {
    findings: Vec<ListedFinding>,
    #[serde(default)]
    review_body_unreadable: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ListedFinding {
    source: String,
}

/// `check-ci-coderabbit --list-findings` の出力 (か失敗の理由) から、表示する 1 行を作る。
pub(crate) fn line_for(pr: u64, listed: Result<String, String>) -> String {
    const LABEL: &str = "レビュー本文の指摘 (スレッドにならない Outside diff)";
    let parsed = listed.and_then(|json| {
        serde_json::from_str::<ListFindings>(&json).map_err(|e| format!("出力を読めない: {e}"))
    });
    let output = match parsed {
        Ok(output) => output,
        Err(e) => return format!("{LABEL}: 読めなかった ({e})。本文を直接確かめる"),
    };
    let count = output.findings.iter().filter(|f| f.source == "review_body").count();
    let unreadable = output.review_body_unreadable.len();
    let detail = format!("中身は `pnpm -s check-ci --list-findings --pr {pr}` で見る");
    match (count, unreadable) {
        (0, 0) => format!("{LABEL}: 0 件"),
        (n, 0) => format!("{LABEL}: {n} 件。返信して閉じる対象ではない。{detail}"),
        (n, m) => format!("{LABEL}: {n} 件と、節を読めなかったレビューが {m} 件 (取りこぼしの疑い)。{detail}"),
    }
}

/// `check-ci-coderabbit --list-findings` を実行して、stdout (JSON) を返す。
pub(crate) fn list_findings(repo: &str, pr: u64) -> Result<String, String> {
    let exe = checker_exe_path();
    let pr = pr.to_string();
    let captured = lib_subprocess::run_cmd_direct_capture(
        &exe.to_string_lossy(),
        &["--list-findings", "--pr", &pr, "--repo", repo],
        LIST_FINDINGS_TIMEOUT_SECS,
    );
    if captured.timed_out {
        return Err(format!("{LIST_FINDINGS_TIMEOUT_SECS} 秒で終わらなかった"));
    }
    if !captured.ok {
        let first = captured.stderr.lines().next().unwrap_or("").trim().to_string();
        return Err(format!("{} が失敗した: {first}", exe.display()));
    }
    Ok(captured.stdout)
}

/// `check-ci-coderabbit` のパス。配布先では同じディレクトリに置かれる (`scripts/deploy-artifacts.mjs`)。
fn checker_exe_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("check-ci-coderabbit{}", std::env::consts::EXE_SUFFIX))
}
