//! Collect stage — poll 結果を `.takt/review-comments.json` に書き出す。
//!
//! ## docs-only 判定を決定論層で行う理由 (順位 233)
//!
//! 以前は post-pr-review の `analyze-coderabbit` facet が `.takt/review-diff.txt` を
//! **目視して** PR が docs-only かを分類していた。この入力は 2 通りに壊れる:
//!
//! - `.takt/facets/instructions/fix.md` の refresh が tip 限定 diff で上書きするため、
//!   fix を挟んだ iteration では祖先コミットの code 変更が消える
//! - post-pr-review は push の後に走るので、pre-push run が diff を書かなかった場合
//!   (`DiffResult::Empty` で takt skip 等) **別 PR の残骸**がそのまま残る
//!
//! どちらでも「code を含む PR」を docs-only と誤判定し、CodeRabbit が PR 全体を見て
//! 出した finding を ADR-035 filter で誤って適用外にする (PR #227 実観測)。
//!
//! そこで判定を決定論層へ引き上げ、**GitHub が持つ PR 全体の変更ファイル一覧**を
//! 真実源にして `lib_docs_policy` で分類し、結果を JSON の `docs_only` として渡す。
//! facet は値を読むだけで diff を分類しない。ローカルの working copy 状態に一切
//! 依存しないため、上の 2 経路とも構造的に消える。
//!
//! fail-closed ([ADR-043]): 一覧を取得できなければ `docs_only: false` を書く。
//! docs-only 扱いは ADR-035 filter を緩める方向なので、取得失敗を `true` に倒すと
//! 「検証できなかった」が「docs だけだった」に化けて finding を握り潰す。
//!
//! **一覧が全件そろっていることも検証する**: 取得した件数と PR が申告する
//! `changedFiles` が一致しなければ `false` に倒す。「取得できた」と「全部取得できた」は
//! 別物で、後者を確かめずに分類すると、見えていないファイルが source かどうかを
//! 判断できないまま docs-only 側へ倒れる。

use std::path::Path;

use crate::log::log_info;
use crate::runner::run_gh_quiet;
use crate::stages::poll::PollResult;
use crate::util::PrInfo;

const OUTPUT_PATH: &str = ".takt/review-comments.json";

/// PollResult を .takt/review-comments.json に書き出す
///
/// instruction (analyze-coderabbit.md) が期待するフィールド:
/// action, summary, ci, coderabbit, findings, docs_only
pub(crate) fn collect_findings(result: &PollResult, pr_info: &PrInfo) -> bool {
    let docs_only = resolve_docs_only(pr_info);

    let wrapper = serde_json::json!({
        "action": result.action,
        "summary": result.summary,
        "ci": result.ci,
        "coderabbit": result.coderabbit,
        "findings": result.findings,
        "check_output": result.check_output,
        "docs_only": docs_only,
    });

    let output_path = Path::new(OUTPUT_PATH);

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log_info(&format!("{} ディレクトリ作成失敗: {}", parent.display(), e));
                return false;
            }
        }
    }

    let json = match serde_json::to_string_pretty(&wrapper) {
        Ok(j) => j,
        Err(e) => {
            log_info(&format!("review-comments JSON シリアライズ失敗: {}", e));
            return false;
        }
    };

    match std::fs::write(output_path, &json) {
        Ok(()) => {
            log_info(&format!(
                "書き出し完了: {} ({} bytes)",
                OUTPUT_PATH,
                json.len()
            ));
            true
        }
        Err(e) => {
            log_info(&format!("{} 書き込み失敗: {}", OUTPUT_PATH, e));
            false
        }
    }
}

/// PR 全体の変更ファイル一覧と、PR が申告する変更ファイル数。
///
/// 2 つを別々に取得して**件数の一致を要求する**のが本 struct の存在理由で、
/// 「一覧が全件そろっている」ことを取得元の挙動に依存せず検証するための対になっている
/// ([`classify_docs_only`])。
struct PrFileList {
    paths: Vec<String>,
    expected_count: usize,
}

/// PR 全体の変更ファイル一覧から docs-only 判定を得る (順位 233、module doc 参照)。
fn resolve_docs_only(pr_info: &PrInfo) -> bool {
    let files = fetch_pr_files(pr_info);
    let docs_only = classify_docs_only(files.as_ref());
    log_info(&format!(
        "[docs_only] PR 全体の path 基準判定 (ADR-035): {} ({})",
        docs_only,
        describe_file_list(files.as_ref())
    ));
    docs_only
}

fn describe_file_list(files: Option<&PrFileList>) -> String {
    match files {
        None => "ファイル一覧を取得できませんでした".to_string(),
        Some(f) => format!(
            "取得 {} 件 / PR 申告 {} 件",
            f.paths.len(),
            f.expected_count
        ),
    }
}

/// 取得済みのファイル一覧を分類する。gh 実行から切り離して単体テスト可能にする。
///
/// **件数の一致を先に要求する** (PR #435 CodeRabbit Major): 一覧が PR の申告数に
/// 満たなければ、見えていないファイルが source かどうかを確かめる術が無い。
/// 欠けた分を「無かった」と扱うと docs-only 側へ倒れて ADR-035 filter が有効な
/// finding を落とすため、不一致は必ず `false` にする ([ADR-043])。
/// 取得元の上限値 (`gh pr view --json files` の 100 件、REST の 3,000 件) を
/// 定数として持たないのは、上限が変わっても本検査が成立し続けるようにするため。
///
/// `None` (取得失敗) と `Some(空)` (ファイルが 1 件も無い) はどちらも `false`。
/// 前者は fail-closed、後者は `lib_docs_policy` 側の「空は docs-only ではない」と
/// 同じ規則で、判定規則を本関数に写し取らない (ADR-035 の drift 防止)。
fn classify_docs_only(files: Option<&PrFileList>) -> bool {
    let Some(files) = files else {
        return false;
    };
    if files.paths.len() != files.expected_count {
        return false;
    }
    lib_docs_policy::is_docs_only_paths(files.paths.iter().map(String::as_str))
}

/// PR 全体の変更ファイル一覧を GitHub から取得する。
///
/// `jj diff` ではなく GitHub を真実源にするのは、post-pr-review 時点のローカル
/// working copy が PR の内容と一致している保証が無いため (fix step の途中経過や、
/// 別ブランチへ移動した後の `--monitor-only` 再実行)。
///
/// `repo` が解決できない場合も `None` (fail-closed): endpoint を組み立てられない。
fn fetch_pr_files(pr_info: &PrInfo) -> Option<PrFileList> {
    let pr_number = pr_info.pr_number?;
    let repo = pr_info.repo.as_deref()?;
    let expected_count = fetch_changed_file_count(pr_number, repo)?;
    let paths = fetch_all_file_paths(pr_number, repo)?;
    Some(PrFileList {
        paths,
        expected_count,
    })
}

/// PR が申告する変更ファイル数 (`changedFiles`)。一覧の全件性を検証する対照値。
fn fetch_changed_file_count(pr_number: u64, repo: &str) -> Option<usize> {
    let number = pr_number.to_string();
    let raw = run_gh_quiet(&[
        "pr",
        "view",
        &number,
        "--repo",
        repo,
        "--json",
        "changedFiles",
        "-q",
        ".changedFiles",
    ])?;
    raw.trim().parse::<usize>().ok()
}

/// 変更ファイルのパスを **全ページ**取得する。
///
/// `gh pr view --json files` を使わないのは、100 件で**無言で切り捨てる**ため
/// (実測: 185 ファイルの PR で 100 件しか返らない / cli/cli#13338)。切り捨てられた
/// 一覧をそのまま分類すると、101 件目以降の source file が見えず docs-only へ
/// 誤って倒れる。REST の files endpoint を `--paginate` で辿る。
fn fetch_all_file_paths(pr_number: u64, repo: &str) -> Option<Vec<String>> {
    let endpoint = format!("repos/{}/pulls/{}/files", repo, pr_number);
    let raw = run_gh_quiet(&["api", "--paginate", &endpoint, "--jq", ".[].filename"])?;
    let paths: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if paths.is_empty() {
        return None;
    }
    Some(paths)
}

#[cfg(test)]
mod tests;
