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
use std::path::{Path, PathBuf};

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

/// 内容欄が名指す識別子の、宣言先から見た状態。
#[derive(Debug, PartialEq, Eq)]
enum IdentifierState {
    /// 宣言先に在る。健全。
    Declared,
    /// **宣言先には無いが、リポジトリの他所には在る** = 漂流。
    Drifted,
    /// リポジトリのどこにも無い = これから作る成果物。検査対象外。
    NotYetCreated,
}

/// 内容欄のバッククォート引用から、照合対象の識別子を取り出す (I/O なし)。
///
/// **Rust 識別子の形だけを採る。** 内容欄のバッククォートには識別子以外も入る —
/// 型の一部 (`Option::None`)、CLI 引数 (`--pr 0`)、パス (`src/foo.rs`)、
/// 文字列リテラル (`"custom-block"`)。2026-08-25 の実測では、素朴に全部を識別子として
/// 扱うと 30 行中 12 行が偽陽性になり、この絞り込みで 3 行まで落ちた。
///
/// 末尾の `(...)` は落とす (`some_fn(&str)` → `some_fn`)。**例に実在の識別子を書かないこと** —
/// 本ファイル自身がリポジトリ索引に入るため、台帳が名指す識別子をここへ書くと
/// 「これから作る」行が「リポジトリに在る」= 漂流へ誤分類される (2026-08-25 に実際に踏んだ)。
/// 3 文字以下は一般語との衝突が多いので採らない。
fn content_identifiers(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for span in backtick_spans(content) {
        let bare = match span.split_once('(') {
            Some((head, _)) => head,
            None => span.as_str(),
        };
        if bare.len() > 3 && is_rust_identifier(bare) {
            out.push(bare.to_string());
        }
    }
    out
}

/// バッククォートで囲まれた区間を列挙する (I/O なし)。閉じていない引用は捨てる。
fn backtick_spans(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        out.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    out
}

fn is_rust_identifier(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 識別子 1 件を分類する (I/O なし。ファイルの中身は呼び出し元が読んで渡す)。
///
/// **「リポジトリのどこにも無い」を漂流に数えない**のが要点。台帳は*これからやる作業*を
/// 書く場所なので、内容欄が名指す識別子には「既存コード (漂流の signal)」と
/// 「これから作るもの (ただの予定)」が混ざる。両者を構文では見分けられないが、
/// **リポジトリ全体に在るかどうか**が決定的な差になる。
fn classify_identifier(identifier: &str, declared: &str, repository: &str) -> IdentifierState {
    if contains_token(declared, identifier) {
        IdentifierState::Declared
    } else if contains_token(repository, identifier) {
        IdentifierState::Drifted
    } else {
        IdentifierState::NotYetCreated
    }
}

/// `haystack` が `identifier` を**トークンとして**含むか (I/O なし)。
///
/// 素の [`str::contains`] は部分一致なので、`render_row` が `render_rows` に当たる。
/// これは**漂流を見逃す**向きに効く — 宣言先から `render_row` が消えても、同じファイルに
/// `render_rows` が残っていれば `Declared` と読んでしまう (CodeRabbit #447)。
/// 前後が識別子文字でないことを確かめて、接頭辞・接尾辞一致を弾く。
fn contains_token(haystack: &str, identifier: &str) -> bool {
    let bytes = haystack.as_bytes();
    let width = identifier.len();
    haystack.match_indices(identifier).any(|(start, _)| {
        let before_ok = start == 0 || !is_identifier_byte(bytes[start - 1]);
        let after = start + width;
        let after_ok = after >= bytes.len() || !is_identifier_byte(bytes[after]);
        before_ok && after_ok
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// 注意欄の照合除外マーカー。
///
/// 書式: `照合除外: ` + バッククォート識別子 + `（理由）`。**理由は必須**で、
/// 空だと [`Err`] に倒す — 理由の無い除外は「なぜ通しているか分からない穴」になり、
/// 検査を骨抜きにする経路がそこだけ無検査になる。
///
/// 除外を台帳の行に置くのは、行を削除すれば除外も一緒に消えるため。テスト側の allowlist に
/// 置くと、台帳から行が消えても除外だけが残って腐る。
const REVIEW_EXCLUSION_MARKER: &str = "照合除外:";

/// 注意欄から照合除外の識別子を読む (I/O なし)。理由が無ければ `Err`。
fn parse_review_exclusions(note: &str) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for chunk in note.split(REVIEW_EXCLUSION_MARKER).skip(1) {
        let Some(identifier) = backtick_spans(chunk).into_iter().next() else {
            return Err(format!(
                "{REVIEW_EXCLUSION_MARKER} の後にバッククォート引用の識別子がありません"
            ));
        };
        let after_ident = chunk
            .split_once(&format!("`{identifier}`"))
            .map(|(_, tail)| tail)
            .unwrap_or("");
        if reason_of(after_ident).is_empty() {
            return Err(format!(
                "照合除外 `{identifier}` に理由 (全角丸括弧) がありません"
            ));
        }
        out.insert(identifier);
    }
    Ok(out)
}

/// 除外マーカー直後の全角丸括弧から理由を読む (I/O なし)。
fn reason_of(after_identifier: &str) -> String {
    let Some(open) = after_identifier.find('（') else {
        return String::new();
    };
    let tail = &after_identifier[open + '（'.len_utf8()..];
    let Some(close) = tail.find('）') else {
        return String::new();
    };
    tail[..close].trim().to_string()
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

/// リポジトリ索引から外すディレクトリ。ビルド生成物・VCS 内部・実行ログは実体ではない。
const INDEX_SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".jj", ".takt", "docs"];

/// リポジトリ索引に入れる拡張子。**`.md` は入れない** — 台帳と todo 自身が識別子を
/// 名指しているため、含めると「これから作る識別子」まで「リポジトリに在る」= 漂流と読む
/// (2026-08-25 実測: 除外前は 3 件すべてが漂流に誤分類された)。
const INDEX_EXTENSIONS: &[&str] = &["rs", "toml", "mjs", "ts", "yml", "sh"];

fn has_indexed_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| INDEX_EXTENSIONS.contains(&e))
}

/// ディレクトリ配下のファイル内容を連結して読む。読めないファイルは飛ばす。
fn concat_files(root: &Path, indexed_only: bool, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !INDEX_SKIP_DIRS.contains(&name.as_ref()) {
                concat_files(&path, indexed_only, out);
            }
        } else if !indexed_only || has_indexed_extension(&path) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

/// 宣言パス群の中身を連結して読む (ディレクトリ宣言はその配下すべて)。
fn declared_text(paths: &[String]) -> String {
    let root = repo_root();
    let mut text = String::new();
    for relative in paths {
        let path = root.join(relative);
        if path.is_dir() {
            concat_files(&path, false, &mut text);
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            text.push_str(&content);
            text.push('\n');
        }
    }
    text
}

/// リポジトリ全体のコードを 1 本の文字列として索引する。
fn repository_text() -> String {
    let mut text = String::new();
    concat_files(&repo_root(), true, &mut text);
    text
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

    /// 識別子でないバッククォートは照合対象にしない (2026-08-25 実測の偽陽性 7 種)。
    #[test]
    fn only_rust_identifiers_are_collected_from_the_content_cell() {
        let content = "`Option::None` と `--pr 0` と `src/foo.rs` と `\"custom-block\"` を直す";
        assert!(content_identifiers(content).is_empty(), "{:?}", content_identifiers(content));
    }

    /// 末尾の引数リストは落として本体だけを採る。
    #[test]
    fn a_trailing_argument_list_is_stripped() {
        assert_eq!(content_identifiers("`render_row(&str)` を pub 化"), vec!["render_row"]);
    }

    /// 3 文字以下は一般語と衝突するので採らない。
    #[test]
    fn very_short_identifiers_are_ignored() {
        assert!(content_identifiers("`id` と `run` を直す").is_empty());
    }

    /// 閉じていないバッククォートで後続を巻き込まない。
    #[test]
    fn an_unclosed_backtick_does_not_swallow_the_rest() {
        assert_eq!(content_identifiers("`alpha` と `beta"), vec!["alpha"]);
    }

    /// **検査 B の核**: 宣言先に無く、リポジトリの他所に在れば漂流。
    #[test]
    fn an_identifier_missing_from_the_declared_path_but_present_elsewhere_is_drift() {
        assert_eq!(
            classify_identifier("check_todo_staleness", "fn main() {}", "fn check_todo_staleness()"),
            IdentifierState::Drifted
        );
    }

    /// **これから作る識別子は漂流ではない** — 台帳は未着手の作業を書く場所なので、
    /// ここを漂流に数えると未着手行がすべて赤くなる (実測 30 行中 12 行)。
    #[test]
    fn an_identifier_absent_from_the_whole_repository_is_not_yet_created() {
        assert_eq!(
            classify_identifier("brand_new_helper", "fn main() {}", "fn main() {}"),
            IdentifierState::NotYetCreated
        );
    }

    /// **接頭辞一致で漂流を見逃さない** (CodeRabbit #447)。素の `contains` だと
    /// `render_row` が `render_rows` に当たり、宣言先から消えていても `Declared` と読む。
    #[test]
    fn a_longer_name_sharing_the_prefix_does_not_count_as_a_match() {
        assert_eq!(
            classify_identifier("render_row", "fn render_rows() {}", "fn render_row() {}"),
            IdentifierState::Drifted
        );
    }

    /// 接尾辞側も同じ。`row_id` が `first_row_id` に当たってはいけない。
    #[test]
    fn a_longer_name_sharing_the_suffix_does_not_count_as_a_match() {
        assert!(!contains_token("let first_row_id = 1;", "row_id"));
    }

    /// 識別子文字でない区切り (`::` / `(` / 行頭行末) は境界として通す。
    #[test]
    fn non_identifier_neighbours_are_valid_boundaries() {
        assert!(contains_token("std::env::current_dir()", "current_dir"));
        assert!(contains_token("current_dir", "current_dir"));
        assert!(contains_token("fn current_dir(", "current_dir"));
    }

    /// 同じ行に接頭辞一致と真の一致が混在しても検出できる。
    #[test]
    fn a_true_match_after_a_prefix_match_is_still_found() {
        assert!(contains_token("render_rows(); render_row();", "render_row"));
    }

    #[test]
    fn an_identifier_present_at_the_declared_path_is_declared() {
        assert_eq!(
            classify_identifier("alpha", "fn alpha() {}", "fn alpha() {}"),
            IdentifierState::Declared
        );
    }

    #[test]
    fn a_note_without_a_marker_excludes_nothing() {
        assert_eq!(parse_review_exclusions("ふつうの注意書き"), Ok(BTreeSet::new()));
    }

    #[test]
    fn a_marker_with_a_reason_excludes_the_identifier() {
        let note = "検出対象の説明。照合除外: `current_dir`（lint rule の検出対象であって成果物ではない）";
        assert_eq!(
            parse_review_exclusions(note),
            Ok(BTreeSet::from(["current_dir".to_string()]))
        );
    }

    /// **理由の無い除外は拒否する。** 通すと「なぜ通しているか分からない穴」が残り、
    /// 検査を骨抜きにする経路がそこだけ無検査になる。
    #[test]
    fn a_marker_without_a_reason_is_rejected() {
        let error = parse_review_exclusions("照合除外: `current_dir`").unwrap_err();
        assert!(error.contains("理由"), "{error}");
    }

    #[test]
    fn a_marker_without_an_identifier_is_rejected() {
        let error = parse_review_exclusions("照合除外: current_dir（理由）").unwrap_err();
        assert!(error.contains("識別子"), "{error}");
    }

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
