//! `git`/`gh`/`cargo`/`ruff`/`pytest`/`pnpm`/`npm` ― サブコマンドで削除を行う開発ツール CLI の
//! 引数解析。[`super::deletions`] から分けた (ADR-080、800 行ガイドライン) — 構造の移動と
//! visibility 調整のみで、挙動は変えていない (ADR-080 不変条件 1)。

use super::super::lexer::{Piece, Word};
use super::deletions::{plain, posix_rm, unknown_deletion, Deletion};
use super::State;

/// `git rm` / `git clean` ― git 自身が持つ実削除 subcommand。先頭の非オプション語を subcommand
/// とみなし、それが `rm`/`clean` のときだけ対象にする。`git log --grep rm` のように `rm`/`clean`
/// が別 subcommand の引数に現れるだけのケースは、先頭語 (`log`) が一致しないので対象にならない
/// (SIM-NEW-deletions-rs-L57)。subcommand の前の値を取るグローバルオプション (`-C <dir>` / `-c k=v`)
/// は値ごと読み飛ばす。`-C` / `--git-dir` / `--work-tree` は対象の基準の場所を変えるので、
/// 対象を判定できないものとする。ディスクから消さない `git rm --cached` と、dry-run の `-n` は対象外。
pub(super) fn git_deletion(args: &[Word]) -> Option<Deletion> {
    let (i, moved) = git_subcommand(args);
    let rest = args.get(i + 1..).unwrap_or_default();
    let has = |flag: &str, short: Option<char>| {
        rest.iter().map(Word::leading_lit).any(|l| {
            l == flag || short.is_some_and(|s| l.starts_with('-') && !l.starts_with("--") && l.contains(s))
        })
    };
    let mut found = match args.get(i)?.leading_lit() {
        "rm" if !has("--cached", None) && !has("--dry-run", Some('n')) => posix_rm(rest),
        "clean" if !has("--dry-run", Some('n')) => git_clean(rest),
        "worktree" if rest.first().map(Word::leading_lit) == Some("remove") => plain(&rest[1..], true),
        _ => return None,
    };
    if moved {
        for target in &mut found.targets {
            target.pieces.insert(0, Piece::Opaque);
        }
    }
    Some(found)
}

/// git の subcommand の位置と、`-C` / `--git-dir` / `--work-tree` で基準の場所が変わるか。
/// [`super::outputs::git_output`] も使うため `pub(super)` ([`super`] = `commands`)。
pub(super) fn git_subcommand(args: &[Word]) -> (usize, bool) {
    let mut i = 0;
    let mut moved = false;
    while let Some(lead) = args.get(i).map(Word::leading_lit).filter(|l| l.starts_with('-')) {
        moved |= lead == "-C" || ["--git-dir", "--work-tree"].iter().any(|o| lead.starts_with(o));
        i += if matches!(lead, "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace") { 2 } else { 1 };
    }
    (i, moved)
}

/// `git clean` の引数。パス指定が無ければ cwd 全体が対象になるため `.` を対象として積む
/// ([`super::find::posix_find`] の対象省略時の扱いと同じ考え方)。ディレクトリかどうかに関わらず
/// 作業木全体を走査して消すコマンドなので、常に recursive として扱う (安全側 — fail-closed)。
fn git_clean(args: &[Word]) -> Deletion {
    let mut targets: Vec<Word> = args.iter().filter(|w| !w.leading_lit().starts_with('-')).cloned().collect();
    if targets.is_empty() {
        targets.push(Word::lit("."));
    }
    Deletion { targets, recursive: true }
}

/// `gh repo delete` / `gh release delete` / `gh secret delete` / `gh variable delete` /
/// `gh run delete` / `gh cache delete` / `gh ssh-key delete` / `gh gpg-key delete` /
/// `gh codespace delete` など ― gh CLI は削除系サブコマンドを `<noun> delete` の形に統一している
/// (SIM-NEW-allowlist-rs-L28: `gh` が `DEVELOPMENT_TOOLS` の確認なし allowlist に入っているのに
/// `deletion()` に `gh` 用の経路が無く、削除系サブコマンドが未検査で通過していた)。消す対象は
/// GitHub 上のリソース (リポジトリ本体・リリース・シークレット等) でローカルのファイルパスを
/// 持たないため、場所を判定できないものとして fail-closed に倒す (ADR-043)。`-R`/`--repo` の
/// ようなグローバルオプションは値ごと読み飛ばす。2 番目の位置引数は `delete` 完全一致だけでなく
/// `delete-` 始まりも対象にする ― `gh release delete-asset <tag> <asset>` のように `delete` を
/// 接頭辞に持つだけの別サブコマンドが完全一致判定を素通りしていた (SEC-NEW-deletions-rs-L262)。
pub(super) fn gh_deletion(args: &[Word]) -> Option<Deletion> {
    let mut i = 0;
    while let Some(lead) = args.get(i).map(Word::leading_lit) {
        if lead == "-R" || lead == "--repo" {
            i += 2;
        } else if lead.starts_with('-') {
            i += 1;
        } else {
            break;
        }
    }
    let rest = &args[i..];
    let positionals = gh_positional_words(rest);
    let deletes = match positionals.as_slice() {
        ["api", ..] => gh_api_deletes(rest),
        _ => gh_command_deletes(&positionals, rest),
    };
    deletes.then(unknown_deletion)
}

/// `rest` からオプションでない語だけを拾う。オプションの直後の語は、値として読み飛ばす ―
/// 読み飛ばさないと `gh pr create --title fix-delete-stale-branches` や
/// `gh pr edit --add-label delete-candidate` のように、`-t`/`--title`/`-l`/`--add-label` 等の
/// 値が `delete` を含むだけで [`gh_command_deletes`] の動詞判定に紛れ込み、無関係な呼び出しを
/// 無条件 block していた (SIM-NEW-deletions-rs-L291)。次の語がさらにオプション (`-` 始まり) なら
/// 値ではなく次のオプションなので読み飛ばさない (`--squash --delete-branch` の `--delete-branch` を
/// 値として吸収してしまうと、option 側の `delete` 判定に渡る前に消えてしまう)。`--opt=value` の
/// ように値が同じ語に埋め込まれている形は、その場で判定が終わっているので次の語は読み飛ばさない。
fn gh_positional_words(args: &[Word]) -> Vec<&str> {
    let mut words = Vec::new();
    let mut expect_value = false;
    for word in args {
        let lead = word.leading_lit();
        if expect_value {
            expect_value = false;
            if !lead.starts_with('-') {
                continue;
            }
        }
        if lead.starts_with('-') {
            expect_value = !lead.contains('=');
            continue;
        }
        words.push(lead);
    }
    words
}

/// gh の subcommand の並び (最大 3 段: `gh repo deploy-key delete`) に削除の動詞があるか
/// (`delete` / `delete-asset` / `item-delete` / `remove`)。名前に `delete` を含むオプション
/// (`gh pr merge --delete-branch` / `gh issue comment --delete-last`) も削除とする。動詞を完全一致で
/// 見ていたため `gh release delete-asset` が通っていた (SEC-NEW-deletions-rs-L262)。
fn gh_command_deletes(positionals: &[&str], args: &[Word]) -> bool {
    let is_verb = |w: &&str| w.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let verb_deletes = positionals
        .iter()
        .copied()
        .skip(1)
        .take(2)
        .filter(is_verb)
        .any(|w| w.contains("delete") || w == "remove");
    let option_deletes = args.iter().map(Word::leading_lit).any(|l| l.starts_with("--") && l.contains("delete"));
    verb_deletes || option_deletes
}

/// `gh api -X DELETE …` / `--method DELETE` / `-XDELETE` / `--method=DELETE`。API を直接呼んで消す。
fn gh_api_deletes(args: &[Word]) -> bool {
    args.iter().enumerate().any(|(i, w)| {
        let lead = w.leading_lit();
        let value = match lead {
            "-X" | "--method" => args.get(i + 1).map(Word::leading_lit),
            _ => lead.strip_prefix("--method=").or_else(|| lead.strip_prefix("-X")),
        };
        value.is_some_and(|v| v.eq_ignore_ascii_case("delete"))
    })
}

/// `cargo clean` と `cargo uninstall`。`clean` は target ディレクトリを再帰で消す。消す場所は
/// `--target-dir`、`CARGO_TARGET_DIR` ([`State::cargo_target_dir`])、既定の `target` の順に決める。
/// `--manifest-path` で別のプロジェクトを指すと場所が分からない。`uninstall` はプロジェクトの外
/// (`~/.cargo/bin`) を消す (SEC-NEW-deletions-rs-L73)。
pub(super) fn cargo_deletion(args: &[Word], state: Option<&State>) -> Option<Deletion> {
    let subcommand = cargo_subcommand(args)?;
    match subcommand {
        "uninstall" => return Some(unknown_deletion()),
        "clean" => {}
        _ => return None,
    }
    let relocated = args.iter().map(Word::leading_lit).any(|l| l.starts_with("--config") || l.starts_with("-C"));
    if relocated {
        return Some(unknown_deletion());
    }
    if flag_value(args, "--manifest-path").is_some() {
        return Some(unknown_deletion());
    }
    let target = flag_value(args, "--target-dir")
        .or_else(|| state.and_then(State::cargo_target_dir))
        .unwrap_or_else(|| Word::lit("target"));
    Some(Deletion { targets: vec![target], recursive: true })
}

/// cargo の subcommand。値を取るグローバルオプション (`--config` / `-Z` / `-C` / `--color`) は値ごと
/// 読み飛ばす。読み飛ばさないと値 (`build.target-dir=/outside`) を subcommand と読む (CodeRabbit の指摘)。
fn cargo_subcommand(args: &[Word]) -> Option<&str> {
    let mut i = 0;
    while let Some(lead) = args.get(i).map(Word::leading_lit) {
        if !lead.starts_with(['-', '+']) {
            return Some(lead);
        }
        i += if matches!(lead, "--config" | "-Z" | "-C" | "--color") { 2 } else { 1 };
    }
    None
}

/// `--flag value` / `--flag=value` の値。[`cargo_deletion`]/[`ruff_deletion`] で共通に使う
/// (ADR-080 — ロジックの重複を避ける)。
fn flag_value(args: &[Word], flag: &str) -> Option<Word> {
    args.iter().enumerate().find_map(|(i, w)| {
        let lead = w.leading_lit();
        match lead.strip_prefix(flag) {
            Some(rest) if rest.starts_with('=') => Some(w.with_head(&rest[1..])),
            Some("") => args.get(i + 1).cloned(),
            _ => None,
        }
    })
}

/// `ruff clean`。ruff の lint/format キャッシュ (既定 `.ruff_cache`、カレントディレクトリ基準) を
/// 再帰で消す。保存場所は `--cache-dir` で変えられる。`--config` で別の pyproject.toml を指すと、
/// そちらの `cache-dir` 設定でも場所が変わり得るため場所を判定できないものとする (cargo clean の
/// `--manifest-path` と同じ考え方)。`allowed_commands` に `ruff` を足しても `deletion()` に対応する
/// 分岐が無く、プロジェクト内再帰削除が無確認で通っていた (SEC-NEW-hooks-config-python-toml-L20)。
pub(super) fn ruff_deletion(args: &[Word]) -> Option<Deletion> {
    if ruff_subcommand(args)? != "clean" {
        return None;
    }
    if args.iter().map(Word::leading_lit).any(|l| l == "--config" || l.starts_with("--config=")) {
        return Some(unknown_deletion());
    }
    let target = flag_value(args, "--cache-dir").unwrap_or_else(|| Word::lit(".ruff_cache"));
    Some(Deletion { targets: vec![target], recursive: true })
}

/// ruff の subcommand。値を取るグローバルオプション (`--cache-dir` / `--config`) は値ごと読み飛ばす。
fn ruff_subcommand(args: &[Word]) -> Option<&str> {
    let mut i = 0;
    while let Some(lead) = args.get(i).map(Word::leading_lit) {
        if !lead.starts_with('-') {
            return Some(lead);
        }
        i += if matches!(lead, "--cache-dir" | "--config") { 2 } else { 1 };
    }
    None
}

/// `pytest --cache-clear`。pytest のキャッシュ (既定 `.pytest_cache`、rootdir 基準) を再帰で消す。
/// `--rootdir` を指定すると相対パスの基準自体が変わり、この関数ではカレントディレクトリ基準でしか
/// 判定できないため、場所を判定できないものとする。`-o cache_dir=...` / `--override-ini
/// cache_dir=...` で保存場所そのものを変える指定があれば、その値を対象にする。`allowed_commands` に
/// `pytest` を足しても `deletion()` に対応する分岐が無く、プロジェクト内再帰削除が無確認で通っていた
/// (SEC-NEW-hooks-config-python-toml-L20)。
pub(super) fn pytest_deletion(args: &[Word]) -> Option<Deletion> {
    if !args.iter().map(Word::leading_lit).any(|l| l == "--cache-clear") {
        return None;
    }
    if args.iter().map(Word::leading_lit).any(|l| l == "--rootdir" || l.starts_with("--rootdir=")) {
        return Some(unknown_deletion());
    }
    let override_ini = args.iter().enumerate().find_map(|(i, w)| {
        let lead = w.leading_lit();
        match lead {
            "-o" | "--override-ini" => args.get(i + 1).map(Word::leading_lit),
            _ => lead.strip_prefix("--override-ini="),
        }
    });
    let cache_dir_override = override_ini.and_then(|v| v.strip_prefix("cache_dir="));
    let target = cache_dir_override.map(Word::lit).unwrap_or_else(|| Word::lit(".pytest_cache"));
    Some(Deletion { targets: vec![target], recursive: true })
}

/// `pnpm store prune` / `pnpm cache delete` と `npm cache clean`、グローバルのアンインストール
/// (`npm uninstall -g` / `pnpm remove -g`)。どれもプロジェクトの外を消すので、場所を判定できないものとする。
pub(super) fn package_cache_deletion(name: &str, args: &[Word]) -> Option<Deletion> {
    let global = args.iter().map(Word::leading_lit).any(|l| l == "-g" || l == "--global");
    let positionals: Vec<&str> = args.iter().map(Word::leading_lit).filter(|l| !l.starts_with('-')).collect();
    let deletes = match (name, positionals.as_slice()) {
        ("pnpm", ["store", "prune", ..]) => true,
        ("pnpm" | "npm", ["cache", sub, ..]) => matches!(*sub, "clean" | "clear" | "rm" | "delete" | "verify"),
        (_, [verb, ..]) if global => matches!(*verb, "uninstall" | "remove" | "rm" | "un" | "r" | "unlink"),
        _ => false,
    };
    deletes.then(unknown_deletion)
}
