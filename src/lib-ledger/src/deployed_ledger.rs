//! 実台帳 (`docs/claude-code-web-tasks.md`) が機械可読の契約を満たすことを固定する検査。
//!
//! # なぜ実ファイルを読むのか
//!
//! [`super::parse_target_files`] の unit test は合成入力で書式を固定するが、**実際の台帳が
//! 契約を満たしているか**は別問題である。人間が新しい行を追記したとき、曖昧な書き方
//! (裸のファイル名 / 引用符の無い成果物) が混ざれば、後続の後始末機構はその順位を
//! 「検証不能」として扱い続ける。無言で自動化の対象外が増えると「自動化したのに半分手作業」へ寄る。
//!
//! そこで **push 時と CI で毎回、実台帳の全行を parse し直す**。書式を外した行を足した時点で
//! 赤くなるので、書いた人へ即座に返る (`.claude/custom-lint-rules.toml` を実読する
//! `rule_test_coverage_check` / `orphan_fixture_check` と同じ形)。
//!
//! # なぜ統合テストではなくクレート内 `#[cfg(test)]` なのか
//!
//! 表の解釈は [`super::split_cells`] と [`super::resolve_target_files_column`] が持つが、
//! どちらも private である。統合テスト (`tests/`) から使うには公開するしかなく、初版は
//! 手で再実装して**本体から静かに乖離した** — 末尾エスケープの取りこぼしと、あいまい列を
//! 黙って先頭採用する差 (SIM-NEW-lib-ledger-deployed_ledger-L53)。#394 型の見逃しを捕まえる
//! ための検査が、自分の側で見逃す形になっていた。
//!
//! かといってテスト都合で `pub` を足すと、本 crate の公開面が恒久的に広がる。ここは夜間ループの
//! 「何を実装してよいか」を決める入口であり、`Cargo.toml` が依存を足さない理由と同じ論理で
//! **表面も絞る**。クレート内 `#[cfg(test)]` なら private のまま同じ関数を共有でき、公開面は
//! 増えない。

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ledger_path() -> PathBuf {
    repo_root().join("docs").join("claude-code-web-tasks.md")
}

fn read_ledger() -> String {
    let path = ledger_path();
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "台帳を読めません ({}): {e} (false-green guard: 読めないまま緑にしない)",
            path.display()
        )
    })
}

/// 無人可 列を持つ表 (= 選択対象のタスク表) のデータ行から「対象ファイル」セルを取り出す。
///
/// セル分解と列解決は本体の関数をそのまま使う。あいまい列は本体と同じくエラーへ倒す —
/// ここで先頭を黙って採用すると、実際の [`super::select`] がエラーになる台帳を検査だけが
/// 通してしまう。
fn target_file_cells(markdown: &str) -> Vec<(u32, String)> {
    let mut cells = Vec::new();
    let mut columns: Option<(usize, usize)> = None;
    for line in markdown.lines() {
        if !line.trim_start().starts_with('|') {
            columns = None;
            continue;
        }
        let split = super::split_cells(line);
        if split.iter().any(|c| c == "無人可") {
            columns = Some(header_columns_for_check(&split));
            continue;
        }
        let Some((rank_idx, target_idx)) = columns else {
            continue;
        };
        if split.len() <= rank_idx.max(target_idx) {
            continue;
        }
        let Ok(rank) = split[rank_idx].parse::<u32>() else {
            continue;
        };
        cells.push((rank, split[target_idx].clone()));
    }
    cells
}

fn header_columns_for_check(split: &[String]) -> (usize, usize) {
    let Some(rank) = split.iter().position(|c| c == "順位") else {
        panic!("無人可 列を持つ表に 順位 列がありません (実台帳の select() も同じ入力で失敗する)")
    };
    let target = super::resolve_target_files_column(split).unwrap_or_else(|message| {
        panic!("対象ファイル 列を解決できません (実台帳の select() も同じ入力で失敗する): {message}")
    });
    (rank, target)
}

#[test]
fn every_target_files_cell_in_the_deployed_ledger_is_machine_readable() {
    let markdown = read_ledger();
    let cells = target_file_cells(&markdown);
    assert!(
        !cells.is_empty(),
        "無人可 列を持つ表からデータ行が 1 件も取れませんでした — false-green guard \
         (台帳の表構成が変わった可能性)"
    );

    let mut failures: Vec<String> = Vec::new();
    for (rank, cell) in &cells {
        if let Err(message) = super::parse_target_files(cell) {
            failures.push(format!("順位 {rank}: {message}"));
        }
    }
    assert!(
        failures.is_empty(),
        "台帳の「対象ファイル」セルが機械可読の契約を満たしていません ({} 件):\n  - {}\n\n\
         書式: 注釈 (丸括弧) を除いた本体は、リポジトリ相対パスのバッククォート引用と `+` のみ。\n\
         成果物はすべてバッククォートで囲み、`main.rs` のような裸のファイル名ではなく\n\
         `src/<crate>/src/main.rs` と書くこと。",
        failures.len(),
        failures.join("\n  - ")
    );
}

/// 順位の重複が無いことも同時に固定する。重複したまま後始末が走ると、どちらの行を
/// 消すべきか決まらない。
#[test]
fn deployed_ledger_task_ranks_are_unique() {
    let markdown = read_ledger();
    let cells = target_file_cells(&markdown);
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (rank, _) in &cells {
        if !seen.insert(*rank) {
            duplicates.push(*rank);
        }
    }
    assert!(
        duplicates.is_empty(),
        "タスク表に重複した順位があります: {duplicates:?}"
    );
}
