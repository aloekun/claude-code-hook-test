//! 字句解析が返すデータの型 (dialect・語の断片・語・segment)。[`super`] から分けた (ADR-080、振る舞いは変えない)。

/// どのシェルの構文として読むか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    Bash,
    PowerShell,
    Cmd,
}

/// 語を構成する断片。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    /// 文字どおりの文字列。
    Lit(String),
    /// 変数の参照 (`$NAME` / `${NAME}` / `$env:NAME`)。同じコマンド内の代入でだけ解決できる。
    Var(String),
    /// コマンド置換・`$1` などの特殊変数・here-string など、中身を静的に決められないもの。
    Opaque,
}

/// 1 つの語。`glob` は引用符の外にワイルドカード (`*` `?`、Bash では `[` `{` も) があったか。
/// `split_prone` は Bash で引用符の外に変数・コマンド置換の展開があったか。Bash はこの展開を
/// 空白で複数の語に分けるので、空白を含む値に展開されたら 1 つのパスとは見なせない
/// (`X='/a /b'; rm -rf $X` は 2 つの対象。CodeRabbit の指摘)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Word {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) glob: bool,
    pub(crate) split_prone: bool,
    /// 引用符で始まった語か。PowerShell では、引用符で始まる文はコマンドではなく式
    /// (`"text" | Set-Clipboard`) なので、コマンド名として扱わない。
    pub(crate) starts_quoted: bool,
}

impl Word {
    /// 文字列だけの語を作る。
    pub(crate) fn lit(text: &str) -> Self {
        Word {
            pieces: vec![Piece::Lit(text.to_string())],
            glob: false,
            split_prone: false,
            starts_quoted: false,
        }
    }

    /// 語の末尾に 1 文字足す。
    pub(super) fn push_char(&mut self, c: char) {
        match self.pieces.last_mut() {
            Some(Piece::Lit(s)) => s.push(c),
            _ => self.pieces.push(Piece::Lit(c.to_string())),
        }
    }

    /// 変数も Opaque も含まない語なら、その文字列。
    pub(crate) fn literal(&self) -> Option<String> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Lit(s) => out.push_str(s),
                _ => return None,
            }
        }
        Some(out)
    }

    /// 先頭の Lit 断片 (オプション名の判定に使う)。
    pub(crate) fn leading_lit(&self) -> &str {
        match self.pieces.first() {
            Some(Piece::Lit(s)) => s,
            _ => "",
        }
    }

    /// 先頭の Lit 断片だけを `new_head` に差し替えた語を作る。変数・Opaque を含む残りの断片と
    /// `glob` はそのまま引き継ぐ (`NAME=value` の `value` 側を取り出す `commands.rs` の
    /// `split_assignment`/`bash_c_inline_script`/`inline_value` が同じ組み立てを 3 重複させていた
    /// — SIM-NEW-commands-rs-L309)。
    pub(crate) fn with_head(&self, new_head: &str) -> Self {
        let mut out = Word::lit(new_head);
        out.pieces.extend(self.pieces.iter().skip(1).cloned());
        out.glob = self.glob;
        out.split_prone = self.split_prone;
        out
    }
}

/// 直前の segment とのつながり方。`&&` / `||` / `|` の後の segment は、直前の segment の結果に
/// よって実行されたりされなかったりする (`||` とパイプでは、直前の `cd` も効かない)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Join {
    /// 先頭、または `;` / 改行 / `&` の後。直前の segment に関わらず実行される。
    #[default]
    Seq,
    /// `&&` の後。直前が成功したときだけ実行される。
    And,
    /// `||` の後。直前が失敗したときだけ実行される。
    Or,
    /// `|` の後。パイプの各段は別のプロセスで動く。
    Pipe,
}

/// 1 つのコマンド。`piped_in` は直前がパイプ (`|`) だったか。
#[derive(Clone, Debug, Default)]
pub(crate) struct Segment {
    pub(crate) words: Vec<Word>,
    pub(crate) piped_in: bool,
    /// Bash の heredoc (`<<EOF` ... `EOF`) の本文。コマンドではなく標準入力のデータなので、
    /// 語としては読まない。本文を実行するのは、シェルに渡したとき (`bash <<EOF`) だけ。
    pub(crate) heredoc: String,
    /// 区切り語を引用しない heredoc (`<<EOF`) の本文に、コマンド置換 (`$(...)` / `` `...` ``) が
    /// あるか。実 Bash は渡す先のコマンドに関わらず置換を実行するが、中身は読まずに確認する
    /// (ADR-086 決定 7)。
    pub(crate) heredoc_substitutes: bool,
    /// PowerShell の呼び出し演算子 `&` または、同じく動的呼び出しのドットソーシング演算子 `.`
    /// で始まったか。これらの直後は「式」ではなく明示的なコマンド呼び出しなので、先頭語が
    /// 解決できないときに「式だから止めない」例外 (`$env:PATH -split ';'` のような裸の変数
    /// 参照向け) を適用してはならない (`&` は SEC-NEW-commands-rs-L123、`.` は
    /// SEC-NEW-lexer-rs-L247)。
    pub(crate) called: bool,
    /// Bash の `(` `)` によるサブシェルの入れ子の深さ (0 なら最上位)。実 Bash では `(...)` の
    /// 中で `cd`/変数代入をしても、`)` を抜けた外側のシェルには伝わらない。`analyze` がこの深さの
    /// 増減を見て `state.cwd`/`state.vars` を push/pop し、サブシェル内の変化を閉じ込める
    /// (SEC-NEW-lexer-rs-L178)。Bash 以外の dialect では常に 0 (PowerShell/cmd の `(...)` は
    /// 新しいプロセスを fork しないため)。
    pub(crate) depth: usize,
    /// Bash の単発バックグラウンドジョブ (`cmd &`) として終端したか。背景実行も fork された
    /// 子プロセスで動くため、`cd`/変数代入が外側に伝わらない。`depth` と同じ理由
    /// (SEC-NEW-lexer-rs-L178) で `analyze` がこの segment だけ state を save/restore する。
    pub(crate) background: bool,
    /// 直前の segment とのつながり方 ([`Join`])。
    pub(crate) join: Join,
    /// この segment の中のコマンド置換 (`$(...)` / `` `...` `` / PowerShell の `$(...)` `@(...)` と
    /// コマンド途中の `(...)`) の本文。外側のシェルが実行するコマンドなので、`analyze` が別の
    /// コマンド列として読む (CodeRabbit の指摘: `echo "$(rm -rf ~)"` を一度も読んでいなかった)。
    pub(crate) substitutions: Vec<String>,
    /// Bash の case のパターン (`a|b)` の `)` で終わる語の並び)。コマンドではない。
    pub(crate) case_pattern: bool,
    /// Bash の関数定義の名前 (`name()` の `name`)。本体は後ろの segment として読む。
    pub(crate) function_name: bool,
    /// 標準入力を `<` で受け取るか (`sh < script.sh`)。シェルなら、パイプと同じく中身が見えない
    /// スクリプトを実行する。here-string (`<<<`) は本文を [`Segment::heredoc`] に入れるので含めない。
    pub(crate) stdin_redirected: bool,
    /// PowerShell の `{ }` の入れ子の深さ。ブロックの中のコマンドは実行されるとは限らない
    /// (`if (…) { Set-Location x }`、関数の本体) ので、抜けたら中の変化を不明にする。
    pub(crate) brace_depth: usize,
    /// PowerShell の stop-parsing symbol `--%` が現れたか。以降は構文規則を適用せず生の文字列で
    /// 子プロセスへ渡るため解析できない — `commands::handled_before_the_command` がこれを見て
    /// この segment 全体を判定できない削除として確認する (SEC-NEW-lexer-mod-rs-L307)。
    pub(crate) stop_parsing: bool,
    /// 上書きのリダイレクト (`> file` / `>| file`) の書き込み先。既にあるファイルの中身が消えるので、
    /// コピーと同じく上書きとして確認する (CodeRabbit の指摘、ADR-086)。`>>` / `2>&1` は含めない。
    pub(crate) redirect_targets: Vec<Word>,
}
