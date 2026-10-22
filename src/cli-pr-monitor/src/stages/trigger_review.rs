//! PR 作成後に CodeRabbit のレビューを発火させる (順位 321)。
//!
//! # なぜ要るのか
//!
//! CodeRabbit は 2026-09-08 に **star 10 未満のリポジトリへの自動レビューを停止**した。
//! それまでは `.coderabbit.yaml` の `auto_review.enabled: true` で初回 PR は自動レビュー
//! され、fix push だけ手動トリガーを要する設計だった (ADR-019 § WP-03)。
//!
//! **2026-09-10 頃に auto が戻った** (順位 520)。bot 作成 PR でも PR 作成の 2〜8 秒後に
//! 自発 walkthrough が出る (#494 / #502 / #507 / #510 の初動で実測、4/4)。よって本 module は
//! **「トリガーが要るかどうかを毎回実測する」** 形に倒してある — auto が効いていれば何もせず
//! 戻り、効いていなければ従来どおりチェックを入れる。CodeRabbit の挙動は短期間で何度も
//! 変わるため、どちらか一方の挙動を前提に固定しない。
//!
//! auto が効かないとき、CodeRabbit は PR へ次の形のコメントを出す。チェックを入れるとレビューが始まる:
//!
//! ```text
//! > - [ ] <!-- {"checkboxId":"..."} --> 🔍 Trigger review
//! >
//! > This repository does not receive automatic reviews because it has fewer than 10 stars.
//! ```
//!
//! **2026-10-21 に再び止まり、通知が遅れて届くようになった** (順位 584)。#585 では PR 作成の
//! 16 分後にスキップ通知が付き、ポーリング (75 秒) の間は CodeRabbit が何も言わなかった。
//! そこで、ポーリングの間ずっと黙っていたときは **`@coderabbitai review` を人間名義で投稿する**
//! ([`request_review_by_comment`])。手動の依頼は通知を待たずに受け付けられる (#585 で実測:
//! 投稿の 11 分後に受け付け、18 分後にレビュー完了)。上限は 1 時間に 1 回 (同レビューの告知)。
//!
//! **チェックボックスは素の markdown タスクリスト**で、GitHub UI の操作実体は「コメント
//! 本文の `- [ ]` を `- [x]` に書き換える」だけである。API から同じ編集をしても CodeRabbit
//! は区別しない (2026-09-08 に PR #489 で実測: PATCH の 1 分 23 秒後にレビュー開始)。
//!
//! # 1 PR につき 1 回だけ
//!
//! **push ごとには発火させない。** CodeRabbit はレビュー回数に上限を持ち (告知は可変で、
//! 実測では 30 分 / 27 分 / 56 分)、超えると `Review limit reached` で拒否される。
//! 枠は「1 度レビューを受けたらマージする」運用に充てる方が価値が高い。再 push 後に
//! 改めてレビューが要るなら人間が判断して入れる (チェックは push ごとにリセットされる)。
//!
//! # 失敗しても PR 作成は止めない
//!
//! トリガーは助言層である。コメントが見つからない / 権限が無い / API が失敗した場合は
//! **loud に記録して続行**する ([ADR-043](../../../../docs/adr/adr-043-security-gates-fail-closed.md)
//! の fail-open 側)。レビューが要ることは人間が気づけばよく、PR 作成を止める理由にはならない。
//! なお他人 (bot) のコメントを編集できるのはリポジトリへの write 権限があるときだけで、
//! 権限が無ければ PATCH が 403 で落ちる — その場合もここで止まらない。

use crate::config::DEFAULT_STEP_TIMEOUT_SECS;
use crate::log::log_info;
use crate::runner::{run_cmd_capture, run_cmd_direct, run_gh_quiet, GH_CMD_TIMEOUT_SECS};
use crate::stages::coderabbit_reviewed::should_skip_request;
use crate::stages::create_pr::write_body_tempfile;

/// CodeRabbit のトリガー用チェックボックスの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TriggerCheckbox {
    /// 未チェック。ここを `- [x]` にするとレビューが始まる。
    Unchecked,
    /// 既にチェック済み。触らない (二重発火させない)。
    Checked,
    /// この本文にトリガー行が無い。自動レビューが効いている PR ではこうなる。
    Absent,
}

/// トリガー行の目印。`checkboxId` の JSON は CodeRabbit が付ける HTML コメントで、
/// **本文中でこの並びを持つ行はトリガー行だけ**である (walkthrough の他のチェックボックスは
/// この marker を持たない)。
const CHECKBOX_MARKER: &str = r#"<!-- {"checkboxId"#;

/// トリガー行に付く見出し。marker と併せて見ることで、将来 CodeRabbit が別用途の
/// checkbox を同じ marker で出しても取り違えない。
const TRIGGER_LABEL: &str = "Trigger review";

fn is_trigger_line(line: &str) -> bool {
    line.contains(CHECKBOX_MARKER) && line.contains(TRIGGER_LABEL)
}

/// 本文からトリガー行の状態を読む (I/O なし)。
pub(crate) fn classify(body: &str) -> TriggerCheckbox {
    for line in body.lines() {
        if !is_trigger_line(line) {
            continue;
        }
        if line.contains("- [ ]") {
            return TriggerCheckbox::Unchecked;
        }
        if line.contains("- [x]") || line.contains("- [X]") {
            return TriggerCheckbox::Checked;
        }
    }
    TriggerCheckbox::Absent
}

/// 未チェックのトリガー行だけを `- [x]` にした本文を返す (I/O なし)。
///
/// **書き換えるのはトリガー行の 1 箇所だけ**。コメント本文には walkthrough や共有リンクなど
/// CodeRabbit が管理する内容が含まれ、PATCH は本文全体を送り直す形になるため、ここで
/// 他の行に触れると相手の管理する記述を壊す。
///
/// 既にチェック済み / トリガー行が無い場合は [`None`] — 呼び手は「何もしない」に倒す。
pub(crate) fn check(body: &str) -> Option<String> {
    if classify(body) != TriggerCheckbox::Unchecked {
        return None;
    }
    let mut done = false;
    let replaced: Vec<String> = body
        .lines()
        .map(|line| {
            if !done && is_trigger_line(line) && line.contains("- [ ]") {
                done = true;
                return line.replacen("- [ ]", "- [x]", 1);
            }
            line.to_string()
        })
        .collect();
    done.then(|| replaced.join("\n"))
}

/// CodeRabbit がコメントを出すまでの待ち。2026-09 の実測では PR 作成から 30〜60 秒で現れたが、
/// 2026-10-21 以降は 16 分後まで遅れた例がある。待ち切れなかったときは
/// [`request_review_by_comment`] で依頼を投稿するので、ここを延ばして PR 作成を待たせはしない。
const POLL_ATTEMPTS: u32 = 6;
const POLL_INTERVAL_SECS: u64 = 15;

/// PR 作成直後に、CodeRabbit のトリガーへチェックを入れる。
///
/// **失敗しても呼び手は止めない** (module doc の fail-open)。見つからない・既にチェック
/// 済み・API 失敗のいずれも、理由を 1 行出して戻る。
pub(crate) fn trigger_review_after_create(repo: &str, pr_number: u64) {
    if should_skip_request(pr_number, Some(repo), "[trigger-review]") {
        return;
    }
    for attempt in 1..=POLL_ATTEMPTS {
        match find_trigger_comment(repo, pr_number) {
            TriggerLookup::Found(comment_id, body) => {
                apply_check(repo, comment_id, &body);
                return;
            }
            TriggerLookup::AlreadyChecked => {
                log_info("[trigger-review] 既にチェック済みでした (二重発火させません)。");
                return;
            }
            TriggerLookup::AutoActive => {
                log_info(
                    "[trigger-review] CodeRabbit は反応済みですがトリガー行がありません。待っても出ないため何もしません (自動レビューが効いているか、rate-limit 等の別応答です。PR 作成は成功しています)。",
                );
                return;
            }
            TriggerLookup::Silent | TriggerLookup::Unavailable if attempt < POLL_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
            }
            TriggerLookup::Silent => request_review_by_comment(repo, pr_number),
            TriggerLookup::Unavailable => log_info(&format!(
                "[trigger-review] CodeRabbit のコメントを照会できなかったため、レビューの依頼は投稿しません (PR 作成は成功しています)。必要なら PR #{pr_number} に手動で @coderabbitai review を投稿してください。",
            )),
        }
    }
}

/// ポーリングの間 CodeRabbit が黙っていたときに、`@coderabbitai review` を投稿して依頼する。
///
/// 待っている間に自動レビューが始まっていれば ([`should_skip_request`] が covered と判定)
/// 投稿しない。投稿は手元の `gh` の認証、つまり PR を作った人間の名義で行う。失敗しても
/// PR 作成は止めない (module doc の fail-open)。
fn request_review_by_comment(repo: &str, pr_number: u64) {
    if should_skip_request(pr_number, Some(repo), "[trigger-review]") {
        return;
    }
    let pr = pr_number.to_string();
    if run_gh_quiet(&review_request_args(repo, &pr)).is_some() {
        log_info(&format!(
            "[trigger-review] CodeRabbit が黙っていたので @coderabbitai review を投稿しました (PR #{pr})。受け付けまで 10 分前後かかることがあります。",
        ));
    } else {
        log_info(&format!(
            "[trigger-review] @coderabbitai review を投稿できませんでした (PR 作成は成功しています)。必要なら PR #{pr} に手動で投稿してください。",
        ));
    }
}

/// レビュー依頼を投稿する `gh` の引数 (I/O なし)。**`--repo` を必ず付ける** — 非 colocated の
/// jj workspace では、付けないと `gh` が別のリポジトリを向くことがある (ADR-045)。
fn review_request_args<'a>(repo: &'a str, pr: &'a str) -> [&'a str; 7] {
    ["pr", "comment", pr, "--repo", repo, "--body", "@coderabbitai review"]
}

/// [`find_trigger_comment`] の 5 分類。
///
/// **「トリガー行が無い」と「CodeRabbit がまだ何も言っていない」を分ける。** 前者は
/// 自動レビューが効いている状態 (順位 520 で 2026-09-10 以降の既定になった) で、待っても
/// トリガー行は出ない。両者を `None` に潰していた頃は auto が効く PR でも毎回 75 秒
/// (15 秒間隔で 6 回照会) 待ってから「見つかりません」と報告していた。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TriggerLookup {
    /// 未チェックのトリガー行を持つコメント。
    Found(u64, String),
    /// トリガー行はあるが既にチェック済み。触らない。
    AlreadyChecked,
    /// CodeRabbit は反応しているがトリガー行が無い。典型は自動レビューが効いている状態だが、
    /// rate-limit 通知など別の応答だけが付いた場合もここに入る。**どちらでも処置は同じ** —
    /// 待ってもトリガー行は出ないので、ポーリングを打ち切る。
    AutoActive,
    /// 照会に成功し、CodeRabbit のコメントがまだ無い。待てば変わりうる。待ち切ったら依頼を投稿する。
    Silent,
    /// 照会に失敗した (gh の失敗・タイムアウト)。待てば変わりうるが、**待ち切っても依頼は
    /// 投稿しない** — 既に通知や依頼があるかを確かめられないまま投稿すると、二重の依頼で
    /// 1 時間に 1 回の枠を使ってしまう (PR #586 の CodeRabbit 指摘)。
    Unavailable,
}

/// CodeRabbit のコメント群からトリガー行の状態を読む。
///
/// 照会の成功と失敗を分ける。`run_gh_quiet` は失敗も空の出力も `None` にするので使わない —
/// コメントが無いときの `--jq` は空を出すのが正常で、それを失敗と同じに扱うと区別できない。
fn find_trigger_comment(repo: &str, pr_number: u64) -> TriggerLookup {
    let cap = run_cmd_capture(
        "gh",
        &[
            "api",
            &format!("repos/{repo}/issues/{pr_number}/comments"),
            "--jq",
            r#".[] | select(.user.login=="coderabbitai[bot]") | "\(.id)\t\(.body|@base64)""#,
        ],
        &[],
        GH_CMD_TIMEOUT_SECS,
    );
    lookup_from(cap.ok, &cap.stdout)
}

/// 照会の成否と出力から [`TriggerLookup`] を決める純粋関数 (I/O なし)。
fn lookup_from(ok: bool, stdout: &str) -> TriggerLookup {
    if ok {
        classify_comments(stdout)
    } else {
        TriggerLookup::Unavailable
    }
}

/// `"<id>\t<base64 body>"` 行の列を [`TriggerLookup`] へ畳む純粋関数 (I/O なし)。
fn classify_comments(raw: &str) -> TriggerLookup {
    let mut seen_coderabbit = false;
    let mut seen_checked = false;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        seen_coderabbit = true;
        let Some((id, encoded)) = line.split_once('\t') else {
            continue;
        };
        let Some(body) = decode_base64(encoded) else {
            continue;
        };
        match classify(&body) {
            TriggerCheckbox::Unchecked => {
                if let Ok(id) = id.parse() {
                    return TriggerLookup::Found(id, body);
                }
            }
            TriggerCheckbox::Checked => seen_checked = true,
            TriggerCheckbox::Absent => {}
        }
    }
    match (seen_checked, seen_coderabbit) {
        (true, _) => TriggerLookup::AlreadyChecked,
        (false, true) => TriggerLookup::AutoActive,
        (false, false) => TriggerLookup::Silent,
    }
}

/// 本文を base64 で受けるのは、コメントに改行・タブ・引用符が混ざるため。
/// `--jq` の出力を行単位で切る都合上、生のまま流すと 1 コメントが複数行に割れる。
fn decode_base64(encoded: &str) -> Option<String> {
    let table = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for byte in encoded.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        acc = (acc << 6) | table(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    String::from_utf8(out).ok()
}

/// チェックを入れた本文で既存コメントを上書きする。
fn apply_check(repo: &str, comment_id: u64, body: &str) {
    let Some(checked) = check(body) else {
        log_info("[trigger-review] 既にチェック済みでした (二重発火させません)。");
        return;
    };
    let temp = match write_body_tempfile(&std::env::temp_dir(), &checked) {
        Ok(path) => path,
        Err(e) => {
            log_info(&format!("[trigger-review] 一時ファイルを作れません (継続): {e}"));
            return;
        }
    };
    let (success, output) = run_cmd_direct(
        "gh",
        &["api", "-X", "PATCH"],
        &[
            format!("repos/{repo}/issues/comments/{comment_id}"),
            "-F".to_string(),
            format!("body=@{}", temp.display()),
            "--silent".to_string(),
        ],
        DEFAULT_STEP_TIMEOUT_SECS,
    );
    if success {
        log_info("[trigger-review] CodeRabbit のレビューを発火しました (Trigger review にチェック)。");
    } else {
        log_info(&format!(
            "[trigger-review] チェックを入れられませんでした (PR 作成は成功しています): {output}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-08 に PR #489 で実測したコメント本文の骨格。
    ///
    /// **トリガー行以外にも `- [ ]` を置いてある。** CodeRabbit のコメントは walkthrough や
    /// pre-merge checks でチェックボックスを使うため、トリガー行が本文中で唯一の
    /// チェックボックスとは限らない。1 個しか置かない fixture では「トリガー行だけを
    /// 書き換える」を守れているか検証できない (2026-09-08 に変異で確認 — 全行を書き換える
    /// 実装でもテストが通ってしまった)。
    fn real_body() -> String {
        [
            "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->",
            "<!-- This is an auto-generated comment: skip review by coderabbit.ai -->",
            "",
            "> [!IMPORTANT]",
            r#"> - [ ] <!-- {"checkboxId":"e9bb8d72-00e8-4f67-9cb2-caf3b22574fe"} --> 🔍 Trigger review"#,
            "> ",
            "> This repository does not receive automatic reviews because it has fewer than 10 stars.",
            "",
            "<details>",
            "<summary>✅ Pre-merge checks</summary>",
            "",
            "- [ ] Title check",
            "- [ ] Description check",
            "",
            "</details>",
        ]
        .join("\n")
    }

    #[test]
    fn an_unchecked_trigger_line_is_detected() {
        assert_eq!(classify(&real_body()), TriggerCheckbox::Unchecked);
    }

    /// **書き換わるのはトリガー行だけ。** 他の行が 1 文字でも変わると、PATCH が
    /// CodeRabbit の管理する記述を壊す。
    #[test]
    fn checking_the_box_changes_only_the_trigger_line() {
        let before = real_body();
        let after = check(&before).expect("未チェックなので書き換わる");
        let changed: Vec<(&str, &str)> = before
            .lines()
            .zip(after.lines())
            .filter(|(b, a)| b != a)
            .collect();
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert!(changed[0].0.contains("- [ ]"));
        assert!(changed[0].1.contains("- [x]"));
        assert_eq!(before.lines().count(), after.lines().count(), "行数が変わっている");
        assert_eq!(classify(&after), TriggerCheckbox::Checked);
    }

    /// 二重発火させない。
    #[test]
    fn an_already_checked_box_is_left_alone() {
        let checked = check(&real_body()).expect("1 回目");
        assert_eq!(classify(&checked), TriggerCheckbox::Checked);
        assert_eq!(check(&checked), None, "2 回目は何もしない");
    }

    /// 自動レビューが効いている PR (walkthrough が出ている) には trigger 行が無い。
    #[test]
    fn a_reviewed_pr_body_has_no_trigger_line() {
        let body = "<!-- walkthrough_start -->\n## Walkthrough\n- [ ] 何かの手順\n";
        assert_eq!(classify(body), TriggerCheckbox::Absent);
        assert_eq!(check(body), None);
    }

    /// **marker と見出しの両方**を見る。片方だけのチェックボックスは対象外 —
    /// CodeRabbit は walkthrough 内でも checkbox を使う。
    #[test]
    fn a_checkbox_without_both_marks_is_not_the_trigger() {
        let marker_only = r#"> - [ ] <!-- {"checkboxId":"x"} --> Something else"#;
        let label_only = "> - [ ] Trigger review";
        assert_eq!(classify(marker_only), TriggerCheckbox::Absent);
        assert_eq!(classify(label_only), TriggerCheckbox::Absent);
    }

    /// `"<id>\t<base64 body>"` 行を組む (実装の `--jq` 出力形式に合わせる)。
    /// base64 は `classify_comments` が使う decoder と同じ表を避けるため、
    /// テスト内で独立に組まず既知値を持たない — ここでは decoder の正しさではなく
    /// **分類の分岐**を見るので、`decode_base64` を通せれば十分。
    fn encode_line(id: u64, body: &str) -> String {
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = body.as_bytes();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(TABLE[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        format!("{id}\t{out}")
    }

    /// 順位 520: CodeRabbit が反応しているがトリガー行が無い = 自動レビューが効いている。
    /// **待たずに戻る**。ここが `Silent` に倒れると auto が効く PR で毎回 90 秒を捨てる。
    #[test]
    fn a_walkthrough_without_a_trigger_line_means_auto_review_is_active() {
        let raw = encode_line(1, "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n## Walkthrough\n");
        assert_eq!(classify_comments(&raw), TriggerLookup::AutoActive);
    }

    /// CodeRabbit のコメントが 1 件も無い場合だけ待つ。
    #[test]
    fn no_coderabbit_comment_yet_means_silent() {
        assert_eq!(classify_comments(""), TriggerLookup::Silent);
        assert_eq!(classify_comments("\n  \n"), TriggerLookup::Silent);
    }

    /// 未チェックのトリガー行があれば id と本文を返す。
    #[test]
    fn an_unchecked_trigger_line_is_returned_with_its_id() {
        let raw = encode_line(4242, &real_body());
        match classify_comments(&raw) {
            TriggerLookup::Found(id, body) => {
                assert_eq!(id, 4242);
                assert_eq!(classify(&body), TriggerCheckbox::Unchecked);
            }
            other => panic!("Found を期待したが {other:?}"),
        }
    }

    /// 既にチェック済みのトリガー行は `AlreadyChecked` — 待たずに戻る (二重発火させない)。
    #[test]
    fn a_checked_trigger_line_is_reported_as_already_checked() {
        let checked = check(&real_body()).expect("1 回目のチェック");
        let raw = encode_line(7, &checked);
        assert_eq!(classify_comments(&raw), TriggerLookup::AlreadyChecked);
    }

    /// `decode_base64` 自体を直接叩く round-trip test。既存テストは `classify`/`check`
    /// 経由でしか通らず、`--jq` の `@base64` が実際に出す改行・タブ・非 ASCII (UTF-8
    /// マルチバイト) を含む本文・パディング (`=`) 付きで decode が壊れていないことを
    /// 保証しない。`encoded` は `printf '...' | base64` で独立に生成した既知値
    /// (このテストが decode 側の実装と同じロジックを再利用して自己検証にならないため)。
    #[test]
    fn decode_base64_round_trips_arbitrary_utf8_bodies() {
        let body = "line one\nline two\ttabbed\n日本語もOK\n";
        let encoded = "bGluZSBvbmUKbGluZSB0d28JdGFiYmVkCuaXpeacrOiqnuOCgk9LCg==";
        assert_eq!(decode_base64(encoded).as_deref(), Some(body));
    }

    /// base64 として不正な入力 (アルファベット外の文字) は `None` — panic せず fail-open に
    /// 倒れることを確認する。
    #[test]
    fn decode_base64_rejects_invalid_alphabet() {
        assert_eq!(decode_base64("not valid base64!!"), None);
    }

    /// PR #586 の CodeRabbit 指摘: 照会の失敗を「黙っている」と区別する。失敗は、出力が
    /// 空でも何かが出ていても `Unavailable` (依頼を投稿しない側) になり、成功した空の結果だけが
    /// `Silent` (待ち切ったら依頼を投稿する側) になる。
    #[test]
    fn a_failed_lookup_is_not_mistaken_for_silence() {
        assert_eq!(lookup_from(false, ""), TriggerLookup::Unavailable);
        assert_eq!(lookup_from(false, "partial output"), TriggerLookup::Unavailable);
        assert_eq!(lookup_from(true, ""), TriggerLookup::Silent);
        assert_eq!(lookup_from(true, "\n"), TriggerLookup::Silent);
    }

    /// 順位 584: CodeRabbit が黙っていたときの依頼は、対象の PR とリポジトリを明示して投稿する。
    /// `--repo` が落ちると、非 colocated の jj workspace で別のリポジトリへ投稿しうる (ADR-045)。
    #[test]
    fn the_review_request_names_the_repository_and_the_pr() {
        assert_eq!(
            review_request_args("aloekun/claude-code-hook-test", "585"),
            [
                "pr",
                "comment",
                "585",
                "--repo",
                "aloekun/claude-code-hook-test",
                "--body",
                "@coderabbitai review"
            ]
        );
    }
}
