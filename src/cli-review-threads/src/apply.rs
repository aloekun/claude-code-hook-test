//! 登録したスレッドに返信して閉じる (push の後に呼ぶ、ADR-087)。

use crate::gh::{self, Gh};
use crate::pending::{Entry, Kind};
use crate::threads::Thread;

/// 返信に書く commit の桁数。
const SHORT_SHA_LEN: usize = 8;

/// 処理の結果。`kept` は登録ファイルに残すもの (失敗・別の PR の登録)。
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    pub(crate) kept: Vec<Entry>,
    pub(crate) closed: usize,
    pub(crate) report: Vec<String>,
}

/// いま push した PR。
pub(crate) struct Target<'a> {
    pub(crate) repo: &'a str,
    pub(crate) pr: u64,
    /// PR の head commit (修正の返信に書く)。
    pub(crate) head: &'a str,
    /// PR のスレッドをすべて読めたか。読めなかったスレッドがあれば、見つからない登録は
    /// 消えたスレッドのものと断定できないので、消さずに残す。
    pub(crate) all_read: bool,
}

/// スレッドへの返信の本文。
pub(crate) fn reply_body(entry: &Entry, head: &str) -> String {
    match entry.kind {
        Kind::Fixed => {
            let sha: String = head.chars().take(SHORT_SHA_LEN).collect();
            format!("{sha} で対応しました。\n\n{}", entry.note)
        }
        Kind::Rejected => format!("対応しません。\n\n理由: {}", entry.note),
    }
}

/// 登録を 1 件ずつ処理する。別の PR の登録は残し、見つからないスレッドの登録は消す。
/// 返信か resolve に失敗した登録は、理由を付けて残す。
pub(crate) fn apply(gh: &dyn Gh, target: &Target, entries: Vec<Entry>, threads: &[Thread]) -> Outcome {
    let mut outcome = Outcome::default();
    for entry in entries {
        let label = format!("{} ({})", entry.title, entry.path);
        if entry.repo != target.repo || entry.pr != target.pr {
            outcome.report.push(format!("別の PR (#{}) の登録なので残す: {label}", entry.pr));
            outcome.kept.push(entry);
            continue;
        }
        match threads.iter().find(|t| t.id == entry.thread_id) {
            None if !target.all_read => {
                outcome.report.push(format!("読めなかったスレッドがあるので、見つからない登録を残す: {label}"));
                outcome.kept.push(entry);
            }
            None => outcome.report.push(format!("スレッドが見つからないので登録を消す: {label}")),
            Some(t) => handle(gh, target, t.resolved, entry, &label, &mut outcome),
        }
    }
    outcome
}

/// 見つかったスレッドの登録を処理する。もう閉じている (CodeRabbit や人が先に resolve した) スレッドにも、
/// まだ返信していなければ返信だけは書く — 書かずに消すと、対応の要約や棄却の理由がスレッドに残らない
/// (pre-push review の指摘)。
fn handle(gh: &dyn Gh, target: &Target, resolved: bool, mut entry: Entry, label: &str, outcome: &mut Outcome) {
    let result = match (resolved, entry.replied) {
        (true, true) => {
            outcome.report.push(format!("もう閉じていて返信も済んでいるので登録を消す: {label}"));
            return;
        }
        (true, false) => reply_once(gh, target, &mut entry).map(|()| "閉じていたので返信だけした"),
        (false, _) => close(gh, target, &mut entry).map(|()| "返信して閉じた"),
    };
    match result {
        Ok(done) => {
            outcome.closed += usize::from(!resolved);
            outcome.report.push(format!("{done}: {label}"));
        }
        Err(e) => {
            outcome.report.push(format!("失敗したので登録を残す: {label}: {e}"));
            entry.last_error = Some(e);
            outcome.kept.push(entry);
        }
    }
}

fn reply_once(gh: &dyn Gh, target: &Target, entry: &mut Entry) -> Result<(), String> {
    if !entry.replied {
        gh::reply(gh, target.repo, target.pr, entry.comment_id, &reply_body(entry, target.head))
            .map_err(|e| format!("返信に失敗: {e}"))?;
        entry.replied = true;
    }
    Ok(())
}

fn close(gh: &dyn Gh, target: &Target, entry: &mut Entry) -> Result<(), String> {
    reply_once(gh, target, entry)?;
    gh::resolve(gh, &entry.thread_id).map_err(|e| format!("resolve に失敗: {e}"))
}

/// 登録の無い未解決のスレッドの案内。登録を忘れても気づけるように、push の後に出す。
pub(crate) fn unregistered_notice(threads: &[Thread], entries: &[Entry]) -> Option<String> {
    let open: Vec<&Thread> = threads
        .iter()
        .filter(|t| !t.resolved && !entries.iter().any(|e| e.thread_id == t.id))
        .collect();
    if open.is_empty() {
        return None;
    }
    Some(format!(
        "未解決で、対応の登録が無いスレッドが {} 件あります。直したものは `pnpm threads fixed <番号> \"<対応の要約>\"`、\
         対応しないものは `pnpm threads rejected <番号> \"<理由>\"` で登録すると、次の push の後に返信して閉じます \
         (番号は `pnpm threads list`)。",
        open.len()
    ))
}
