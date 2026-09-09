//! PR 作成後に CodeRabbit のレビューを発火させる (順位 321)。
//!
//! # なぜ要るのか
//!
//! CodeRabbit は 2026-09-08 に **star 10 未満のリポジトリへの自動レビューを停止**した。
//! それまでは `.coderabbit.yaml` の `auto_review.enabled: true` で初回 PR は自動レビュー
//! され、fix push だけ手動トリガーを要する設計だった (ADR-019 § WP-03)。今は**初回から
//! トリガーが要る**ため、その設計の前提が崩れている。
//!
//! 停止時、CodeRabbit は PR へ次の形のコメントを出す。チェックを入れるとレビューが始まる:
//!
//! ```text
//! > - [ ] <!-- {"checkboxId":"..."} --> 🔍 Trigger review
//! >
//! > This repository does not receive automatic reviews because it has fewer than 10 stars.
//! ```
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
//! **loud に記録して続行**する ([ADR-043](../../../docs/adr/adr-043-security-gates-fail-closed.md)
//! の fail-open 側)。レビューが要ることは人間が気づけばよく、PR 作成を止める理由にはならない。
//! なお他人 (bot) のコメントを編集できるのはリポジトリへの write 権限があるときだけで、
//! 権限が無ければ PATCH が 403 で落ちる — その場合もここで止まらない。

use crate::config::DEFAULT_STEP_TIMEOUT_SECS;
use crate::log::log_info;
use crate::runner::{run_cmd_direct, run_gh_quiet};
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

/// CodeRabbit がコメントを出すまでの待ち。実測では PR 作成から 30〜60 秒で現れる。
const POLL_ATTEMPTS: u32 = 6;
const POLL_INTERVAL_SECS: u64 = 15;

/// PR 作成直後に、CodeRabbit のトリガーへチェックを入れる。
///
/// **失敗しても呼び手は止めない** (module doc の fail-open)。見つからない・既にチェック
/// 済み・API 失敗のいずれも、理由を 1 行出して戻る。
pub(crate) fn trigger_review_after_create(repo: &str, pr_number: u64) {
    for attempt in 1..=POLL_ATTEMPTS {
        match find_trigger_comment(repo, pr_number) {
            Some((comment_id, body)) => {
                apply_check(repo, comment_id, &body);
                return;
            }
            None if attempt < POLL_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
            }
            None => log_info(
                "[trigger-review] CodeRabbit のトリガーコメントが見つかりません。自動レビューが効いているか、コメントがまだ出ていません (PR 作成は成功しています)。",
            ),
        }
    }
}

/// トリガー行を持つ CodeRabbit コメントの `(id, body)` を返す。
fn find_trigger_comment(repo: &str, pr_number: u64) -> Option<(u64, String)> {
    let raw = run_gh_quiet(&[
        "api",
        &format!("repos/{repo}/issues/{pr_number}/comments"),
        "--jq",
        r#".[] | select(.user.login=="coderabbitai[bot]") | "\(.id)\t\(.body|@base64)""#,
    ])?;
    for line in raw.lines() {
        let (id, encoded) = line.split_once('\t')?;
        let body = decode_base64(encoded)?;
        if classify(&body) == TriggerCheckbox::Unchecked {
            return Some((id.parse().ok()?, body));
        }
    }
    None
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
}
