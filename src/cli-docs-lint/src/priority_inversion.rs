//! priority_inversion check — `docs/todo-summary.md` table の Tier 1 → Tier 2/3/4/5
//! 依存記述を機械検知する。
//!
//! 由来: PR #199 (Bundle W) 開始時にユーザー指摘で発覚した meta-issue。
//! 順位 34/35 (Tier 1) が 順位 19 (Tier 2) に依存する記述になっており、
//! しかも 順位 19 自身が「baseline 観測フェーズ」で着手時期未定 → 待ち先 Tier 2 が
//! 動かないため Tier 1 がデッドロックに近い扱いを受ける状態だった。
//! 同型の構造リスクを CI 層で再発防止する。
//!
//! ロジック:
//! 1. `docs/todo-summary.md` の table を parse して `(順位, Tier, 依存記述)` を抽出
//! 2. 各 row の 依存 column から `順位 NN` または `順位 NN/MM` を抽出
//! 3. 参照先の Tier が **自分より高い数値** (= 低優先度) の場合 inversion 候補
//! 4. 依存記述近傍に "land 済" 等の resolved marker があれば skip
//! 5. 残った候補を violation として報告
//!
//! 試験運用 (ADR-039): MVP は severity=warning 相当、cli-docs-lint exit code 1 で
//! 報告するが kill-switch (`CLI_DOCS_LINT_DISABLE=1`) で skip 可能。

use crate::Violation;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Tier 番号 (1-5) を `Tier N` 表記から抽出する regex。module 初期化時に
/// 1 度だけ compile する (per-row 再 compile を回避)。
static TIER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Tier\s+(\d)").expect("static Tier regex must compile"));

/// 「順位 NN」または「順位 NN/MM」から rank 番号を抽出する regex。
static RANK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"順位\s*(\d+)(?:/(\d+))?").expect("static rank regex must compile"));

/// 分割された index の全 part にマッチする name prefix。
/// `todo-summary.md` / `todo-summary2.md` / 将来の `todo-summary3.md` を含む。
const SUMMARY_FILE_PREFIX: &str = "todo-summary";

/// 依存記述近傍で「解決済」を示す substring。順位参照の **直後 RESOLUTION_WINDOW_CHARS
/// 文字以内** にいずれかが含まれていれば、その順位参照は resolved 扱い (= inversion
/// チェックから除外)。
const RESOLVED_MARKERS: &[&str] = &["land 済", "完了", "retired", "retire 済", "採用昇格済"];

/// 順位参照の **直後** から数えた **文字数** (バイト数ではない)。日本語などの
/// multi-byte 文字でも spec 通りの 80 文字 window を保証する。
const RESOLUTION_WINDOW_CHARS: usize = 80;

/// `docs/` 配下の todo-summary*.md (分割された index の全 part) を読み inversion を検査する。
///
/// 複数 part にまたがる場合も **全 part の row を統合してから** tier_by_rank を構築するため、
/// part をまたいだ cross-file 依存 (例: part1 の Tier1 が part2 の Tier2 に依存) も検査対象になる。
pub fn check(docs_dir: &Path) -> Result<Vec<Violation>, String> {
    let mut rows: Vec<TableRow> = Vec::new();
    for path in list_summary_files(docs_dir)? {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("読み込み失敗 {}: {}", path.display(), e))?;
        rows.extend(parse_table_rows(&content, &path));
    }
    Ok(check_rows(&rows))
}

/// `docs/todo-summary*.md` を name 順に列挙する (分割された index の全 part)。
fn list_summary_files(docs_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(docs_dir)
        .map_err(|e| format!("docs ディレクトリ読み込み失敗 {}: {}", docs_dir.display(), e))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with(SUMMARY_FILE_PREFIX) && name.ends_with(".md") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// 与えられた単一ファイルの markdown 内容から inversion violations を抽出する。
pub fn check_content(path: &Path, content: &str) -> Vec<Violation> {
    check_rows(&parse_table_rows(content, path))
}

/// 収集済みの全 row から inversion violations を抽出する。
/// tier_by_rank は渡された row 全体 (= 全 part) から構築するため、分割された
/// summary 間の cross-file 依存も検査される。violation は各 row の出自 (file/line) に帰属する。
fn check_rows(rows: &[TableRow]) -> Vec<Violation> {
    let tier_by_rank: HashMap<u32, u32> = rows.iter().map(|r| (r.rank, r.tier)).collect();

    let mut violations = Vec::new();
    for row in rows {
        if has_no_dependency_prefix(&row.dependency) {
            continue;
        }
        for dep_rank in extract_referenced_ranks(&row.dependency) {
            if dep_rank == row.rank {
                continue;
            }
            let Some(&dep_tier) = tier_by_rank.get(&dep_rank) else {
                continue;
            };
            if dep_tier <= row.tier {
                continue;
            }
            if is_rank_resolved(&row.dependency, dep_rank) {
                continue;
            }
            violations.push(make_violation(row, dep_rank, dep_tier));
        }
    }
    violations
}

#[derive(Debug, Clone)]
struct TableRow {
    rank: u32,
    tier: u32,
    dependency: String,
    line: usize,
    /// row の出自ファイル。分割された summary で violation を正しい part に帰属させる。
    path: PathBuf,
}

fn parse_table_rows(content: &str, path: &Path) -> Vec<TableRow> {
    content
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| parse_row(line).map(|(rank, tier, dep)| TableRow {
            rank,
            tier,
            dependency: dep,
            line: idx + 1,
            path: path.to_path_buf(),
        }))
        .collect()
}

fn parse_row(line: &str) -> Option<(u32, u32, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = trimmed
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim())
        .collect();
    if cells.len() != 6 {
        return None;
    }
    let rank: u32 = cells[0].parse().ok()?;
    let tier = parse_tier(cells[1])?;
    Some((rank, tier, cells[5].to_string()))
}

fn parse_tier(s: &str) -> Option<u32> {
    let caps = TIER_REGEX.captures(s)?;
    caps.get(1)?.as_str().parse().ok()
}

fn extract_referenced_ranks(dep: &str) -> Vec<u32> {
    let mut result = Vec::new();
    for caps in RANK_REGEX.captures_iter(dep) {
        if let Some(n) = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
            result.push(n);
        }
        if let Some(n) = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok()) {
            result.push(n);
        }
    }
    result
}

/// 依存欄が「なし」で始まるか判定する。
///
/// 「なし」「なし。」「なし (...)」のいずれも no-dependency と判定。
/// 「なし」で始まる行は括弧内に 順位 NN を書いていても integration note 扱いとし、
/// inversion チェックから除外する (false positive 抑制)。
fn has_no_dependency_prefix(dep: &str) -> bool {
    let trimmed = dep.trim_start();
    if !trimmed.starts_with("なし") {
        return false;
    }
    let after = &trimmed["なし".len()..];
    let next = after.chars().next();
    matches!(next, None | Some(' ') | Some('　') | Some('。') | Some('(') | Some('（'))
}

/// 「順位 N」or 複合参照内の「/N」近傍に resolved marker があれば true。
fn is_rank_resolved(dep: &str, rank: u32) -> bool {
    let single = format!("順位 {}", rank);
    let single_no_space = format!("順位{}", rank);
    let in_multi = format!("/{}", rank);
    for needle in [single.as_str(), single_no_space.as_str(), in_multi.as_str()] {
        if has_resolved_marker_after(dep, needle) {
            return true;
        }
    }
    false
}

fn has_resolved_marker_after(haystack: &str, needle: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = haystack[search_from..].find(needle) {
        let abs_pos = search_from + pos;
        let after = abs_pos + needle.len();
        if haystack[after..].starts_with(|c: char| c.is_ascii_digit()) {
            search_from = after;
            continue;
        }
        let window_end = haystack[after..]
            .char_indices()
            .nth(RESOLUTION_WINDOW_CHARS)
            .map(|(i, _)| after + i)
            .unwrap_or(haystack.len());
        let window = &haystack[after..window_end];
        if RESOLVED_MARKERS.iter().any(|m| window.contains(m)) {
            return true;
        }
        search_from = after;
    }
    false
}

fn make_violation(row: &TableRow, dep_rank: u32, dep_tier: u32) -> Violation {
    Violation {
        file: row.path.display().to_string(),
        line: row.line,
        message: format!(
            "priority inversion 検出: 順位 {} (Tier {}) が 順位 {} (Tier {}) に依存しています。\
             Tier N→Tier N+k 依存は待ち先 Tier の着手時期未定でデッドロック化するリスクあり。\
             依存解除 (代替経路を確認) または「land 済」等の resolved marker を追記してください",
            row.rank, row.tier, dep_rank, dep_tier
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn fake_path() -> &'static Path {
        Path::new("docs/todo-summary.md")
    }

    #[test]
    fn parse_row_extracts_rank_tier_dep() {
        let line = "| 34 | 🚀 Tier 1 | task | todo4.md | M | 順位 19 land 後推奨 |";
        let row = parse_row(line).unwrap();
        assert_eq!(row.0, 34);
        assert_eq!(row.1, 1);
        assert!(row.2.contains("順位 19"));
    }

    #[test]
    fn parse_row_rejects_header() {
        let line = "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |";
        assert!(parse_row(line).is_none());
    }

    #[test]
    fn parse_row_rejects_separator() {
        let line = "|---|---|---|---|---|---|";
        assert!(parse_row(line).is_none());
    }

    #[test]
    fn parse_tier_extracts_number_from_emoji_prefix() {
        assert_eq!(parse_tier("🚀 Tier 1"), Some(1));
        assert_eq!(parse_tier("🔧 Tier 2"), Some(2));
        assert_eq!(parse_tier("💎 Tier 3"), Some(3));
    }

    #[test]
    fn extract_referenced_ranks_single() {
        let ranks = extract_referenced_ranks("順位 19 land 後推奨");
        assert_eq!(ranks, vec![19]);
    }

    #[test]
    fn extract_referenced_ranks_multi() {
        let ranks = extract_referenced_ranks("Bundle X (順位 36/37) land 後推奨");
        assert!(ranks.contains(&36));
        assert!(ranks.contains(&37));
    }

    #[test]
    fn extract_referenced_ranks_no_reference() {
        let ranks = extract_referenced_ranks("なし");
        assert!(ranks.is_empty());
    }

    #[test]
    fn is_resolved_detects_land_zumi_in_multi() {
        let dep = "Bundle W (順位 34/35) land 済 (2026-06-07) → 着手可能";
        assert!(is_rank_resolved(dep, 34));
        assert!(is_rank_resolved(dep, 35));
    }

    #[test]
    fn is_resolved_returns_false_when_unresolved() {
        let dep = "順位 19 land 後推奨";
        assert!(!is_rank_resolved(dep, 19));
    }

    /// 順位参照と resolved marker の間に multi-byte 文字 (日本語等) が大量に挟まる
    /// ケースで、window がバイト数ではなく文字数で評価されることを保証する
    /// (CodeRabbit Major #1 / PR #200)。
    ///
    /// 例: 「順位 19」と「land 済」の間に 40 個の「あ」(= 120 bytes / 40 chars) が挟まる。
    /// byte-based の旧実装では 80 bytes window で marker を見落として false negative
    /// (= inversion 誤検出)。char-based の新実装では 80 chars window で marker を捕捉する。
    #[test]
    fn is_resolved_detects_marker_across_multibyte_gap() {
        let gap: String = "あ".repeat(40);
        let dep = format!("順位 19{} land 済", gap);
        assert!(
            is_rank_resolved(&dep, 19),
            "multibyte gap でも 80 chars window 内の marker は resolved 扱いになるべき"
        );
    }

    #[test]
    fn is_resolved_distinguishes_resolved_and_unresolved_in_same_dep() {
        let dep = "Bundle W (順位 34/35) land 済 (2026-06-07) → Bundle X (順位 36/37) land のみ残依存";
        assert!(is_rank_resolved(dep, 34));
        assert!(is_rank_resolved(dep, 35));
        assert!(!is_rank_resolved(dep, 36));
        assert!(!is_rank_resolved(dep, 37));
    }

    #[test]
    fn check_content_detects_pr199_historical_inversion() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 19 | 🔧 Tier 2 | REJECT-ESCALATE | todo3.md | M | なし |
| 34 | 🚀 Tier 1 | proptest 導入 | todo4.md | M | 順位 19 land 後推奨 |
| 35 | 🚀 Tier 1 | 型で意味を表現 | todo4.md | S | 順位 34 と同 PR |
";
        let violations = check_content(fake_path(), content);
        assert_eq!(violations.len(), 1, "expected 1 inversion (34 → 19), got {:#?}", violations);
        let v = &violations[0];
        assert!(v.message.contains("順位 34"));
        assert!(v.message.contains("Tier 1"));
        assert!(v.message.contains("順位 19"));
        assert!(v.message.contains("Tier 2"));
    }

    #[test]
    fn check_content_skips_resolved_dependency() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 19 | 🔧 Tier 2 | task | todo3.md | M | なし |
| 34 | 🚀 Tier 1 | task | todo4.md | M | 順位 19 land 済 (2026-06-07) |
";
        let violations = check_content(fake_path(), content);
        assert!(violations.is_empty(), "resolved dep should be skipped, got {:#?}", violations);
    }

    #[test]
    fn check_content_allows_tier3_depending_on_tier2() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 19 | 🔧 Tier 2 | task | todo3.md | M | なし |
| 38 | 💎 Tier 3 | task | todo4.md | S | 順位 19 land 後 |
";
        let violations = check_content(fake_path(), content);
        assert!(violations.is_empty(), "Tier 3 → Tier 2 is not inversion, got {:#?}", violations);
    }

    /// missing-rank 経路 (`tier_by_rank.get(&dep_rank) == None` での `continue`) のみ
    /// が exercise されることを保証するため、依存文字列には resolved-marker
    /// (`RESOLVED_MARKERS`) を意図的に含めない。resolved-marker を含めると将来
    /// fixture に rank 19 行を追加した際に test 経路が missing-rank → resolved-marker
    /// に silent shift する fragility を生む。
    #[test]
    fn check_content_skips_when_referenced_rank_missing_from_table() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 34 | 🚀 Tier 1 | task | todo4.md | M | 順位 19 land 後推奨 |
";
        let violations = check_content(fake_path(), content);
        assert!(violations.is_empty(), "missing rank ref should be skipped");
    }

    #[test]
    fn has_no_dependency_prefix_detects_various_forms() {
        assert!(has_no_dependency_prefix("なし"));
        assert!(has_no_dependency_prefix("なし。"));
        assert!(has_no_dependency_prefix("なし (理由付き)"));
        assert!(has_no_dependency_prefix("なし (順位 102 と整合)"));
        assert!(!has_no_dependency_prefix("順位 19 land 後推奨"));
        assert!(!has_no_dependency_prefix("Bundle X (順位 36/37) land 後"));
    }

    #[test]
    fn check_content_skips_row_starting_with_nashi() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 118 | 💎 Tier 3 | task | todo.md | XS | なし |
| 150 | 🔧 Tier 2 | task | todo.md | M | なし (順位 102 paths filter + 順位 118 適用範囲検討と整合) |
";
        let violations = check_content(fake_path(), content);
        assert!(violations.is_empty(), "「なし」で始まる行は context note 扱いで skip、got {:#?}", violations);
    }

    #[test]
    fn is_resolved_does_not_treat_prefix_as_match_for_rank_19() {
        let dep = "順位 190 land 済 (2026-06-10) → 順位 19 land 後推奨";
        assert!(!is_rank_resolved(dep, 19), "rank 19 must not be shadowed by rank 190");
        assert!(is_rank_resolved(dep, 190), "rank 190 should still be resolved");
    }

    #[test]
    fn check_content_flags_tier2_depending_on_tier3() {
        let content = "\
| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 90 | 💎 Tier 3 | task | todo.md | S | なし |
| 100 | 🔧 Tier 2 | task | todo.md | M | 順位 90 land 後 |
";
        let violations = check_content(fake_path(), content);
        assert_eq!(violations.len(), 1, "Tier 2 → Tier 3 is an inversion");
        assert!(violations[0].message.contains("順位 100"));
        assert!(violations[0].message.contains("順位 90"));
    }

    #[test]
    fn list_summary_files_returns_all_parts_sorted() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("todo-summary.md"), "").unwrap();
        fs::write(tmp.path().join("todo-summary2.md"), "").unwrap();
        fs::write(tmp.path().join("todo.md"), "").unwrap();
        fs::write(tmp.path().join("README.md"), "").unwrap();
        let files = list_summary_files(tmp.path()).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["todo-summary.md", "todo-summary2.md"]);
    }

    /// 分割された index (todo-summary.md + todo-summary2.md) を跨ぐ inversion を
    /// 統合 tier_by_rank で検出し、violation が part2 (出自ファイル) に帰属することを保証する。
    /// 分割で priority-inversion カバレッジが半減しないことの回帰テスト (Phase 3)。
    #[test]
    fn check_detects_cross_part_inversion_attributed_to_source_part() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("todo-summary.md"),
            "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n\
             | 19 | 🔧 Tier 2 | task | todo3.md | M | なし |\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("todo-summary2.md"),
            "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n\
             | 220 | 🚀 Tier 1 | task | todo14.md | M | 順位 19 land 後推奨 |\n",
        )
        .unwrap();
        let violations = check(tmp.path()).unwrap();
        assert_eq!(
            violations.len(),
            1,
            "cross-part inversion should be detected, got {:#?}",
            violations
        );
        assert!(
            violations[0].file.contains("todo-summary2.md"),
            "violation は part2 (出自) に帰属すべき: {}",
            violations[0].file
        );
        assert!(violations[0].message.contains("順位 220"));
        assert!(violations[0].message.contains("順位 19"));
    }

    /// 単一 part のみ (todo-summary.md だけ) でも従来どおり動作する回帰テスト。
    #[test]
    fn check_single_part_still_works() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("todo-summary.md"),
            "| 順位 | Tier | タスク | ファイル | 工数 | 依存 |\n\
             |---|---|---|---|---|---|\n\
             | 19 | 🔧 Tier 2 | task | todo3.md | M | なし |\n\
             | 34 | 🚀 Tier 1 | task | todo4.md | M | 順位 19 land 後推奨 |\n",
        )
        .unwrap();
        let violations = check(tmp.path()).unwrap();
        assert_eq!(violations.len(), 1);
        assert!(violations[0].file.contains("todo-summary.md"));
    }
}
