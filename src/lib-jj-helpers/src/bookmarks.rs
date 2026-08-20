//! bookmark 探索 — trunk 判定 / revset 走査 / ローカル・リモート追跡 bookmark の取得。
//!
//! ADR-021 原則 5 (bookmark 検出) と ADR-024 (共有ヘルパー) の実装本体。
//! 副作用 (jj subprocess・ログ) は呼び出し側から注入する方針は crate doc を参照。

use std::process::{Command, Stdio};

/// PR / bookmark 検出から除外する trunk 系 bookmark 名。
pub const TRUNK_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop"];

/// `TRUNK_BOOKMARKS` に含まれる名前であれば `true`。
pub fn is_trunk_bookmark(name: &str) -> bool {
    TRUNK_BOOKMARKS.contains(&name)
}

/// **ローカル** bookmark 検索に使用する revset のリスト (近い順 = 優先順)。
///
/// [`select_from_revsets`] は先頭から順に試し、最初に (trunk 除外後の)
/// bookmark が見つかった時点で後続の revset を検索しない。
///
/// - `@`: 標準運用 (bookmark が現在のコミット上)。共通ケースを深い revset に触れず
///   即決させるため、および [`select_from_revsets`] の fallback_log (先頭以外で hit
///   したら通知) の意味を保つために先頭へ残す
/// - `heads(::@ & bookmarks())`: **@ から祖先方向で最も近い bookmark 付きコミット**。
///   旧構成 (`["@", "@-", "@--"]`) は 3 段しか遡らず、監視・自動 fix 経路が積む
///   説明なし空コミットで bookmark が範囲外に出ると検出不能になった (順位 386、
///   計 9 回実観測。使い捨て jj リポジトリの実測では監視サイクルごとに +1 段ずつ
///   深くなり、3 サイクル目から検出不能)。深さ非依存 revset は同じ実測で境界
///   5 ケース (@ 直上 / @- / 子 bookmark / 同一コミット複数 / 祖先 2 段) すべてで
///   旧構成と同等以上に解決した (jj 0.42 実機確認)
///
/// ## 適用範囲の境界 (PR #271 の決定との関係)
///
/// この深さ非依存 revset は **読み取り専用の検出 (PR 検索 / --head 解決)** 専用。
/// push 対象の bookmark 選定 (push-runner の `-b` 付与) は PR #271 の決定どおり
/// `@` 厳密一致のまま — `::@` は他 workspace が作った未マージコミット上の bookmark を
/// 含みうるため、**書き込み対象の所有権判定には使わない** (ADR-045 の並列 workspace
/// 運用)。読み取り側で他 workspace の bookmark を拾った場合の逃げ道は
/// `--pr <番号>` (順位 397)。
///
/// ## 既知の限界 (旧構成にあった挙動の変化)
///
/// `heads()` は「bookmark 付きコミットの集合の先端」を返すため、trunk 系 bookmark の
/// コミットが feature bookmark より @ 側にあると、その祖先の feature bookmark は
/// 影に入って返らない (trunk 名は [`parse_bookmark_list_output`] が名前で除外するが、
/// heads の計算はその前段)。squash マージ運用ではマージ済み feature bookmark が
/// trunk の祖先に来ることは無いため実運用では発生しない構成であり、発生しても
/// `--pr` で回避できる。
pub const BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "heads(::@ & bookmarks())"];

/// **リモート追跡** bookmark 検索に使用する revset のリスト。
///
/// [`BOOKMARK_SEARCH_REVSETS`] のリモート版。深い側の revset は
/// `remote_bookmarks()` でフィルタする — ローカル版の `bookmarks()` を流用すると
/// 「ローカル bookmark を持つコミット」しか候補にならず、リモート専用 bookmark
/// (bot が remote に作った PR head、順位 397 の実測) のコミットが**原理的に候補から
/// 外れる**。旧構成は位置指定 (`@-` 等) で両用できたが、bookmark フィルタを含む
/// revset はローカル / リモートで分ける必要がある。
pub const REMOTE_BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "heads(::@ & remote_bookmarks())"];

/// jj サブプロセスの stderr ハンドリング方針。
///
/// 失敗時の jj stderr (不正な revset 指定や jj 非互換テンプレート等) を
/// どう扱うかを呼び出し側が選ぶ。
pub enum StderrMode {
    /// stderr を捨てる (`Stdio::null`)。CI ログを汚したくない場合。
    Silent,
    /// stderr を捕捉し、非空であれば引数のログ関数に渡す。
    Piped(fn(&str)),
}

/// `jj log` テンプレート出力 (カンマ区切り × 行) からユニークな bookmark 名を抽出する。
/// trunk 系 bookmark は除外する。
///
/// 想定テンプレート: `local_bookmarks.map(|b| b.name()).join(",") ++ "\n"`
pub fn parse_bookmark_list_output(raw: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for line in raw.lines() {
        for name in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if is_trunk_bookmark(name) {
                continue;
            }
            let name = name.to_string();
            if !seen.contains(&name) {
                seen.push(name);
            }
        }
    }
    seen
}

/// 指定 revset の**ローカル** bookmark 名を `jj log` で取得する (I/O)。
///
/// `stderr_mode` で stderr の扱いを指定する。
/// revset 不正や jj テンプレート非互換等の失敗時は空 Vec を返す。
pub fn query_bookmarks_at(revset: &str, stderr_mode: &StderrMode) -> Vec<String> {
    query_bookmarks_with_template(
        revset,
        "local_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        stderr_mode,
    )
}

/// 指定 revset の**リモート追跡** bookmark 名を `jj log` で取得する (I/O)。
///
/// テンプレートの `b.name()` は remote 名を含まない bare な bookmark 名を返すため
/// (`claude/nightly-163@origin` → `claude/nightly-163`)、そのまま `gh pr list --head`
/// や `git` の branch 名として使える (jj 0.42.0 で実機確認)。
///
/// colocated リポジトリでは、ローカル bookmark の `@git` 複製も同じ名前で列挙される。
/// 重複は [`parse_bookmark_list_output`] が畳み込むため呼び出し側の考慮は不要。
pub fn query_remote_bookmarks_at(revset: &str, stderr_mode: &StderrMode) -> Vec<String> {
    query_bookmarks_with_template(
        revset,
        "remote_bookmarks.map(|b| b.name()).join(\",\") ++ \"\\n\"",
        stderr_mode,
    )
}

/// `jj log -T <template>` を実行して bookmark 名リストを得る共通処理。
fn query_bookmarks_with_template(
    revset: &str,
    template: &str,
    stderr_mode: &StderrMode,
) -> Vec<String> {
    let mut cmd = Command::new("jj");
    cmd.args(["log", "-r", revset, "--no-graph", "-T", template])
        .stdout(Stdio::piped());

    cmd.stderr(match stderr_mode {
        StderrMode::Silent => Stdio::null(),
        StderrMode::Piped(_) => Stdio::piped(),
    });

    let output = match cmd.output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            if let StderrMode::Piped(log) = stderr_mode {
                let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                if !stderr.is_empty() {
                    log(&format!(
                        "jj bookmark 取得失敗 (revset={}): {}",
                        revset, stderr
                    ));
                }
            }
            return Vec::new();
        }
        Err(e) => {
            if let StderrMode::Piped(log) = stderr_mode {
                log(&format!("jj コマンド実行失敗: {}", e));
            }
            return Vec::new();
        }
    };

    parse_bookmark_list_output(&String::from_utf8_lossy(&output.stdout))
}

/// 指定 revset を優先順に試し、最初に非空の bookmark リストを得た revset の結果を返す。
///
/// `fallback_log` を渡すと、先頭以外の revset で bookmark が検出された場合に
/// "revset '@-' で bookmark を検出: [...]" 形式のメッセージを記録する。
///
/// テスト用に `query` をクロージャで注入できる pure function。
pub fn select_from_revsets<F>(
    revsets: &[&str],
    query: F,
    fallback_log: Option<fn(&str)>,
) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    for (i, revset) in revsets.iter().enumerate() {
        let bookmarks = query(revset);
        if !bookmarks.is_empty() {
            if i > 0 {
                if let Some(log) = fallback_log {
                    log(&format!(
                        "revset '{}' で bookmark を検出: {:?}",
                        revset, bookmarks
                    ));
                }
            }
            return bookmarks;
        }
    }
    Vec::new()
}

/// bookmark 前進 (advance) の移動先: **@ から祖先方向で最も近い、説明のある
/// コミット** (順位 386 症状 2)。
///
/// 旧実装 (「@ が非空なら @、空なら @-」) は **description の有無を見ない**ため、
/// 監視・自動 fix 経路が積んだ説明なしコミットへ bookmark を移し、push が
/// `Won't push commit ... since it has no description` で落ちた (#370 実観測)。
/// jj が push を拒否する条件は description なので、移動先も description で選ぶ。
///
/// - @ に説明があれば従来どおり @ (共通ケースは挙動不変)
/// - @ が説明なしなら、説明なしコミットを何段でも飛ばして直近の説明ありコミットへ
/// - 説明ありの祖先が無い (root まで説明なし) なら候補 0 件 → advance skip
///
/// jj 0.42 実機確認: 線形チェーンで説明なし 3 段 + dirty WC の構成から
/// ちょうど 1 件 (最近傍の説明ありコミット) を返す。マージ祖先があると複数件を
/// 返すため、呼び出し側 ([`classify_advance_target`]) は複数を ambiguous として
/// skip に倒す (旧実装も複数親の `@-` では `jj bookmark set` が解決失敗して
/// skip だった — 同じ安全側)。
pub const ADVANCE_TARGET_REVSET: &str = "heads(::@ ~ description(exact:\"\"))";

/// [`resolve_advance_target`] の結果。
#[derive(Debug, PartialEq, Eq)]
pub enum AdvanceTarget {
    /// 一意に解決した。commit id を `jj bookmark set -r` に渡せる。
    Commit(String),
    /// マージ祖先等で候補が複数。bookmark をどれに移すべきか決められないため
    /// advance を skip する (呼び出し側で警告ログ)。
    Ambiguous(Vec<String>),
    /// 説明ありコミットが @ の祖先に無い (root まで説明なし)。advance 対象なし。
    None,
}

/// 候補 commit id リストを [`AdvanceTarget`] に分類する pure function。
pub fn classify_advance_target(mut ids: Vec<String>) -> AdvanceTarget {
    match ids.len() {
        0 => AdvanceTarget::None,
        1 => AdvanceTarget::Commit(ids.remove(0)),
        _ => AdvanceTarget::Ambiguous(ids),
    }
}

/// [`BOOKMARK_SEARCH_REVSETS`] を優先順に走査し、最初に見つかった
/// (trunk 除外後の) bookmark を返す。
///
/// - `stderr_mode`: `jj log` の stderr ハンドリング方針
/// - `fallback_log`: `@` 以外の revset で hit した場合の通知 (`None` なら無通知)
pub fn get_jj_bookmarks(stderr_mode: StderrMode, fallback_log: Option<fn(&str)>) -> Vec<String> {
    select_from_revsets(
        BOOKMARK_SEARCH_REVSETS,
        |r| query_bookmarks_at(r, &stderr_mode),
        fallback_log,
    )
}

/// [`get_jj_bookmarks_with_remote_fallback`] の結果。空 Vec を持つ variant は返らない。
#[derive(Debug, PartialEq, Eq)]
pub enum BookmarkSearch {
    /// ローカル bookmark で解決した (従来と同じ経路)。
    Local(Vec<String>),
    /// ローカル bookmark が無く、リモート追跡 bookmark で解決した。
    /// `jj bookmark track` されていない = ローカルに実体が無い状態なので、
    /// 呼び出し側が bookmark をローカル操作 (`jj bookmark set` 等) の対象にするのは誤り。
    RemoteOnly(Vec<String>),
    /// どちらにも bookmark が無い。
    NotFound,
}

/// ローカル bookmark を優先し、無ければリモート追跡 bookmark へフォールバックして探索する。
///
/// bot が remote に作った PR を人間がマージする経路 (ADR-072 の夜間ループ) では、
/// PR の head が `claude/nightly-163@origin` のようなリモート専用 bookmark しか持たず、
/// [`get_jj_bookmarks`] (ローカルのみ) では検出できない (順位 397 で実測)。
///
/// **ローカルを先に全 revset 走査してからリモートへ移る**二段構成にしてあり、
/// ローカル bookmark が 1 つでも見つかる状況では [`get_jj_bookmarks`] と結果が一致する
/// (共有ライブラリの既存呼び出し側 = push-runner / pr-monitor への回帰を避けるため、
/// ADR-024)。読み取り専用の PR 検出用途を想定した API で、bookmark を書き換える経路は
/// `RemoteOnly` を区別して扱うこと。
pub fn get_jj_bookmarks_with_remote_fallback(
    stderr_mode: StderrMode,
    fallback_log: Option<fn(&str)>,
) -> BookmarkSearch {
    select_with_remote_fallback(
        BOOKMARK_SEARCH_REVSETS,
        REMOTE_BOOKMARK_SEARCH_REVSETS,
        |r| query_bookmarks_at(r, &stderr_mode),
        |r| query_remote_bookmarks_at(r, &stderr_mode),
        fallback_log,
    )
}

/// [`get_jj_bookmarks_with_remote_fallback`] の探索順序を、注入した query で検証可能にした pure function。
///
/// `local` / `remote` はそれぞれ revset を受け取り bookmark 名を返す。
/// ローカル / リモートで revset リストを分けて受ける (順位 386)。
/// 深い側の revset が bookmark フィルタ (`bookmarks()` / `remote_bookmarks()`) を
/// 含むようになったため、1 本のリストを両用できない — 詳細は
/// [`REMOTE_BOOKMARK_SEARCH_REVSETS`] の doc を参照。
pub fn select_with_remote_fallback<L, R>(
    local_revsets: &[&str],
    remote_revsets: &[&str],
    local: L,
    remote: R,
    fallback_log: Option<fn(&str)>,
) -> BookmarkSearch
where
    L: Fn(&str) -> Vec<String>,
    R: Fn(&str) -> Vec<String>,
{
    let found = select_from_revsets(local_revsets, local, fallback_log);
    if !found.is_empty() {
        return BookmarkSearch::Local(found);
    }
    let found = select_from_revsets(remote_revsets, remote, fallback_log);
    if found.is_empty() {
        BookmarkSearch::NotFound
    } else {
        BookmarkSearch::RemoteOnly(found)
    }
}

// test module は別ファイルへ分離している (本体 800 行ガイドライン、順位 147)。
// 分割方式は pipeline_lock.rs 等と同じ `#[path]` 方式に揃えた。
#[cfg(test)]
#[path = "bookmarks/tests.rs"]
mod tests;
