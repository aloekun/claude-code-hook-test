//! 発火の経路ラベル [`Reason`] — ADR-055 の「メタデータのみ」契約を**検査で**守る層。
//!
//! # なぜ型だけでは足りなかったか
//!
//! 初版は [`crate::Firing`] の `reason` を `&'static str` にして「safe Rust では実行時に
//! 組み立てた文字列を渡せない」と書いた。**これは誤りだった** — `Box::leak` は safe Rust で
//! `String` を `&'static str` にできる。PR #511 の CodeRabbit 指摘を受けて実測したところ、
//! `Box::leak(format!("src/secret/{}.rs", "path").into_boxed_str())` は素通りし、
//! `"reason":"src/secret/path.rs"` が JSONL に書かれた。lifetime は**素朴な借用を弾くだけ**で、
//! 語彙の保証にはならない。
//!
//! # 何を保証するか (そしてしないか)
//!
//! [`Reason::new`] は**ラベルの形**を検査する: 小文字英数と `-` のみ、1..=32 バイト、
//! 先頭と末尾は `-` でない。これにより区切り (`/` `\`)・空白・`:`・大文字・拡張子の `.` を
//! 含む文字列は作れず、**ファイルパス・コマンド本文・エラー本文は通らない**。
//!
//! **特定の語彙集合に属することは保証しない。** 許可リストを lib 側に持つと、
//! ADR-055 が保つと決めた「観測層をドメインに結合させない」設計 (§ 責務分離) が崩れ、
//! hook が 1 つ増えるたびに中立の器を編集することになる。閉じた enum を採らないのはこの
//! 理由で、保証範囲は形の検査までと明示する。
//!
//! 不正なラベルは `None` になり、フィールドごと行から落ちる。telemetry は observation 層
//! なので panic も block もしない (ADR-055 § 設計原則 fail-open)。

/// ラベルの最大バイト数。id (`testability_gate:scan-incomplete` 等) より短く抑える。
const MAX_LEN: usize = 32;

/// 発火の経路ラベル。[`Reason::new`] を通ったものしか作れない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reason(&'static str);

impl Reason {
    /// 形の検査を通れば `Some`、通らなければ `None`。
    ///
    /// `&'static str` を要求するのは実行時文字列を**書きにくく**するためで、保証の本体は
    /// この検査のほうにある (module doc 参照)。
    pub const fn new(label: &'static str) -> Option<Self> {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_LEN {
            return None;
        }
        if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
            return None;
        }
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
                return None;
            }
            i += 1;
        }
        Some(Reason(label))
    }

    /// JSONL へ書く文字列。
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_labels_in_use() {
        for label in ["diff-failed", "unparsable-status", "files-unscannable", "a", "x9"] {
            assert!(Reason::new(label).is_some(), "拒否されました: {label}");
        }
    }

    /// 実際に載せたくないもの。**この一覧が保証の内容**なので、形ごとに 1 件ずつ置く。
    /// 形の名前はコメントではなくデータとして持ち、失敗時にどの形が漏れたかを出す。
    #[test]
    fn rejects_paths_command_text_and_error_bodies() {
        let bs = char::from(92u8);
        let windows_path = format!("src{bs}secret{bs}path.rs").leak();
        let too_long = "x".repeat(MAX_LEN + 1).leak();
        for (shape, label) in [
            ("空", ""),
            ("パス (slash)", "src/secret/path.rs"),
            ("パス (backslash)", &*windows_path),
            ("コマンド本文", "cargo test --workspace"),
            ("エラー本文", "Error: file not found"),
            ("ドット", "scan.incomplete"),
            ("大文字", "Scan-Incomplete"),
            ("先頭ハイフン", "-leading"),
            ("末尾ハイフン", "trailing-"),
            ("アンダースコア", "unparsable_status"),
            ("長さ超過", too_long),
        ] {
            assert!(
                Reason::new(label).is_none(),
                "{shape} が通ってしまいました: {label:?}"
            );
        }
    }

    /// `Box::leak` で作った `&'static str` も検査を通らない (初版の穴、PR #511)。
    #[test]
    fn rejects_a_leaked_runtime_string() {
        let leaked: &'static str = Box::leak(format!("src/secret/{}.rs", "path").into_boxed_str());
        assert!(Reason::new(leaked).is_none());
    }

    #[test]
    fn as_str_round_trips() {
        assert_eq!(Reason::new("diff-failed").unwrap().as_str(), "diff-failed");
    }
}
