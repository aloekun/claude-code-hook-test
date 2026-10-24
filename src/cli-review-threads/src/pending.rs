//! 返信して閉じる予定のスレッドの登録 (`.claude/review-threads-pending.json`)。
//!
//! 登録は修正・棄却したセッションが行い、`apply` が push の後に処理して消す (ADR-087)。
//! 処理に失敗した登録は残し、次の `apply` で再試行する。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 登録ファイルの名前 (`.claude/` の下)。`.gitignore` 済み。
pub(crate) const FILE_NAME: &str = "review-threads-pending.json";

/// 対応の種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    /// 修正した。返信に push した commit を書く。
    Fixed,
    /// 対応しない。返信に理由を書く。
    Rejected,
}

/// 登録 1 件。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) repo: String,
    pub(crate) pr: u64,
    pub(crate) branch: String,
    pub(crate) thread_id: String,
    pub(crate) comment_id: u64,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) kind: Kind,
    /// 修正なら対応の要約、棄却なら理由。
    pub(crate) note: String,
    pub(crate) registered_at: String,
    /// 返信は済んだが resolve に失敗したとき true。再試行で同じ返信を二度書かない。
    #[serde(default)]
    pub(crate) replied: bool,
    /// 前回の `apply` で失敗した理由。成功すれば登録ごと消える。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_error: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    entries: Vec<Entry>,
}

/// 登録ファイルの場所。exe と同じ `.claude/` に置く (workspace ごとに分かれる、ADR-045)。
pub(crate) fn default_path() -> Result<PathBuf, String> {
    lib_jj_helpers::pipeline_lock::exe_claude_dir().map(|d| d.join(FILE_NAME)).ok_or_else(|| "exe の場所を読めなかった".to_string())
}

/// 登録を読む。ファイルが無ければ空。壊れていれば読まずに止める (登録を黙って捨てないため)。
pub(crate) fn load(path: &Path) -> Result<Vec<Entry>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<File>(&text)
            .map(|f| f.entries)
            .map_err(|e| format!("{} を読めなかった (直すか消してください): {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{} を開けなかった: {e}", path.display())),
    }
}

/// 登録を書く。空なら消す。
pub(crate) fn save(path: &Path, entries: &[Entry]) -> Result<(), String> {
    if entries.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{} を消せなかった: {e}", path.display())),
        };
    }
    let text = serde_json::to_string_pretty(&File { entries: entries.to_vec() }).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{} に書けなかった: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{} に置き換えられなかった: {e}", path.display())
    })
}

/// 同じスレッドの登録があれば置き換え、無ければ足す。置き換えるときも、返信済みの印は引き継ぐ。
/// 引き継がないと、resolve だけ失敗した登録を登録し直したときに、同じ返信を二度書く
/// (CodeRabbit の指摘)。
pub(crate) fn upsert(entries: &mut Vec<Entry>, mut entry: Entry) {
    if let Some(old) = entries.iter().find(|e| e.thread_id == entry.thread_id) {
        entry.replied |= old.replied;
    }
    entries.retain(|e| e.thread_id != entry.thread_id);
    entries.push(entry);
}
