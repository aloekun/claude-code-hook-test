//! transcript jsonl の時刻 range filter。
//!
//! `~/.claude/projects/<project-id>/*.jsonl` を commit 時刻 range で抽出し、
//! workflow が読む合成 transcript を書き出す。置き場所の解決は [`super::project_dir`]、
//! 各行の縮約は [`super::transcript_compact`] が持つ。

use crate::feedback::pr_metadata::PrTimeRange;
use crate::feedback::transcript_compact::compact_entry;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// transcript jsonl をフィルタして書き出す。
///
/// 入力: `sources` の各 transcript dir 配下の `*.jsonl`
/// 出力: `out_path` に [first_commit_time, merged_at] かつ type が user/assistant の行のみ。
/// 各行は [`compact_entry`] で縮約してから書く (重複・暗号化 thinking・付帯データを捨て、
/// 長い正常 tool_result の中ほどを省略する)
/// 戻り値: 書き込んだ行数
///
/// # 複数 workspace を横断する (順位 469)
///
/// `sources` は `(transcript dir, その workspace root)` の組。各エントリは **`cwd` が
/// 対の workspace root 配下にあること**を必須条件として通す。フォルダを増やすだけでは
/// 無関係なセッションを引き込むため、陽性一致で束縛する。
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
/// 決定論は失っていない。同一 timestamp は `(source_path, line_index)` で tie-break する
/// ため、同じ入力からは常に同じ出力になる ([`MatchedEntry::sort_key`])。
pub fn filter_transcripts(
    sources: &[(PathBuf, PathBuf)],
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

    let mut entries = Vec::new();
    for (source_dir, workspace_root) in sources {
        let Some(jsonl_paths) = read_jsonl_paths_or_skip(source_dir) else {
            continue;
        };
        entries.extend(collect_matching_entries(
            &jsonl_paths,
            range,
            workspace_root,
        ));
    }
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let written = entries.len();
    for entry in entries {
        writeln!(writer, "{}", entry.line).map_err(|e| format!("出力書込失敗: {}", e))?;
    }

    writer.flush().map_err(|e| format!("flush 失敗: {}", e))?;
    Ok(written)
}

/// 1 つの transcript dir を読む。読めなければログを残して `None`。
///
/// # 1 つの source の失敗で全 workspace 分を失わない
///
/// source が複数になったため、**1 ディレクトリが読めないだけで全体を `Err` にすると
/// 他の workspace の行も 1 行も出なくなる**。`workspace_transcript_dirs` は実在する
/// ディレクトリだけを返すが、解決から読み取りまでの間に削除や権限変更が起きうる
/// (TOCTOU)。
///
/// この層は助言用の入力生成なので fail-open が正しい ([ADR-043](../../../../docs/adr/adr-043-security-gates-fail-closed.md)
/// の助言層の扱い)。ただし**黙って飛ばさない** — 欠落に気づけなくなるのは順位 469 で
/// 直した問題そのものなので、飛ばした事実は必ずログへ出す。
fn read_jsonl_paths_or_skip(source_dir: &Path) -> Option<Vec<PathBuf>> {
    match collect_jsonl_paths_in_deterministic_order(source_dir) {
        Ok(paths) => Some(paths),
        Err(e) => {
            eprintln!(
                "[merge-pipeline] [feedback] transcript dir を読めないため飛ばします {}: {}",
                source_dir.display(),
                e
            );
            None
        }
    }
}

/// range に入る 1 行と、並べ替えに必要な位置情報。
struct MatchedEntry {
    /// 精度を揃えた timestamp (→ [`normalize_timestamp_for_comparison`])。
    timestamp: String,
    /// 出所を一意にするパス。同一 timestamp の tie-break に使う。
    ///
    /// workspace を横断すると別ディレクトリの同じ順番の行が並びうるため、
    /// ディレクトリ内の index だけでは順序が決まらない。
    source_path: PathBuf,
    /// ファイル内での出現順。
    line_index: usize,
    line: String,
}

impl MatchedEntry {
    fn sort_key(&self) -> (&str, &Path, usize) {
        (&self.timestamp, self.source_path.as_path(), self.line_index)
    }
}

/// 各ファイルを走査し、range に入る user/assistant 行のうち **`workspace_root` 配下の
/// セッションのもの**だけを集め、縮約した形で保持する。
fn collect_matching_entries(
    jsonl_paths: &[PathBuf],
    range: &PrTimeRange,
    workspace_root: &Path,
) -> Vec<MatchedEntry> {
    let mut entries = Vec::new();
    for path in jsonl_paths {
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let reader = BufReader::new(file);
        for (line_index, line) in reader.lines().map_while(Result::ok).enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let Some(timestamp) = matched_timestamp(&value, range, workspace_root) else {
                continue;
            };
            entries.push(MatchedEntry {
                timestamp,
                source_path: path.clone(),
                line_index,
                line: compact_entry(&value).to_string(),
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

/// transcript の 1 行が時刻 range + type filter + workspace 所属を満たせば、
/// 正規化した timestamp を返す。
///
/// 並べ替えにも timestamp が要るため、判定と同時に取り出す。パースは呼び手が 1 回だけ行い、
/// 同じ値を縮約にも使う。
///
/// # `cwd` は必須 (順位 469)
///
/// 複数の project-id フォルダを走査するようになったため、**エントリがどの workspace の
/// ものかを本文で確認する**。`cwd` を持たない行は判定できないので通さない (実測では
/// 全 89,625 エントリが `cwd` を持つため、実質的な取りこぼしは無い)。`cwd` は判定にだけ
/// 使い、縮約で出力からは落とす。
fn matched_timestamp(
    value: &serde_json::Value,
    range: &PrTimeRange,
    workspace_root: &Path,
) -> Option<String> {
    let entry_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(entry_type, "user" | "assistant") {
        return None;
    }

    let cwd = value.get("cwd").and_then(|v| v.as_str())?;
    if !lib_jj_helpers::is_inside_workspace(cwd, workspace_root) {
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

    /// テストが使う workspace root。fixture の `cwd` はこの配下にする。
    const WORKSPACE_ROOT: &str = "/repo";

    fn write_transcript_line(dir: &Path, name: &str, timestamp: &str, id: &str) -> PathBuf {
        write_transcript_line_with_cwd(dir, name, timestamp, id, WORKSPACE_ROOT)
    }

    /// 目印の `id` は `message.content` に置く (最上位の未知フィールドは縮約で落ちるため)。
    fn write_transcript_line_with_cwd(
        dir: &Path,
        name: &str,
        timestamp: &str,
        id: &str,
        cwd: &str,
    ) -> PathBuf {
        let path = dir.join(name);
        let line = format!(
            r#"{{"type":"user","timestamp":"{timestamp}","cwd":"{cwd}","message":{{"role":"user","content":"{id}"}}}}"#
        );
        fs::write(&path, format!("{line}\n")).unwrap();
        path
    }

    /// `(transcript dir, workspace root)` の組を 1 つだけ持つ走査対象。
    fn single_source(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
        vec![(dir.to_path_buf(), PathBuf::from(WORKSPACE_ROOT))]
    }

    /// 1 行が range + type filter に該当するか (判定だけを見るテスト用の薄い包み)。
    fn entry_matches_filter(line: &str, range: &PrTimeRange) -> bool {
        serde_json::from_str(line)
            .ok()
            .and_then(|value| matched_timestamp(&value, range, Path::new(WORKSPACE_ROOT)))
            .is_some()
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
    fn entry_matches_user_in_range() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo"}"#;
        assert!(entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_assistant_outside_range() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"assistant","timestamp":"2026-04-25T11:00:00.000Z","cwd":"/repo"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_queue_operation() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line =
            r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo"}"#;
        assert!(!entry_matches_filter(line, &range));
    }

    #[test]
    fn entry_skips_attachment() {
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let line = r#"{"type":"attachment","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo"}"#;
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
        let lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z","cwd":"/repo"}"#;
        let upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.000Z","cwd":"/repo"}"#;
        assert!(entry_matches_filter(lower, &range));
        assert!(entry_matches_filter(upper, &range));
    }

    #[test]
    fn entry_includes_lower_boundary_with_mixed_precision() {
        let range =
            PrTimeRange::without_head_branch("2026-04-25T08:00:00Z", "2026-04-25T10:00:00Z");
        let at_lower = r#"{"type":"user","timestamp":"2026-04-25T08:00:00.000Z","cwd":"/repo"}"#;
        assert!(entry_matches_filter(at_lower, &range));
    }

    #[test]
    fn entry_excludes_past_upper_boundary_with_mixed_precision() {
        let range =
            PrTimeRange::without_head_branch("2026-04-25T08:00:00Z", "2026-04-25T10:00:00Z");
        let past_upper = r#"{"type":"user","timestamp":"2026-04-25T10:00:00.500Z","cwd":"/repo"}"#;
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
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T07:00:00.000Z","cwd":"/repo"}"#);
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo"}"#);
        content.push('\n');
        content.push_str(
            r#"{"type":"assistant","timestamp":"2026-04-25T09:30:00.000Z","cwd":"/repo"}"#,
        );
        content.push('\n');
        content.push_str(
            r#"{"type":"queue-operation","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo"}"#,
        );
        content.push('\n');
        content.push_str(r#"{"type":"user","timestamp":"2026-04-25T11:00:00.000Z","cwd":"/repo"}"#);
        content.push('\n');
        fs::write(&session_path, content).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let range = PrTimeRange::without_head_branch(
            "2026-04-25T08:00:00.000Z",
            "2026-04-25T10:00:00.000Z",
        );
        let written = filter_transcripts(&single_source(&dir), &range, &out_path).unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("09:00:00"));
        assert!(out.contains("09:30:00"));
        assert!(!out.contains("07:00:00"));
        assert!(!out.contains("11:00:00"));
        assert!(!out.contains("queue-operation"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// 書き出す行は縮約済みであること (配線の確認。縮約規則自体は `transcript_compact` のテスト)。
    #[test]
    fn filter_transcripts_writes_compacted_entries() {
        let dir = unique_temp_dir("compact");
        let line = r#"{"type":"assistant","timestamp":"2026-04-25T09:00:00.000Z","cwd":"/repo","uuid":"u-1","toolUseResult":{"stdout":"dup"},"message":{"role":"assistant","usage":{"input_tokens":1},"content":[{"type":"thinking","thinking":"","signature":"sig"},{"type":"text","text":"kept"}]}}"#;
        fs::write(dir.join("session.jsonl"), format!("{line}\n")).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(
            &single_source(&dir),
            &range_covering_0900_to_0930(),
            &out_path,
        )
        .unwrap();
        assert_eq!(written, 1);

        let out: serde_json::Value =
            serde_json::from_str(fs::read_to_string(&out_path).unwrap().trim()).unwrap();
        assert_eq!(
            out,
            serde_json::json!({
                "type": "assistant",
                "timestamp": "2026-04-25T09:00:00.000Z",
                "message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "kept" }],
                },
            })
        );

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
        let written = filter_transcripts(
            &single_source(&dir),
            &range_covering_0900_to_0930(),
            &out_path,
        )
        .unwrap();
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

    /// timestamp が同値のときは、**出所のパス**が順序を決める (→ [`MatchedEntry::sort_key`])。
    ///
    /// 時系列順にしても決定性を失わないことの担保。
    ///
    /// **mtime は tie-break に入らない** (順位 469 で変更)。workspace を横断すると別
    /// ディレクトリの行が混ざるため、ディレクトリ内の順番 index では順序が決まらない。
    /// path を直接キーにしたことで `collect_jsonl_paths_in_deterministic_order` の
    /// `(mtime, path)` 順は最終出力の順序へ影響しなくなった (決定論は保たれる)。
    #[test]
    fn filter_transcripts_breaks_timestamp_ties_by_source_path_deterministically() {
        let dir = unique_temp_dir("tie");

        let same_timestamp = "2026-04-25T09:00:00.000Z";
        let zzz_path = write_transcript_line(&dir, "zzz-session.jsonl", same_timestamp, "zzz-line");
        let aaa_path = write_transcript_line(&dir, "aaa-session.jsonl", same_timestamp, "aaa-line");

        let shared_mtime = filetime::FileTime::from_unix_time(1_745_571_600, 0);
        filetime::set_file_mtime(&zzz_path, shared_mtime).unwrap();
        filetime::set_file_mtime(&aaa_path, shared_mtime).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(
            &single_source(&dir),
            &range_covering_0900_to_0930(),
            &out_path,
        )
        .unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        let aaa_pos = out.find("aaa-line").expect("aaa-line 行が存在する");
        let zzz_pos = out.find("zzz-line").expect("zzz-line 行が存在する");
        assert!(
            aaa_pos < zzz_pos,
            "timestamp 同値なら出所パスの昇順 (aaa < zzz) で決まるべき: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 同一ファイル内で timestamp が同値の行は、元の出現順を保つ。
    /// **順位 469 の核心**: 別 workspace のセッションも拾う。
    ///
    /// PR #417 は `improve` workspace で実装され (編集 31 件)、main からマージされたため
    /// 実装セッションが分析入力から丸ごと落ちた。走査対象を workspace ごとに持つ。
    #[test]
    fn filter_transcripts_collects_sessions_from_every_workspace() {
        let main_dir = unique_temp_dir("ws-main");
        let other_dir = unique_temp_dir("ws-other");
        write_transcript_line_with_cwd(
            &main_dir,
            "main.jsonl",
            "2026-04-25T09:00:00.000Z",
            "from-main",
            "/repo",
        );
        write_transcript_line_with_cwd(
            &other_dir,
            "other.jsonl",
            "2026-04-25T09:30:00.000Z",
            "from-other",
            "/repo-improve",
        );

        let out_path = main_dir.join("filtered.jsonl");
        let sources = vec![
            (main_dir.clone(), PathBuf::from("/repo")),
            (other_dir.clone(), PathBuf::from("/repo-improve")),
        ];
        let written =
            filter_transcripts(&sources, &range_covering_0900_to_0930(), &out_path).unwrap();

        assert_eq!(written, 2, "両 workspace のセッションを拾う");
        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("from-main") && out.contains("from-other"));
        assert!(
            out.find("from-main").unwrap() < out.find("from-other").unwrap(),
            "workspace を跨いでも timestamp 昇順: {out}"
        );

        let _ = fs::remove_dir_all(&main_dir);
        let _ = fs::remove_dir_all(&other_dir);
    }

    /// **1 つの source が読めなくても他の workspace 分は出す** (fail-open)。
    ///
    /// source が複数になったため、1 ディレクトリの読み取り失敗を全体の `Err` にすると
    /// 他の workspace の行も 1 行も出なくなる。`workspace_transcript_dirs` は実在する
    /// ディレクトリだけを返すが、解決から読み取りまでに削除されうる (TOCTOU)。
    #[test]
    fn filter_transcripts_skips_an_unreadable_source_and_keeps_the_others() {
        let good_dir = unique_temp_dir("fail-open-good");
        let missing_dir = good_dir.join("does-not-exist");
        write_transcript_line(
            &good_dir,
            "good.jsonl",
            "2026-04-25T09:00:00.000Z",
            "survivor",
        );

        let out_path = good_dir.join("filtered.jsonl");
        let sources = vec![
            (missing_dir, PathBuf::from(WORKSPACE_ROOT)),
            (good_dir.clone(), PathBuf::from(WORKSPACE_ROOT)),
        ];
        let written =
            filter_transcripts(&sources, &range_covering_0900_to_0930(), &out_path).unwrap();

        assert_eq!(written, 1, "読めない source は飛ばし、他は出す");
        assert!(fs::read_to_string(&out_path).unwrap().contains("survivor"));

        let _ = fs::remove_dir_all(&good_dir);
    }

    /// **広げるだけにしない**: `cwd` が対の workspace root 配下でないエントリは通さない。
    ///
    /// project-id フォルダには別リポジトリのセッションが同居しうる。フォルダを増やした
    /// だけで無条件に取り込むと、無関係な知見が feedback に混入する。
    #[test]
    fn filter_transcripts_rejects_entries_whose_cwd_is_outside_the_workspace() {
        let dir = unique_temp_dir("ws-foreign");
        write_transcript_line_with_cwd(
            &dir,
            "mine.jsonl",
            "2026-04-25T09:00:00.000Z",
            "mine",
            "/repo/src",
        );
        write_transcript_line_with_cwd(
            &dir,
            "foreign.jsonl",
            "2026-04-25T09:10:00.000Z",
            "foreign",
            "/somewhere/else",
        );
        write_transcript_line_with_cwd(
            &dir,
            "sibling.jsonl",
            "2026-04-25T09:20:00.000Z",
            "sibling",
            "/repo-improve",
        );

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(
            &single_source(&dir),
            &range_covering_0900_to_0930(),
            &out_path,
        )
        .unwrap();

        assert_eq!(written, 1, "workspace root 配下のエントリだけ通す");
        let out = fs::read_to_string(&out_path).unwrap();
        assert!(out.contains("mine"), "サブディレクトリ起動は通す: {out}");
        assert!(!out.contains("foreign"), "無関係な cwd は落とす: {out}");
        assert!(
            !out.contains("sibling"),
            "前方一致するだけの別 workspace は落とす: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_transcripts_keeps_original_line_order_within_a_file() {
        let dir = unique_temp_dir("within-file");

        let same_timestamp = "2026-04-25T09:00:00.000Z";
        let path = dir.join("session.jsonl");
        let body = format!(
            "{{\"type\":\"user\",\"timestamp\":\"{same_timestamp}\",\
             \"message\":{{\"role\":\"user\",\"content\":\"line-1\"}},\
             \"cwd\":\"{WORKSPACE_ROOT}\"}}\n\
             {{\"type\":\"user\",\"timestamp\":\"{same_timestamp}\",\
             \"message\":{{\"role\":\"user\",\"content\":\"line-2\"}},\
             \"cwd\":\"{WORKSPACE_ROOT}\"}}\n"
        );
        fs::write(&path, body).unwrap();

        let out_path = dir.join("filtered.jsonl");
        let written = filter_transcripts(
            &single_source(&dir),
            &range_covering_0900_to_0930(),
            &out_path,
        )
        .unwrap();
        assert_eq!(written, 2);

        let out = fs::read_to_string(&out_path).unwrap();
        assert!(
            out.find("line-1").unwrap() < out.find("line-2").unwrap(),
            "同一ファイル・同一 timestamp は元の行順を保つべき: {out}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
