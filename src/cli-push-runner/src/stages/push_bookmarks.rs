//! push 対象の bookmark 名のリスト。**空にならないことを型で保証する** (順位 484)。
//!
//! PR #434 (順位 288(b)) の根因は、fail-closed の判定結果である空リストが push stage の
//! fallback に「対象の指定なし」と読み替えられたことだった。`build_push_command` は空リストを
//! 受けると base command (`jj git push`) をそのまま実行し、jj 0.42 の bare push は tracked
//! bookmark を全件送る。レビュー範囲の外にある ref まで出る経路である。
//!
//! 修正後も `bookmark_check` から push stage までは `Vec<String>` で運んでおり、「`Some` なら
//! 1 件以上」は doc comment に書いてあるだけだった。空リストを作る経路がどこかで復活しても、
//! コンパイラもテストも気づかない。本型は空のリストを構築できないようにし、push stage が
//! 空リストを受け取る経路そのものを消す。
//!
//! 読み取りは `Deref<Target = [String]>` で既存の `&[String]` を取る関数へそのまま渡せる。
//! 構築は `new` だけで、空なら `None` を返す。

use std::ops::Deref;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PushBookmarks(Vec<String>);

impl PushBookmarks {
    /// `names` が空なら `None`。呼び出し側は `None` を「push 対象が無い」として扱い、
    /// pipeline を止める。
    pub(crate) fn new(names: Vec<String>) -> Option<Self> {
        if names.is_empty() {
            None
        } else {
            Some(Self(names))
        }
    }
}

impl Deref for PushBookmarks {
    type Target = [String];

    fn deref(&self) -> &[String] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_empty_list() {
        assert_eq!(PushBookmarks::new(Vec::new()), None);
    }

    #[test]
    fn keeps_every_name_in_order() {
        let bookmarks = PushBookmarks::new(vec!["feat/a".to_string(), "fix-b".to_string()]).expect("non-empty");
        assert_eq!(&*bookmarks, ["feat/a".to_string(), "fix-b".to_string()].as_slice());
    }
}
