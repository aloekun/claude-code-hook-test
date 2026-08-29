//! 台帳が複数ファイルへ分割された状態で validator 群が壊れないことを、**実ファイル経由**で
//! 固定する統合テスト (defect-convergence-plan.md § Phase F の F2)。
//!
//! # なぜ統合テストなのか
//!
//! 単体テストは「読み込んだ内容をどう解釈するか」を固定するが、**どのファイルを読むかを
//! 決める層**は素通りする。#452 で実際に踏んだ形で、`read_all_detail_files` を空 Map へ
//! 差し替える変異がテストを通り抜けた。ここでは tempdir に実ファイルを置き、公開 API と
//! 実 exe の両方から呼ぶ。
//!
//! # 何を見るか (F1 との重複を作らない)
//!
//! 「3 validator が `todo-summary3.md` をそろって認識する」は F1 が `lib.rs` の
//! `shared_summary_definition_tests` で固定済みなので、ここでは**未カバーの分割形**だけを扱う:
//! 詳細ファイル側の分割 / part の欠番 / summary が 1 つも無い場合 / 分割後に足した
//! 詳細ファイルの走査。

use std::path::Path;
use std::process::Command;

use cli_docs_lint::{entry_pairing, preamble, priority_inversion, Violation};

const TABLE_HEADER: &str = "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
                            |---|---|---|---|---|---|\n";

const TASK_BODY: &str = "> **動機**: x\n\n#### 完了基準\n\n- done\n";

/// tempdir に `docs/` 相当を作る最小ヘルパー。**これが F2 の「fixture template」**で、
/// 分割シナリオごとにファイル一式を並べるだけの薄い層に留める (順位 465 が実際に来た
/// 時点で必要な形へ広げる)。
fn docs_with(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, content) in files {
        std::fs::write(dir.path().join(name), content).expect("write");
    }
    dir
}

fn summary_row(rank: u32, detail_file: &str) -> String {
    format!("| {rank} | 🚀 Tier 1 | **タスク {rank}** | {detail_file} | S | なし |\n")
}

fn detail_entry(rank: u32) -> String {
    format!("### 順位 {rank}: タスク {rank}\n\n{TASK_BODY}")
}

fn all_checks(docs_dir: &Path) -> Vec<Violation> {
    let mut violations = entry_pairing::check(docs_dir).expect("entry-pairing");
    violations.extend(priority_inversion::check(docs_dir).expect("priority-inversion"));
    violations.extend(preamble::check(docs_dir).expect("preamble"));
    violations
}

/// 詳細エントリが `todoN.md` 複数へ散っていても、順位 table 側の 1 行と対応が取れる。
#[test]
fn detail_entries_split_across_several_files_are_paired() {
    let docs = docs_with(&[
        (
            "todo-summary.md",
            format!("{TABLE_HEADER}{}{}{}", summary_row(10, "todo1.md"), summary_row(20, "todo2.md"), summary_row(30, "todo3.md")),
        ),
        ("todo1.md", format!("# TODO\n\n{}", detail_entry(10))),
        ("todo2.md", format!("# TODO\n\n{}", detail_entry(20))),
        ("todo3.md", format!("# TODO\n\n{}", detail_entry(30))),
    ]);
    assert!(all_checks(docs.path()).is_empty(), "{:?}", all_checks(docs.path()));
}

/// **part 番号が飛んでいても動く。** `todo-summary2.md` を削除して 3 へ移した状態を
/// 想定する — 分割 part は連番であることを前提にしない。
#[test]
fn a_gap_in_the_split_summary_parts_is_tolerated() {
    let docs = docs_with(&[
        ("todo-summary.md", format!("{TABLE_HEADER}{}", summary_row(10, "todo1.md"))),
        ("todo-summary3.md", format!("{TABLE_HEADER}{}", summary_row(20, "todo1.md"))),
        (
            "todo1.md",
            format!("# TODO\n\n{}{}", detail_entry(10), detail_entry(20)),
        ),
    ]);
    assert!(all_checks(docs.path()).is_empty(), "{:?}", all_checks(docs.path()));
}

/// **分割後に足した詳細ファイルも走査される。** 順位 table から参照されていない
/// `todo9.md` に孤児が残っていれば報告する (方向 B1)。
#[test]
fn a_detail_file_added_after_the_split_is_still_scanned() {
    let docs = docs_with(&[
        ("todo-summary.md", format!("{TABLE_HEADER}{}", summary_row(10, "todo1.md"))),
        ("todo-summary3.md", "# 空\n".to_string()),
        ("todo1.md", format!("# TODO\n\n{}", detail_entry(10))),
        ("todo9.md", format!("# TODO\n\n{}", detail_entry(99))),
    ]);
    let violations = entry_pairing::check(docs.path()).expect("entry-pairing");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0].file, "todo9.md", "{:?}", violations[0]);
    assert!(violations[0].message.contains("順位 99"), "{:?}", violations[0]);
}

/// **順位 table が 1 行も読めない構成は Err に倒す** (fail-closed)。
/// 「分割の途中でファイルを取り違えた」状態を緑で通すと、以降の検査が空振りし続ける。
#[test]
fn a_docs_dir_without_any_summary_row_is_an_error() {
    let docs = docs_with(&[
        ("todo-summary.md", "# 空\n".to_string()),
        ("todo1.md", format!("# TODO\n\n{}", detail_entry(10))),
    ]);
    let error = entry_pairing::check(docs.path()).unwrap_err();
    assert!(error.contains("false-green guard"), "{error}");
}

/// **実 exe を分割構成に対して走らせる。** 公開 API 経由のテストだけでは、CLI の
/// 引数解決や check の配線が外れても気づけない。
#[test]
fn the_real_exe_passes_on_a_split_ledger() {
    let docs = docs_with(&[
        ("todo-summary.md", format!("{TABLE_HEADER}{}", summary_row(10, "todo1.md"))),
        ("todo-summary3.md", format!("{TABLE_HEADER}{}", summary_row(20, "todo2.md"))),
        ("todo1.md", format!("# TODO\n\n{}", detail_entry(10))),
        ("todo2.md", format!("# TODO\n\n{}", detail_entry(20))),
    ]);
    let status = Command::new(env!("CARGO_BIN_EXE_cli-docs-lint"))
        .args(["--check", "entry-pairing", "--docs-dir"])
        .arg(docs.path())
        .status()
        .expect("run cli-docs-lint");
    assert_eq!(status.code(), Some(0), "分割構成で違反なしのはずが exit != 0");
}

/// 実 exe は分割構成でも違反を見つけたら非ゼロで終わる (上の対照)。
#[test]
fn the_real_exe_reports_a_violation_on_a_split_ledger() {
    let docs = docs_with(&[
        ("todo-summary.md", format!("{TABLE_HEADER}{}", summary_row(10, "todo1.md"))),
        ("todo-summary3.md", format!("{TABLE_HEADER}{}", summary_row(20, "todo2.md"))),
        ("todo1.md", format!("# TODO\n\n{}", detail_entry(10))),
        ("todo2.md", "# TODO\n".to_string()),
    ]);
    let status = Command::new(env!("CARGO_BIN_EXE_cli-docs-lint"))
        .args(["--check", "entry-pairing", "--docs-dir"])
        .arg(docs.path())
        .status()
        .expect("run cli-docs-lint");
    assert_eq!(status.code(), Some(1), "詳細エントリ欠落を検出できていない");
}
