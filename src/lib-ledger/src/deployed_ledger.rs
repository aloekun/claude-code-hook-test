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

use crate::identifiers::{classify_identifier, content_identifiers, parse_review_exclusions, IdentifierState, REVIEW_EXCLUSION_MARKER};
use crate::repo_index::{declared_text, raw_repository_text, repository_text};

pub(crate) fn repo_root() -> PathBuf {
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
        let Some(rank) = parse_rank(&split, rank_idx) else {
            continue;
        };
        assert!(
            split.len() > target_idx,
            "順位 {rank} の行に 対象ファイル 列がありません (必要 {} 列、実際 {} 列)。\
             実台帳の select() も同じ入力で「列数が足りません」で失敗する — \
             ここで読み飛ばすと、その行だけが書式検査からも順位重複検査からも外れる",
            target_idx + 1,
            split.len()
        );
        cells.push((rank, split[target_idx].clone()));
    }
    cells
}

/// データ行の順位を読む。`None` は「データ行ではない」(区切り行 `|---|` など) の意味で、
/// 列不足とは区別する — 前者は読み飛ばしてよく、後者は検査の穴になる。
fn parse_rank(split: &[String], rank_idx: usize) -> Option<u32> {
    split.get(rank_idx)?.parse::<u32>().ok()
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

/// ─── 実体整合の検査 (順位 491) ───
///
/// 台帳は「宣言」であって「実体」ではない。宣言と実体がずれる形は 2 つあり、入り口が違う。
///
/// - **A: 台帳の内部矛盾** — 表で auto lane を付けた順位が § 対象外 にも載っている。
///   台帳の**編集時**に生まれる。`cli-ledger-candidates` は未掲載の順位しか見ないため、
///   掲載済み行どうしの整合は誰も見ていなかった。
/// - **B: 宣言パスの漂流** — 内容欄が名指す識別子が、宣言先のファイルに無い。
///   **台帳を 1 文字も編集していなくても壊れる** (module 分割でコードが動く)。
///   [`ADR-074`](../../../docs/adr/adr-074-auto-lane-screening-criteria.md) 決定 4 の実在検査は
///   登録時 1 回きりで、しかもパスの存在しか見ない — 順位 162 の `main.rs` は**存在していた**。
///   無かったのは中身。
///
/// どちらも `cargo test` で毎回走らせる。台帳を書き換えた時点 (A) / コードを動かした時点 (B) で
/// 落ちるので、夜間ループが着手して初めて露見する状態を作らない。
///
/// タスク表 1 行分。実体整合の検査に要る列だけを持つ。
#[derive(Debug)]
struct TaskRow {
    rank: u32,
    lane: String,
    content: String,
    note: String,
}


/// タスク表のデータ行を読む。列は見出し名で解決する (列順の変更に追随するため)。
///
/// `内容` / `注意` セルは自由記述であり、`lib.rs` の `build_task` が同種の全列
/// (内容/対象ファイル/注意/PRタイトル) に適用している `reject_prompt_frame_escape`
/// (ADR-072 決定 13: `===BEGIN/END_LEDGER_DATA===` 枠エスケープ・制御文字・不可視文字の拒否)
/// をここでも通す。素通しすると、この生テキストが `drifted_identifiers` / `parse_review_exclusions`
/// を経由して `cargo test` の失敗メッセージへ無検査で埋め込まれ、その出力を読む下流の
/// fix ステップ agent の trust boundary を破りうる (SIM-NEW-lib-ledger-deployed_ledger-L281)。
/// 違反行は `TaskRow` を作らず即座に `Err` へ倒す (fail-closed)。
fn task_rows(markdown: &str) -> Result<Vec<TaskRow>, String> {
    let mut rows = Vec::new();
    let mut columns: Option<[usize; 4]> = None;
    for (index, line) in markdown.lines().enumerate() {
        if !line.trim_start().starts_with('|') {
            columns = None;
            continue;
        }
        let split = super::split_cells(line);
        if split.iter().any(|c| c == "無人可") {
            columns = Some(task_row_columns(&split));
            continue;
        }
        let Some([rank_idx, lane_idx, content_idx, note_idx]) = columns else {
            continue;
        };
        let Some(rank) = parse_rank(&split, rank_idx) else {
            continue;
        };
        let content = cell_at(&split, content_idx);
        let note = cell_at(&split, note_idx);
        let line_number = index + 1;
        super::reject_prompt_frame_escape("内容", &content, line_number)?;
        super::reject_prompt_frame_escape("注意", &note, line_number)?;
        rows.push(TaskRow {
            rank,
            lane: cell_at(&split, lane_idx),
            content,
            note,
        });
    }
    Ok(rows)
}

fn cell_at(split: &[String], index: usize) -> String {
    split.get(index).cloned().unwrap_or_default()
}

/// 見出し行から 順位 / 無人可 / 内容 / 注意 の列位置を解く。欠落は panic (検査の穴になる)。
fn task_row_columns(split: &[String]) -> [usize; 4] {
    let find = |name: &str| {
        split
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| panic!("無人可 列を持つ表に {name} 列がありません"))
    };
    [find("順位"), find("無人可"), find("内容"), find("注意")]
}

/// `### 対象外` 節に挙がっている順位を読む。次の見出しまでを節とみなす。
fn ranks_in_out_of_scope(markdown: &str) -> BTreeSet<u32> {
    let mut ranks = BTreeSet::new();
    let mut inside = false;
    for line in markdown.lines() {
        if line.starts_with("### ") {
            inside = line.starts_with("### 対象外");
            continue;
        }
        if line.starts_with("## ") {
            inside = false;
            continue;
        }
        if inside {
            ranks.extend(ranks_mentioned_in(line));
        }
    }
    ranks
}

/// 1 行から `順位 <N>` の N をすべて読む (I/O なし)。強調記号 `**` を挟む形にも当たる。
fn ranks_mentioned_in(line: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for chunk in line.split("順位").skip(1) {
        let digits: String = chunk
            .trim_start_matches(['*', ' ', '\u{3000}'])
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(rank) = digits.parse::<u32>() {
            out.push(rank);
        }
    }
    out
}

/// auto lane を表す印。台帳の凡例 (§ 無人可列は担当割り当てである) と同じ。
fn is_auto_lane(lane: &str) -> bool {
    lane.trim() == "✅"
}

/// **検査 A: auto lane の順位が § 対象外 にも載っていないこと。**
///
/// 由来: 2026-08-17 の棚卸しで順位 162 が表では `✅` (auto)、§ 対象外 では
/// 「着手前にユーザーに確認」になっていた。夜間ループは表しか見ないので着手し、
/// 人間の確認を経ずに実装が始まる。台帳の**編集時**に生まれる矛盾なので、
/// 台帳を書き換えた時点で落とす。
#[test]
fn no_auto_lane_rank_is_also_listed_as_out_of_scope() {
    let markdown = read_ledger();
    let rows = task_rows(&markdown)
        .unwrap_or_else(|message| panic!("台帳のタスク表に不正な内容があります: {message}"));
    assert!(
        !rows.is_empty(),
        "タスク表からデータ行が 1 件も取れませんでした — false-green guard"
    );
    let out_of_scope = ranks_in_out_of_scope(&markdown);
    assert!(
        !out_of_scope.is_empty(),
        "§ 対象外 から順位が 1 件も取れませんでした — false-green guard \
         (節の見出しか記法が変わった可能性。空にしたいなら本 assert ごと見直すこと)"
    );
    let conflicts: Vec<u32> = rows
        .iter()
        .filter(|r| is_auto_lane(&r.lane) && out_of_scope.contains(&r.rank))
        .map(|r| r.rank)
        .collect();
    assert!(
        conflicts.is_empty(),
        "auto lane (✅) の順位が § 対象外 にも載っています: {conflicts:?}\n\n\
         夜間ループは表しか見ないため、対象外に書いた条件を通り越して着手します。\n\
         どちらが正かを決めて、lane を `—` にするか § 対象外 の記述を削除してください。"
    );
}


/// 1 行を照合し、漂流している識別子を返す (I/O なし)。
fn drifted_identifiers(row: &TaskRow, declared: &str, repository: &str) -> Result<Vec<String>, String> {
    let excluded = parse_review_exclusions(&row.note)?;
    Ok(content_identifiers(&row.content)
        .into_iter()
        .filter(|i| !excluded.contains(i))
        .filter(|i| classify_identifier(i, declared, repository) == IdentifierState::Drifted)
        .collect())
}

/// **検査 B: 内容欄が名指す既存識別子が、宣言先のファイルに在ること。**
///
/// 由来: 2026-08-23 の run 32622369420 が順位 162 で停止した。宣言成果物が `main.rs` の
/// ままで、コードは `todo_staleness.rs` へ移っていた。**台帳を編集していないのに壊れる**ため、
/// 登録時 1 回きりの実在確認 (ADR-074 決定 4) では原理的に捕まらない。しかもパスは
/// **存在していた** — 無かったのは中身なので、パス存在検査でも捕まらない。
///
/// 限界を明示する: 内容欄が識別子を名指していない行は照合できない。#441 の棚卸しでは
/// 順位 143 / 199 / 428 がこれに当たり、**人手で個別に実測して**引き取った。本検査は
/// その 3 件を再現しない。
#[test]
fn every_declared_path_still_contains_the_identifiers_its_row_names() {
    let markdown = read_ledger();
    let rows = task_rows(&markdown)
        .unwrap_or_else(|message| panic!("台帳のタスク表に不正な内容があります: {message}"));
    let cells: std::collections::BTreeMap<u32, String> =
        target_file_cells(&markdown).into_iter().collect();
    let repository = repository_text();
    assert_control_case_is_detected(&repository);

    let mut failures = Vec::new();
    for row in &rows {
        let Some(cell) = cells.get(&row.rank) else {
            continue;
        };
        let Ok(paths) = super::parse_target_files(cell) else {
            continue;
        };
        match drifted_identifiers(row, &declared_text(&paths), &repository) {
            Ok(drifted) if !drifted.is_empty() => {
                failures.push(format!("順位 {}: {drifted:?} が {paths:?} に無い", row.rank));
            }
            Err(message) => failures.push(format!("順位 {}: {message}", row.rank)),
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "台帳の宣言先に、内容欄が名指す識別子が見つかりません ({} 件):\n  - {}\n\n\
         識別子はリポジトリの他所には存在します = 宣言パスの漂流です。\n\
         対象ファイル列を実測値へ直してください (ADR-074 決定 4)。\n\
         成果物ではない識別子 (lint rule の検出対象など) は、注意欄に\n\
         「{REVIEW_EXCLUSION_MARKER} `<識別子>`（理由）」を書いて除外してください。",
        failures.len(),
        failures.join("\n  - ")
    );
}

/// **対照実験**: 既知陽性を毎回検出できることを確かめる。
///
/// 2026-08-23 の初版スクリプトは ripgrep が空を返したのに継続し、**何も照合しないまま
/// 全行 OK と報告した**。fail-open な検査は「異常なし」と「検査していない」を区別しない
/// ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md))。索引が空・
/// 分類が壊れているといった状態をここで落とす。
fn assert_control_case_is_detected(repository: &str) {
    let known = "parse_target_files";
    assert!(
        repository.contains(known),
        "リポジトリ索引に既知の識別子 {known} がありません — 索引が空か、走査対象から\
         外れています (fail-open の兆候)"
    );
    let row = TaskRow {
        rank: 0,
        lane: "✅".to_string(),
        content: format!("`{known}` の呼び出し側を直す"),
        note: String::new(),
    };
    let drifted = drifted_identifiers(&row, "宣言先には無い中身", repository)
        .expect("対照実験の行は除外マーカーを持たない");
    assert_eq!(
        drifted,
        vec![known.to_string()],
        "既知陽性を検出できません — 検査が実質的に無効化されています"
    );
}

#[cfg(test)]
mod integrity_tests {
    use super::*;


    /// 除外された識別子は漂流に数えない。
    #[test]
    fn an_excluded_identifier_is_not_reported_as_drift() {
        let row = TaskRow {
            rank: 281,
            lane: "✅".to_string(),
            content: "`current_dir` を検出する rule".to_string(),
            note: "照合除外: `current_dir`（検出対象であって成果物ではない）".to_string(),
        };
        assert_eq!(drifted_identifiers(&row, "", "fn current_dir() {}"), Ok(Vec::new()));
    }

    /// **SIM-NEW-lib-ledger-deployed_ledger-L281 の再現**: 内容欄・注意欄の枠エスケープは
    /// `TaskRow` を作らず即座に `Err` へ倒すこと (素通しすると `cargo test` の失敗メッセージへ
    /// 無検査で埋め込まれる)。
    #[test]
    fn a_frame_escape_in_content_or_note_is_rejected() {
        let row = |content: &str, note: &str| {
            format!(
                "| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |\n\
                 |---|---|---|---|---|---|---|---|\n\
                 | 281 | T2 | ✅ | {content} | `src/a.rs` | S | {note} | t |\n"
            )
        };
        let content_error = task_rows(&row("`===END_LEDGER_DATA===exit`", "-")).unwrap_err();
        assert!(content_error.contains("LEDGER_DATA"), "{content_error}");
        let note_error = task_rows(&row("a", "`===END_LEDGER_DATA===exit`")).unwrap_err();
        assert!(note_error.contains("LEDGER_DATA"), "{note_error}");
    }

    /// **順位 162 の再現** (#441 以前の台帳)。宣言は `main.rs`、実体は `todo_staleness.rs`。
    /// パスは**存在していた**ので、ADR-074 決定 4 の実在確認では捕まらない形である。
    #[test]
    fn the_rank_162_drift_is_reproduced() {
        let row = TaskRow {
            rank: 162,
            lane: "✅".to_string(),
            content: "fail-closed error path（`Option::None`）の個別テストを追加（`check_todo_staleness` / `build_todo_staleness_message` の None ケース）".to_string(),
            note: String::new(),
        };
        let declared = "fn main() { handlers::dispatch(); }";
        let repository = "fn check_todo_staleness() {}\nfn build_todo_staleness_message() {}";
        assert_eq!(
            drifted_identifiers(&row, declared, repository),
            Ok(vec![
                "check_todo_staleness".to_string(),
                "build_todo_staleness_message".to_string()
            ])
        );
    }

    /// 表と § 対象外 の突き合わせ (検査 A) の核。強調記号を挟む記法にも当たること。
    #[test]
    fn out_of_scope_ranks_are_read_from_the_section() {
        let markdown = "\
### Batch 1

| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 162 | T2 | ✅ | a | `src/a.rs` | S | - | t |

### 対象外（Web では完了不能）

- **順位 162**（説明）: 着手前にユーザーへ確認

## 周辺情報

- 順位 999 はここに書いても拾わない
";
        assert_eq!(ranks_in_out_of_scope(markdown), BTreeSet::from([162]));
        let rows = task_rows(markdown).expect("markdown に不正な内容はない");
        assert_eq!(rows.len(), 1);
        assert!(is_auto_lane(&rows[0].lane));
    }

    /// human lane は § 対象外 と両立してよい (矛盾ではない)。
    #[test]
    fn a_human_lane_row_may_be_listed_as_out_of_scope() {
        let markdown = "\
| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 162 | T2 | — | a | `src/a.rs` | S | - | t |

### 対象外

- **順位 162**: 確認が要る
";
        let rows = task_rows(markdown).expect("markdown に不正な内容はない");
        assert!(!is_auto_lane(&rows[0].lane));
        assert!(ranks_in_out_of_scope(markdown).contains(&162));
    }
}

/// 索引からテストコードとコメントを落としたことで、分類が変わった識別子を数える (実測用)。
///
/// `cargo test -p lib-ledger -- --ignored index_pollution --nocapture`
#[test]
#[ignore = "measurement only"]
fn index_pollution_probe() {
    let markdown = read_ledger();
    let rows = task_rows(&markdown).expect("台帳");
    let cells: std::collections::BTreeMap<u32, String> =
        target_file_cells(&markdown).into_iter().collect();
    let stripped = repository_text();
    let raw = raw_repository_text();

    let mut changed = Vec::new();
    for row in &rows {
        let Some(cell) = cells.get(&row.rank) else {
            continue;
        };
        let Ok(paths) = super::parse_target_files(cell) else {
            continue;
        };
        let declared = declared_text(&paths);
        for identifier in content_identifiers(&row.content) {
            let before = classify_identifier(&identifier, &declared, &raw);
            let after = classify_identifier(&identifier, &declared, &stripped);
            if before != after {
                changed.push(format!("順位 {}: {identifier} {before:?} -> {after:?}", row.rank));
            }
        }
    }
    for line in &changed {
        println!("CHANGED {line}");
    }
    println!(
        "--- rows={} changed={} raw_bytes={} stripped_bytes={}",
        rows.len(),
        changed.len(),
        raw.len(),
        stripped.len()
    );
}

/// **索引の配線の回帰テスト。** `repository_text()` が strip を通っていることを、
/// テスト module の中にしか無い目印で確かめる。純関数側 (`production_code`) のテストだけ
/// では、呼び出し側が strip を外しても気づけない (#452 で同型の穴を踏んでいる)。
///
/// 対照の本番識別子には `completion.rs` の `evaluate` を使う — `deployed_ledger` /
/// `rust_source` は `lib.rs` 側で `#[cfg(test)] mod ..;` と宣言されファイル全体が
/// テスト扱いなので、自ファイルの識別子を対照に使うと索引の自己汚染バグを覆い隠す
/// (SIM-NEW-lib-ledger-rust_source-L75)。
#[test]
fn the_repository_index_drops_test_code() {
    let index = repository_text();
    assert!(
        !index.contains(crate::rust_source::tests::INDEX_PROBE_TOKEN),
        "索引にテストコードが載っています (strip の配線が外れています)"
    );
    assert!(
        index.contains("fn evaluate"),
        "索引から本番コードまで落ちています"
    );
}

/// **索引の自己汚染の回帰テスト (自 module 版)。**
///
/// `lib.rs` の `#[cfg(test)] mod deployed_ledger;` により、このファイル全体は本番ビルドに
/// 一切含まれないテストコードである。ファイル単体にはインラインの `#[cfg(test)]` が無いため、
/// 旧 strip (ファイル単位の [`crate::rust_source::production_code`] だけ) はこの形の
/// テスト専用ファイルを本番コードとして索引に残していた (SIM-NEW-lib-ledger-rust_source-L75)。
const SELF_MODULE_INDEX_PROBE_TOKEN: &str = "deployed_ledger_self_module_probe_token";

#[test]
fn the_repository_index_excludes_a_file_gated_by_the_parents_mod_declaration() {
    let index = repository_text();
    assert!(
        !index.contains(SELF_MODULE_INDEX_PROBE_TOKEN),
        "lib.rs の `#[cfg(test)] mod deployed_ledger;` で丸ごとテスト扱いのこのファイルが、\
         インライン `#[cfg(test)]` が無いという理由で索引に残っています"
    );
}

/// **索引の自己汚染の回帰テスト (F3)。**
///
/// 由来: PR W (順位 491) の実装中に踏んだ罠。別ファイルの doc コメントへ例示として書いた
/// 実在の識別子が索引に載り、「宣言先には無いがリポジトリには在る」= 漂流と誤検出した。
/// 当時は例示の文言を書き換えて回避したが、構造は残っていた。
#[test]
fn an_identifier_mentioned_only_in_a_doc_comment_is_not_drift() {
    let declared = "fn unrelated() {}\n";
    let elsewhere = "/// 例示: `render_row` のように書く\nfn other() {}\n";

    assert_eq!(
        classify_identifier("render_row", declared, elsewhere),
        IdentifierState::Drifted,
        "素の索引ではコメントの言及が漂流に見える (これが誤検出の正体)"
    );
    assert_eq!(
        classify_identifier(
            "render_row",
            declared,
            &crate::rust_source::production_code(elsewhere)
        ),
        IdentifierState::NotYetCreated,
        "コメントを落とせば「まだ作っていない」に戻る"
    );
}

/// テスト module の中だけに在る識別子も、索引では「リポジトリに在る」と数えない。
#[test]
fn an_identifier_only_in_a_test_module_is_not_drift() {
    let declared = "fn unrelated() {}\n";
    let elsewhere = "fn other() {}\n#[cfg(test)]\nmod tests {\n    fn calls_render_row() { render_row(); }\n}\n";

    assert_eq!(
        classify_identifier("render_row", declared, elsewhere),
        IdentifierState::Drifted
    );
    assert_eq!(
        classify_identifier(
            "render_row",
            declared,
            &crate::rust_source::production_code(elsewhere)
        ),
        IdentifierState::NotYetCreated
    );
}

/// **本番コードに在る識別子は従来どおり漂流として検出する** (strip が効きすぎていないこと)。
/// この対照が無いと「全部落として緑」でもテストが通ってしまう。
#[test]
fn an_identifier_in_production_code_elsewhere_is_still_drift() {
    let declared = "fn unrelated() {}\n";
    let elsewhere = "pub fn render_row(row: &str) -> String { row.to_string() }\n";

    assert_eq!(
        classify_identifier(
            "render_row",
            declared,
            &crate::rust_source::production_code(elsewhere)
        ),
        IdentifierState::Drifted
    );
}

/// 宣言先はテストコードも数える (成果物自体がテストの行がある。順位 457 が実例)。
#[test]
fn the_declared_side_counts_test_code() {
    let declared_with_test_only_artifact =
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn rule_test_coverage_check() {}\n}\n";
    assert_eq!(
        classify_identifier(
            "rule_test_coverage_check",
            declared_with_test_only_artifact,
            ""
        ),
        IdentifierState::Declared,
        "宣言先まで strip すると『検査を足す』型のタスクが漂流に化ける"
    );
}

