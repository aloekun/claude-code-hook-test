//! transcript jsonl の時刻 range filter とプロジェクト ID 解決。
//!
//! `~/.claude/projects/<project-id>/*.jsonl` を commit 時刻 range で抽出し、
//! workflow が読む合成 transcript を書き出す。

use crate::feedback::pr_metadata::PrTimeRange;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// `cwd` パス → `~/.claude/projects/` の project ID 形式へ変換する。
///
/// Windows: `E:\work\claude-code-hook-test` → `e--work-claude-code-hook-test`
/// (lowercase、`:` `\` `/` をすべて `-` に置換)。
pub fn cwd_to_project_id(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .to_lowercase()
        .replace([':', '\\', '/'], "-")
}

/// `~/.claude/projects/<project-id>/` を返す。`USERPROFILE` 未設定なら `None`。
pub fn project_transcript_dir(cwd: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let project_id = cwd_to_project_id(cwd);
    let dir = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(project_id);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// transcript jsonl をフィルタして書き出す。
///
/// 入力: `source_dir` 配下の `*.jsonl`
/// 出力: `out_path` に [first_commit_time, merged_at] かつ type が user/assistant の行のみ
/// 戻り値: 書き込んだ行数
///
/// # 出力は timestamp 昇順であることを保証する (2026-08-18)
///
/// 旧実装はファイルを `(mtime, path)` 順に読み、その順のまま連結していた。これは
/// **決定論的ではあるが時系列ではない** — セッションが並行していれば、あるファイルが
/// 14 時間を覆う一方で別ファイルが数分を覆い、連結列の時刻が飛び飛びに前後する。
///
/// 実測 (PR #395 の範囲を再現): 1189 行中 **11 箇所で時刻が逆行**し、最大 **560 分**
/// 巻き戻っていた。先頭行は 15:18 だが実際の最古エントリは 15:02 と、先頭・末尾を見て
/// 範囲を推定すると誤る。実際 `session-analysis` facet はこの列を読み、14 時間分
/// (1189 行) が揃っているのに「2.5 分しか無い」と判断して `session_data_unavailable`
/// を報告した。**行数は合っていたのに範囲だけが誤る**ため、欠落として気づきにくい。
///
/// 決定論は失っていない。同一 timestamp は `(file_index, line_index)` で tie-break する
/// ため、同じ入力からは常に同じ出力になる ([`MatchedEntry::sort_key`])。
pub fn filter_transcripts(
    source_dir: &Path,
    range: &PrTimeRange,
    out_path: &Path,
) -> Result<usize, String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("出力ディレクトリ作成失敗 {}: {}", parent.display(), e))?;
    }

    let mut writer = fs::File::create(out_path)
        .map(std::io::BufWriter::new)
        .map_err(|e| format!("出力ファイル作成失敗 {}: {}", out_path.display(), e))?;

    let jsonl_paths = collect_jsonl_paths_in_deterministic_order(source_dir)?;
    let mut entries = collect_matching_entries(&jsonl_paths, range);
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let written = entries.len();
    for entry in entries {
        writeln!(writer, "{}", entry.line).map_err(|e| format!("出力書込失敗: {}", e))?;
    }

    writer.flush().map_err(|e| format!("flush 失敗: {}", e))?;
    Ok(written)
}

/// range に入る 1 行と、並べ替えに必要な位置情報。
struct MatchedEntry {
    /// 精度を揃えた timestamp (→ [`normalize_timestamp_for_comparison`])。
    timestamp: String,
    /// [`collect_jsonl_paths_in_deterministic_order`] が決めたファイル順の index。
    file_index: usize,
    /// ファイル内での出現順。
    line_index: usize,
    line: String,
}

impl MatchedEntry {
    fn sort_key(&self) -> (&str, usize, usize) {
        (&self.timestamp, self.file_index, self.line_index)
    }
}

/// 各ファイルを走査し、range に入る user/assistant 行を集める。
fn collect_matching_entries(jsonl_paths: &[PathBuf], range: &PrTimeRange) -> Vec<MatchedEntry> {
    let mut entries = Vec::new();
    for (file_index, path) in jsonl_paths.iter().enumerate() {
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let reader = BufReader::new(file);
        for (line_index, line) in reader.lines().map_while(Result::ok).enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Some(timestamp) = matched_timestamp(&line, range) else {
                continue;
            };
            entries.push(MatchedEntry {
                timestamp,
                file_index,
                line_index,
                line,
            });
        }
    }
    entries
}

/// `source_dir` 内の `*.jsonl` を決定論的な順序で収集する。
///
/// `fs::read_dir` の走査順は OS/filesystem 依存で非決定的なため、
/// [`transcript_ordering_key`] でソートして複数セッション jsonl 間の
/// 処理順を決定論化する (ADR-030 determinism 目標)。
fn collect_jsonl_paths_in_deterministic_order(source_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(source_dir)
        .map_err(|e| format!("transcript dir 読込失敗 {}: {}", source_dir.display(), e))?;

    let mut jsonl_paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    jsonl_paths.sort_by_key(|path| transcript_ordering_key(path));
    Ok(jsonl_paths)
}

/// transcript ソート用のキー: `(mtime, path)`。
///
/// 一次キーは mtime。mtime が同値の場合 (粒度の粗い filesystem や metadata 取得失敗で
/// `UNIX_EPOCH` に fallback したケース) でも二次キー `PathBuf` により read_dir の入力順に
/// 依存しない完全な決定論順序を保証する。
fn transcript_ordering_key(path: &Path) -> (std::time::SystemTime, PathBuf) {
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    (mtime, path.to_path_buf())
}

/// ISO 8601 UTC タイムスタンプを lexicographic 比較用に正規化する。
///
/// `gh api` は秒精度 (`…:SSZ`) を返し、Claude transcript は ms 精度 (`…:SS.fffZ`) を返す。
/// `'.'` (0x2E) < `'Z'` (0x5A) のため、精度が混在すると境界判定が狂う。
/// `Z` 末尾かつ小数部なしの文字列を `…:SS.000Z` に揃えることで同一精度での比較を保証する。
///
/// 入力契約: タイムスタンプは UTC (`Z` 末尾) であること。`+09:00` 等のオフセット形式は
/// このシステムでは現れない前提。
fn normalize_timestamp_for_comparison(ts: &str) -> String {
    if ts.ends_with('Z') && !ts.contains('.') {
        format!("{}.000Z", &ts[..ts.len() - 1])
    } else {
        ts.to_string()
    }
}

/// transcript の 1 行が時刻 range + type filter に該当すれば、正規化した timestamp を返す。
///
/// 並べ替えにも timestamp が要るため、判定と同時に取り出す (判定後に再パースしない)。
fn matched_timestamp(line: &str, range: &PrTimeRange) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    let entry_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(entry_type, "user" | "assistant") {
        return None;
    }

    let ts = normalize_timestamp_for_comparison(value.get("timestamp").and_then(|v| v.as_str())?);
    let lower = normalize_timestamp_for_comparison(range.first_commit_time.as_str());
    let upper = normalize_timestamp_for_comparison(range.merged_at.as_str());
    (ts >= lower && ts <= upper).then_some(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "feedback-filter-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_transcript_line(dir: &Path, name: &str, timestamp: &str, id: &str) -> PathBuf {
        let path = dir.join(name);
        let line = format!(r#"{{"type":"user","timestamp":"{timestamp}","id":"{id}"}}"#);
        fs::write(&path, format!("{line}\n")).unwrap();
        path
    }

    /// 1 行が range + type filter に該当するか (判定だけを見るテスト用の薄い包み)。
    fn entry_matches_filter(line: &str, range: &PrTimeRange) -> bool {
        matched_timestamp(line, range).is_some()
    }

    /// `read_first` を `read_second` より古い mtime にする。
    ///
    /// **`thread::sleep` で差をつけない。** mtime 分解能の粗い filesystem では同値になり得て、
    /// 同値だと旧実装 (ファイル順のまま連結) も path 順で `aaa` を先に出すため、
    /// [`filter_transcripts_orders_entries_chronologically_across_files`] が**旧実装でも
    /// 通ってしまう**。回帰テストの識別力を filesystem の分解能に委ねない。
    fn set_mtimes_so_the_later_timestamp_is_read_first(read_first: &Path, read_second: &Path) {
        filetime::set_file_mtime(
            read_first,
            filetime::FileTime::from_unix_time(1_745_571_600, 0),
        )
        .unwrap();
        filetime::set_file_mtime(
            read_second,
            filetime::FileTime::from_unix_time(1_745_571_601, 0),
        )
        .unwrap();
    }

    fn range_covering_0900_to_0930() -> PrTimeRange {
        PrTimeRange::without_head_branch("2026-04-25T08:00:00.000Z", "2026-04-25T10:00:00.000Z")
    }

    #[test]
    fn project_id_windows_drive() {
        let p = Path::new("E:\\work\\claude-code-hook-test");
        assert_eq!(cwd_to_project_id(p), "e--work-claude-code-hook-test");
    }

    #[test]
    fn project_id_unix_path() {
        let p = Path::new("/home/user/project");
        assert_eq!(cwd_to_project_id(p), "-home-user-project");
    }

    #[test]
    fn entry_matches_user_in_range() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_assistant_outside_range() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"assistant","timestamp":"2026-04-25T11:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_queue_operation() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_attachment() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"attachment","timestamp":"2026-04-25T09:00:00.000Z"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_invalid_json() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        assert!(!entry_matches_filter("not-json", &range));
    }

    #[test]
    fn entry_includes_boundary_timestamps() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        let upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.000Z"}"#;
        assert!(entry_matches_filter(lower, &range));
        assert!(entry_matches_filter(upper, &range));
    }

    #[test]
    fn entry_includes_lower_boundary_with_mixed_precision() {
        let range =
            PrTimeRange::without_head_branch("2026-04-25T08:00:00Z", "2026-04-25T10:00:00Z");
        let at_lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z"}"#;
        assert!(entry_matches_filter(at_lower, &range));
    }

    #[test]
    fn entry_excludes_past_upper_boundary_with_mixed_precision() {
        let range =
            PrTimeRange::without_head_branch("2026-04-25T08:00:00Z", "2026-04-25T10:00:00Z");
        let past_upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.500Z"}"#;
        assert!(!entry_matches_filter(past_upper, &range));
    }

    #[test]
    fn filter_transcripts_writes_only_in_range() {
        let dir = std::env::temp_dir().join(format!(
            "feedback-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).unwrap();

        let session_path = dir.join("session-a.jsonl");
        let mut content = String::new();
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T07:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"assistant","timestamp":"2026-04-25T09:30:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T11:00:00.000Z"}"#);
        content.push('\n');
        fs::write(&session_path, content).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let written = filter_transcripts(&dir, &range, &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("09:00:00"));
        assert!(out.contains("09:30:00"));
        assert!(!out.contains("07:00:00"));
        assert!(!out.contains("11:00:00"));
        assert!(!out.contains("queue-operation"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// **順位 446 の核心**: 出力は timestamp 昇順で、ファイルの読み順に引きずられない。
    ///
    /// fixture は「後に書かれた (= mtime が新しい) ファイルの方が古い timestamp を持つ」
    /// 交差ケース。旧実装はファイル順のまま連結したため出力の時刻が逆行し、先頭・末尾から
    /// 範囲を推定する消費側が誤った (PR #395 実測: 11 箇所逆行 / 最大 560 分巻き戻り)。
    #[test]
    fn filter_transcripts_orders_entries_chronologically_across_files() {
        let dir = unique_temp_dir("order");

        let zzz_path = write_transcript_line(
            &dir,
            "zzz-session.jsonl",
            "2026-04-25T09:05:00.000Z",
            "later-timestamp",
        );
        let aaa_path = write_transcript_line(
            &dir,
            "aaa-session.jsonl",
            "2026-04-25T09:00:00.000Z",
            "earlier-timestamp",
        );
        set_mtimes_so_the_later_timestamp_is_read_first(&zzz_path, &aaa_path);

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(&dir, &range_covering_0900_to_0930(), &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        let earlier_pos = out
            .find("earlier-timestamp")
            .expect("earlier-timestamp 行が存在する");
        let later_pos = out
            .find("later-timestamp")
            .expect("later-timestamp 行が存在する");
        assert!(
            earlier_pos < later_pos,
            "mtime / filename に関わらず timestamp の昇順で並ぶべき: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// timestamp が同値のときだけ、ファイル順 (mtime → path) が順序を決める。
    ///
    /// 時系列順にしても決定性を失わないことの担保。
    #[test]
    fn filter_transcripts_breaks_timestamp_ties_by_file_order_deterministically() {
        let dir = unique_temp_dir("tie");

        let same_timestamp = "2026-04-25T09:00:00.000Z";
        let zzz_path = write_transcript_line(&dir, "zzz-session.jsonl", same_timestamp, "zzz-line");
        let aaa_path = write_transcript_line(&dir, "aaa-session.jsonl", same_timestamp, "aaa-line");

        let shared_mtime = filetime::FileTime::from_unix_time(1_745_571_600, 0);
        filetime::set_file_mtime(&zzz_path, shared_mtime).unwrap();
        filetime::set_file_mtime(&aaa_path, shared_mtime).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(&dir, &range_covering_0900_to_0930(), &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        let aaa_pos = out.find("aaa-line").expect("aaa-line 行が存在する");
        let zzz_pos = out.find("zzz-line").expect("zzz-line 行が存在する");
        assert!(
            aaa_pos < zzz_pos,
            "timestamp 同値なら mtime 同値の二次キー PathBuf 昇順 (aaa < zzz) で決まるべき: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 同一ファイル内で timestamp が同値の行は、元の出現順を保つ。
    #[test]
    fn filter_transcripts_keeps_original_line_order_within_a_file() {
        let dir = unique_temp_dir("within-file");

        let same_timestamp = "2026-04-25T09:00:00.000Z";
        let path = dir.join("session.jsonl");
        let body = format!(
            "{{\"type\":\"user\",\"timestamp\":\"{same_timestamp}\",\"id\":\"line-1\"}}\n\
             {{\"type\":\"user\",\"timestamp\":\"{same_timestamp}\",\"id\":\"line-2\"}}\n"
        );
        fs::write(&path, body).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(&dir, &range_covering_0900_to_0930(), &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(
            out.find("line-1").unwrap() < out.find("line-2").unwrap(),
            "同一ファイル・同一 timestamp は元の行順を保つべき: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
