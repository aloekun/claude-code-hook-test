//! 対応したレビュースレッドを、push の後に返信して閉じる (ADR-087)。
//!
//! 使い方 (`pnpm threads <サブコマンド>`):
//!   list [--pr <番号>]            いまのブランチの PR (か指定の PR) の未解決スレッドを番号付きで出す
//!   show [--pr <番号>] [--full]   閉じたものも含めて、全スレッドの状態と返信を出す (--full で返信の本文も)
//!   fixed <番号|URL> <要約>       修正したスレッドとして登録する
//!   rejected <番号|URL> <理由>    対応しないスレッドとして登録する
//!   forget <番号|URL>             登録を取り消す
//!   apply                        登録したスレッドに返信して閉じる (`pnpm push` が push の後に呼ぶ)
//!
//! 対応済みかを判断するのは、修正・棄却したセッション (登録する側)。`apply` は登録どおりに
//! 返信と resolve を行うだけで、判断はしない。`apply` は失敗しても終了コード 0 で終わる
//! (push を失敗扱いにしない。失敗した登録は残し、次の `apply` で再試行する)。

mod apply;
mod gh;
mod pending;
mod threads;

use gh::{Gh, RealGh};
use lib_jj_helpers::StderrMode;
use pending::{Entry, Kind};
use std::path::Path;
use threads::Thread;

/// いまのブランチの PR。
struct Pr {
    repo: String,
    branch: String,
    number: u64,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    lib_jj_helpers::inject_git_dir_for_gh(|m| eprintln!("[review-threads] {m}"));
    let gh = RealGh;
    let code = match args.first().map(String::as_str) {
        Some("apply") => run_apply(&gh),
        Some("list") => report(run_list(&gh, &args[1..])),
        Some("show") => report(run_show(&gh, &args[1..])),
        Some("fixed") => report(run_register(&gh, Kind::Fixed, &args[1..])),
        Some("rejected") => report(run_register(&gh, Kind::Rejected, &args[1..])),
        Some("forget") => report(run_forget(&gh, &args[1..])),
        _ => {
            eprintln!("使い方: pnpm threads list | show | fixed <番号|URL> <要約> | rejected <番号|URL> <理由> | forget <番号|URL> | apply");
            2
        }
    };
    std::process::exit(code);
}

fn report(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[review-threads] {e}");
            1
        }
    }
}

/// いまのブランチ (jj の bookmark) の、開いている PR を探す。
fn current_pr(gh: &dyn Gh) -> Result<Pr, String> {
    let repo = gh::repo(gh)?;
    for branch in lib_jj_helpers::get_jj_bookmarks(StderrMode::Silent, None) {
        if let Some(number) = gh::open_pr_for_branch(gh, &repo, &branch)? {
            return Ok(Pr { repo, branch, number });
        }
    }
    Err("いまのブランチに開いている PR が無い".to_string())
}

/// 読むだけのサブコマンド (`list` / `show`) の PR。`--pr <番号>` を付ければ、いまのブランチでない PR
/// (閉じた PR も) を読む。登録には使わない。
fn pr_to_read(gh: &dyn Gh, args: &[String]) -> Result<Pr, String> {
    match args {
        [flag, n] if flag == "--pr" => {
            let number = n.parse::<u64>().map_err(|_| format!("PR 番号ではない: {n}"))?;
            Ok(Pr { repo: gh::repo(gh)?, branch: String::new(), number })
        }
        [] => current_pr(gh),
        _ => Err("引数は `--pr <番号>` だけ".to_string()),
    }
}

/// すべてのスレッド (閉じたものも) の状態と返信を出す。push の後に、返信と resolve が
/// 行われたかを確かめるのに使う。`--full` を付ければ、返信の本文 (折りたたみ部分を除く) も出す
/// (レビュアーが修正を認めたかを確かめるため)。
fn run_show(gh: &dyn Gh, args: &[String]) -> Result<(), String> {
    let full = args.iter().any(|a| a == "--full");
    let rest: Vec<String> = args.iter().filter(|a| *a != "--full").cloned().collect();
    let pr = pr_to_read(gh, &rest)?;
    let all = threads::fetch(gh, &pr.repo, pr.number)?.threads;
    let open = threads::unresolved(&all).len();
    println!("PR #{} のスレッド: {} 件 (未解決 {open} 件)", pr.number, all.len());
    for t in &all {
        let state = if t.resolved { "解決済み" } else { "未解決" };
        let line = t.line.map(|l| format!(":{l}")).unwrap_or_default();
        println!("[{state}] {}{line}  {}", t.path, t.title);
        for r in &t.replies {
            println!("    返信 ({}): {}", r.author, threads::title_of(&r.body));
            if full {
                for text in threads::readable_lines(&r.body) {
                    println!("      {text}");
                }
            }
        }
        if t.omitted_replies > 0 {
            println!("    (ほかに返信が {} 件あるが、取得の上限を超えたので出していない: {})", t.omitted_replies, t.url);
        }
    }
    Ok(())
}

fn run_list(gh: &dyn Gh, args: &[String]) -> Result<(), String> {
    let pr = pr_to_read(gh, args)?;
    let all = threads::fetch(gh, &pr.repo, pr.number)?.threads;
    let entries = pending::load(&pending::default_path()?)?;
    let open = threads::unresolved(&all);
    println!("PR #{} の未解決スレッド: {} 件", pr.number, open.len());
    for (i, t) in open.iter().enumerate() {
        let mark = match entries.iter().find(|e| e.thread_id == t.id).map(|e| e.kind) {
            Some(Kind::Fixed) => " [修正で登録済み]",
            Some(Kind::Rejected) => " [棄却で登録済み]",
            None => "",
        };
        let line = t.line.map(|l| format!(":{l}")).unwrap_or_default();
        println!("#{}{mark} {}{line}  {}\n    {}", i + 1, t.path, t.title, t.url);
    }
    Ok(())
}

fn run_register(gh: &dyn Gh, kind: Kind, args: &[String]) -> Result<(), String> {
    let (reference, note) = args.split_first().ok_or("登録するスレッドの番号か URL がない")?;
    let note = note.join(" ");
    if note.trim().is_empty() {
        return Err("返信に書く要約 (棄却なら理由) がない".to_string());
    }
    let pr = current_pr(gh)?;
    let all = threads::fetch(gh, &pr.repo, pr.number)?.threads;
    let thread = threads::find(&all, reference)?;
    let path = pending::default_path()?;
    let mut entries = pending::load(&path)?;
    pending::upsert(&mut entries, new_entry(&pr, thread, kind, note));
    pending::save(&path, &entries)?;
    println!("登録した ({} 件): {} ({})", entries.len(), thread.title, thread.path);
    Ok(())
}

fn new_entry(pr: &Pr, thread: &Thread, kind: Kind, note: String) -> Entry {
    Entry {
        repo: pr.repo.clone(),
        pr: pr.number,
        branch: pr.branch.clone(),
        thread_id: thread.id.clone(),
        comment_id: thread.comment_id,
        path: thread.path.clone(),
        title: thread.title.clone(),
        kind,
        note,
        registered_at: lib_pending_file::utc_now_iso8601(),
        replied: false,
        last_error: None,
    }
}

fn run_forget(gh: &dyn Gh, args: &[String]) -> Result<(), String> {
    let reference = args.first().ok_or("取り消すスレッドの番号か URL がない")?;
    let pr = current_pr(gh)?;
    let all = threads::fetch(gh, &pr.repo, pr.number)?.threads;
    let thread = threads::find(&all, reference)?;
    let path = pending::default_path()?;
    let mut entries = pending::load(&path)?;
    let before = entries.len();
    let replied = entries.iter().any(|e| e.thread_id == thread.id && e.replied);
    entries.retain(|e| e.thread_id != thread.id);
    pending::save(&path, &entries)?;
    println!("取り消した: {} 件", before - entries.len());
    if replied {
        println!("このスレッドには返信が済んでいます。登録し直すと、同じスレッドにもう一度返信します。");
    }
    Ok(())
}

/// push の後に呼ぶ。何があっても 0 で終わる (push を失敗扱いにしない)。
fn run_apply(gh: &dyn Gh) -> i32 {
    if let Err(e) = pending::default_path().and_then(|path| apply_at(gh, &path)) {
        println!("[review-threads] スレッドの返信と resolve を行わなかった: {e}");
    }
    0
}

fn apply_at(gh: &dyn Gh, path: &Path) -> Result<(), String> {
    let entries = pending::load(path)?;
    let pr = current_pr(gh)?;
    let fetched = threads::fetch(gh, &pr.repo, pr.number)?;
    if entries.is_empty() {
        // NOTE: 何も書いていないので、読んだスレッドがそのまま最新 (読み直さない)。
        notify_unregistered(&fetched.threads, &entries);
        return Ok(());
    }
    let head = gh::head_commit(gh, &pr.repo, pr.number)?;
    let target = apply::Target { repo: &pr.repo, pr: pr.number, head: &head, all_read: fetched.unreadable == 0 };
    let outcome = apply::apply(gh, &target, entries, &fetched.threads);
    for line in &outcome.report {
        println!("[review-threads] {line}");
    }
    println!("[review-threads] PR #{}: {} 件を返信して閉じた", pr.number, outcome.closed);
    // NOTE: ここから先の失敗は「行わなかった」ではない。返信と resolve は済んでいる (pre-push review の指摘)。
    if let Err(e) = pending::save(path, &outcome.kept) {
        println!(
            "[review-threads] 返信と resolve は済んだが、結果を登録ファイルに書けなかった: {e}。\
             次の push で同じ返信をもう一度書くおそれがあるので、{} を確かめてください",
            path.display()
        );
        return Ok(());
    }
    match threads::fetch(gh, &pr.repo, pr.number) {
        Ok(fresh) => notify_unregistered(&fresh.threads, &outcome.kept),
        Err(e) => println!("[review-threads] 登録の無いスレッドの案内を出すための読み直しに失敗した: {e}"),
    }
    Ok(())
}

fn notify_unregistered(threads: &[Thread], entries: &[Entry]) {
    if let Some(notice) = apply::unregistered_notice(threads, entries) {
        println!("[review-threads] {notice}");
    }
}

#[cfg(test)]
mod tests;
