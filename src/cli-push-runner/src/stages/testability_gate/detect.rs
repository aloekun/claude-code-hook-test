//! 検出エンジン — 「I/O 出力をその場で解釈して判定を返す関数」を AST で見つける。
//!
//! # 何を止めるか
//!
//! G1 (判定が I/O と癒着し、テストの場が最初から無い) の**新規混入**である。
//! 典型は次の形で、`out` の解釈を単体テストする場所がどこにも無い:
//!
//! ```ignore
//! fn diff_is_empty() -> bool {
//!     let (ok, out) = run_cmd_direct("jj", &["diff"], &[], 30);
//!     if !ok { return true; }        // ← エラー処理。これ自体は問題にしない
//!     out.trim().is_empty()          // ← I/O 出力のインライン解釈。これを止める
//! }
//! ```
//!
//! 望ましい形は解釈を名前付きの純関数へ出すことで、そうすると発火しない:
//!
//! ```ignore
//! fn diff_at_is_empty() -> bool {
//!     interpret_at_emptiness(query_at_emptiness())   // ← 純関数へ委譲 = テストの場がある
//! }
//! ```
//!
//! **回避操作が望ましい refactor と一致する**のが本 gate の性質である。1 行の純関数へ
//! 切り出せば通るが、それこそが作りたかったテストの場である。
//!
//! # 射程外 (意図的。doc に明記して追わない)
//!
//! - **分岐して literal を返す形** (`if x.is_empty() { return false }`)。判定の合成が
//!   関数内にある問題は捉えない。stage の entry point がすべてこの形であり、
//!   発火させると FP が支配的になる
//! - **bool 以外の判定型** (独自 enum / タプル / `Option<Vec<T>>`)。`Option<T>` を一般に
//!   含めると「読んで parse して返すだけ」の thin wrapper が大量に当たる
//! - **呼び出し側での解釈** / **別ファイルの I/O ヘルパ経由** (名前が `run_cmd*` 以外)
//!
//! 完全な検査ではなく ratchet である。効果は Phase 4 の測定で追う。

use std::collections::HashSet;

use syn::visit::Visit;
use syn::{Block, Expr, ImplItem, Item, Local, Pat, ReturnType, Stmt, Type};

/// 発火した関数 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) function: String,
    pub(crate) line: usize,
}

/// I/O 原子とみなす path。末尾一致で判定する。
const IO_PATH_MARKERS: &[&str] = &[
    "Command::new",
    "fs::read_to_string",
    "fs::read",
    "fs::write",
    "fs::read_dir",
    "fs::metadata",
    "fs::create_dir_all",
    "fs::remove_file",
    "fs::rename",
    "fs::copy",
    "File::open",
    "File::create",
    "env::var",
];

/// I/O 原子とみなす関数名の接頭辞 (`lib-subprocess` の共通ヘルパ群)。
const IO_FN_PREFIXES: &[&str] = &["run_cmd", "run_gh", "run_jj"];

/// I/O の**成否**を見るだけの method。内容の解釈ではないので発火させない。
const IO_STATUS_METHODS: &[&str] = &["success", "is_ok", "is_err", "is_some", "is_none"];

/// Rust ソース 1 ファイルを走査する。parse 不能なら `Err` (fail-closed で呼び出し側が扱う)。
pub(crate) fn scan_rust_source(src: &str) -> Result<Vec<Finding>, String> {
    let file = syn::parse_file(src).map_err(|e| format!("Rust ソースを parse できません: {e}"))?;
    let mut fns = Vec::new();
    collect_fns(&file.items, &mut fns);

    let same_file: HashSet<String> = fns.iter().map(|f| f.name.clone()).collect();
    let io_fns: HashSet<String> = fns
        .iter()
        .filter(|f| block_has_direct_io(f.block))
        .map(|f| f.name.clone())
        .collect();

    let mut findings = Vec::new();
    for f in &fns {
        if !returns_bool_decision(f.output) {
            continue;
        }
        if decides_from_inline_io(f.block, &io_fns, &same_file) {
            findings.push(Finding {
                function: f.name.clone(),
                line: f.line,
            });
        }
    }
    findings.sort_by_key(|f| f.line);
    Ok(findings)
}

struct FnItem<'a> {
    name: String,
    line: usize,
    output: &'a ReturnType,
    block: &'a Block,
}

fn collect_fns<'a>(items: &'a [Item], out: &mut Vec<FnItem<'a>>) {
    for item in items {
        match item {
            Item::Fn(f) => out.push(FnItem {
                name: f.sig.ident.to_string(),
                line: f.sig.ident.span().start().line,
                output: &f.sig.output,
                block: &f.block,
            }),
            Item::Impl(i) => {
                for it in &i.items {
                    if let ImplItem::Fn(f) = it {
                        out.push(FnItem {
                            name: f.sig.ident.to_string(),
                            line: f.sig.ident.span().start().line,
                            output: &f.sig.output,
                            block: &f.block,
                        });
                    }
                }
            }
            Item::Mod(m) => {
                // NOTE: `#[cfg(test)]` module は対象外 (テストコードを検査しても意味が無い)。
                if has_cfg_test(&m.attrs) {
                    continue;
                }
                if let Some((_, items)) = &m.content {
                    collect_fns(items, out);
                }
            }
            _ => {}
        }
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let path = path_string(a.path());
        path == "cfg" && a.to_token_stream_string().contains("test")
    })
}

trait AttrExt {
    fn to_token_stream_string(&self) -> String;
}

impl AttrExt for syn::Attribute {
    fn to_token_stream_string(&self) -> String {
        match &self.meta {
            syn::Meta::List(list) => list.tokens.to_string(),
            _ => String::new(),
        }
    }
}

/// 返り値が bool 系の判定型か (`bool` / `Option<bool>` / `Result<bool, _>`)。
fn returns_bool_decision(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };
    is_bool_decision_type(ty)
}

fn is_bool_decision_type(ty: &Type) -> bool {
    let Type::Path(p) = ty else {
        return false;
    };
    let Some(last) = p.path.segments.last() else {
        return false;
    };
    let name = last.ident.to_string();
    if name == "bool" {
        return true;
    }
    if name != "Option" && name != "Result" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(first)) = args.args.first() else {
        return false;
    };
    matches!(first, Type::Path(p) if p.path.is_ident("bool"))
}

fn path_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn is_io_path(path: &syn::Path) -> bool {
    let joined = path_string(path);
    if IO_PATH_MARKERS.iter().any(|m| joined.ends_with(m)) {
        return true;
    }
    path.segments
        .last()
        .map(|s| {
            let name = s.ident.to_string();
            IO_FN_PREFIXES.iter().any(|p| name.starts_with(p))
        })
        .unwrap_or(false)
}

/// 本体に I/O 原子を直接含むか (同一ファイル 1 ホップ伝播の判定材料)。
fn block_has_direct_io(block: &Block) -> bool {
    struct V(bool);
    impl<'ast> Visit<'ast> for V {
        fn visit_expr(&mut self, e: &'ast Expr) {
            match e {
                Expr::Call(c) => {
                    if let Expr::Path(p) = &*c.func {
                        if is_io_path(&p.path) {
                            self.0 = true;
                        }
                    }
                }
                Expr::MethodCall(m) => {
                    let name = m.method.to_string();
                    if IO_FN_PREFIXES.iter().any(|p| name.starts_with(p)) {
                        self.0 = true;
                    }
                }
                _ => {}
            }
            syn::visit::visit_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_block(block);
    v.0
}

/// 関数が「I/O 出力のインライン解釈」で判定を返しているか。
///
/// 手順: (1) I/O 由来の値が束縛された local を集める → (2) 返り値の式 (tail + `return`) が
/// その値から導かれているかを見る。同一ファイル内の **I/O を持たない関数**への呼び出しは
/// 汚染を止める (= そこがテストの場)。外部 crate の呼び出し (parse / deserialize 等) は
/// 汚染を通す。
fn decides_from_inline_io(
    block: &Block,
    io_fns: &HashSet<String>,
    same_file: &HashSet<String>,
) -> bool {
    let mut ctx = TaintCtx {
        tainted: HashSet::new(),
        interpreted: HashSet::new(),
        io_fns,
        same_file,
    };
    ctx.collect_taint(block);
    let mut decisions = Vec::new();
    collect_decision_exprs(block, &mut decisions);
    decisions.iter().any(|e| ctx.is_interpretation(e))
}

struct TaintCtx<'a> {
    tainted: HashSet<String>,
    /// `init` が I/O 出力の**解釈**である local の ident。解釈を 1 度 local へ束縛してから
    /// 返す形 (`let empty = out.trim() == "true"; empty`) を素通りさせないために持つ
    /// (CodeRabbit #456)。
    interpreted: HashSet<String>,
    io_fns: &'a HashSet<String>,
    same_file: &'a HashSet<String>,
}

impl TaintCtx<'_> {
    /// `let` 束縛を走査し、I/O 由来の値を受けた ident を汚染集合に入れる。
    /// let-else (`let Ok(x) = ... else`) も `Local` なのでここで拾える。
    fn collect_taint(&mut self, block: &Block) {
        // NOTE: 1 パスで足りる (前方参照なし)。スコープは追わず、ネストした block の束縛も同じ集合へ入れる近似。
        let mut locals = Vec::new();
        collect_locals(block, &mut locals);
        for local in locals {
            let Some(init) = &local.init else { continue };
            if !self.is_tainted(&init.expr) {
                continue;
            }
            let init_is_interpretation = self.is_interpretation(&init.expr);
            let mut idents = HashSet::new();
            collect_pat_idents(&local.pat, &mut idents);
            if init_is_interpretation {
                self.interpreted.extend(idents.iter().cloned());
            }
            self.tainted.extend(idents);
        }
    }

    /// 返り値の式が「I/O 出力の**解釈**」か。汚染値をそのまま返す形 (`ok` / `success`) と、
    /// I/O の成否を見るだけの形 (`.success()` / `.is_ok()`) は解釈ではない — 解釈すべき
    /// 出力内容が無いので、切り出す先も無いからである。
    fn is_interpretation(&self, expr: &Expr) -> bool {
        self.is_interpretation_with(expr, &HashSet::new())
    }

    fn is_interpretation_with(&self, expr: &Expr, extra: &HashSet<String>) -> bool {
        match expr {
            // NOTE: 汚染値をそのまま返す (`ok`) は解釈ではない。解釈結果を束縛した local は解釈として扱う。
            Expr::Path(p) => p
                .path
                .get_ident()
                .is_some_and(|i| self.interpreted.contains(&i.to_string())),
            Expr::Paren(p) => self.is_interpretation_with(&p.expr, extra),
            Expr::Group(g) => self.is_interpretation_with(&g.expr, extra),
            Expr::Unary(u) => self.is_interpretation_with(&u.expr, extra),
            // NOTE: I/O の成否を見るだけ (`.success()` / `.is_ok()`)。解釈すべき内容が無い。
            Expr::MethodCall(m) if IO_STATUS_METHODS.contains(&m.method.to_string().as_str()) => {
                false
            }
            Expr::Call(c) => {
                // NOTE: `Ok(x)` / `Some(x)` は包むだけなので中身を見る。他の呼び出しは委譲 (呼ばれた側が検査対象)。
                if is_wrapper_ctor(&c.func) {
                    return c.args.iter().any(|a| self.is_interpretation_with(a, extra));
                }
                false
            }
            Expr::Match(m) => {
                let scrutinee_tainted = self.is_tainted_with(&m.expr, extra);
                m.arms.iter().any(|arm| {
                    let mut scoped = extra.clone();
                    if scrutinee_tainted {
                        collect_pat_idents(&arm.pat, &mut scoped);
                    }
                    self.is_interpretation_with(&arm.body, &scoped)
                })
            }
            Expr::If(i) => {
                let mut scoped = extra.clone();
                if let Expr::Let(l) = &*i.cond {
                    if self.is_tainted_with(&l.expr, extra) {
                        collect_pat_idents(&l.pat, &mut scoped);
                    }
                }
                block_tail(&i.then_branch)
                    .is_some_and(|e| self.is_interpretation_with(e, &scoped))
                    || i.else_branch
                        .as_ref()
                        .is_some_and(|(_, e)| self.is_interpretation_with(e, extra))
            }
            Expr::Block(b) => {
                block_tail(&b.block).is_some_and(|e| self.is_interpretation_with(e, extra))
            }
            other => self.is_tainted_with(other, extra),
        }
    }

    fn is_tainted(&self, expr: &Expr) -> bool {
        self.is_tainted_with(expr, &HashSet::new())
    }

    /// `extra` は match / if-let の arm pattern が束縛した ident (そのアームだけの汚染)。
    fn is_tainted_with(&self, expr: &Expr, extra: &HashSet<String>) -> bool {
        let t = |e: &Expr| self.is_tainted_with(e, extra);
        match expr {
            Expr::Path(p) => p
                .path
                .get_ident()
                .map(|i| {
                    let name = i.to_string();
                    self.tainted.contains(&name) || extra.contains(&name)
                })
                .unwrap_or(false),
            Expr::Call(c) => {
                if let Expr::Path(p) = &*c.func {
                    if is_io_path(&p.path) {
                        return true;
                    }
                    if let Some(last) = p.path.segments.last() {
                        let name = last.ident.to_string();
                        if self.io_fns.contains(&name) {
                            return true;
                        }
                        // NOTE: 同一ファイル内の I/O を持たない関数 = テストの場。汚染を止める。
                        if self.same_file.contains(&name) {
                            return false;
                        }
                    }
                }
                c.args.iter().filter(|a| !matches!(a, Expr::Closure(_))).any(t)
            }
            Expr::MethodCall(m) => {
                let name = m.method.to_string();
                if IO_FN_PREFIXES.iter().any(|p| name.starts_with(p)) {
                    return true;
                }
                t(&m.receiver)
                    || m.args.iter().filter(|a| !matches!(a, Expr::Closure(_))).any(t)
            }
            Expr::Binary(b) => t(&b.left) || t(&b.right),
            Expr::Unary(u) => t(&u.expr),
            Expr::Paren(p) => t(&p.expr),
            Expr::Group(g) => t(&g.expr),
            Expr::Reference(r) => t(&r.expr),
            Expr::Field(f) => t(&f.base),
            Expr::Index(i) => t(&i.expr) || t(&i.index),
            Expr::Try(x) => t(&x.expr),
            Expr::Cast(c) => t(&c.expr),
            Expr::Await(a) => t(&a.base),
            Expr::Tuple(x) => x.elems.iter().any(t),
            Expr::Closure(c) => t(&c.body),
            Expr::Block(b) => block_tail(&b.block).is_some_and(t),
            // NOTE: scrutinee が汚染でも、アームが literal を返すなら汚染ではない (I/O の成否を返すだけの形)。
            Expr::Match(m) => {
                let scrutinee_tainted = t(&m.expr);
                m.arms.iter().any(|arm| {
                    let mut scoped = extra.clone();
                    if scrutinee_tainted {
                        collect_pat_idents(&arm.pat, &mut scoped);
                    }
                    self.is_tainted_with(&arm.body, &scoped)
                })
            }
            // NOTE: `if let Ok(x) = <tainted>` も同じ扱い。条件そのものの汚染 (成否の分岐) では汚染しない。
            Expr::If(i) => {
                let mut scoped = extra.clone();
                if let Expr::Let(l) = &*i.cond {
                    if t(&l.expr) {
                        collect_pat_idents(&l.pat, &mut scoped);
                    }
                }
                block_tail(&i.then_branch).is_some_and(|e| self.is_tainted_with(e, &scoped))
                    || i.else_branch.as_ref().is_some_and(|(_, e)| t(e))
            }
            _ => false,
        }
    }
}

fn collect_locals<'a>(block: &'a Block, out: &mut Vec<&'a Local>) {
    struct V<'a, 'b>(&'b mut Vec<&'a Local>);
    impl<'ast, 'b> Visit<'ast> for V<'ast, 'b> {
        fn visit_local(&mut self, l: &'ast Local) {
            self.0.push(l);
            syn::visit::visit_local(self, l);
        }
    }
    let mut v = V(out);
    v.visit_block(block);
}

fn collect_pat_idents(pat: &Pat, out: &mut HashSet<String>) {
    match pat {
        Pat::Ident(i) => {
            out.insert(i.ident.to_string());
            if let Some((_, sub)) = &i.subpat {
                collect_pat_idents(sub, out);
            }
        }
        Pat::Tuple(t) => t.elems.iter().for_each(|p| collect_pat_idents(p, out)),
        Pat::TupleStruct(t) => t.elems.iter().for_each(|p| collect_pat_idents(p, out)),
        Pat::Struct(s) => s.fields.iter().for_each(|f| collect_pat_idents(&f.pat, out)),
        Pat::Reference(r) => collect_pat_idents(&r.pat, out),
        Pat::Type(t) => collect_pat_idents(&t.pat, out),
        Pat::Or(o) => o.cases.iter().for_each(|p| collect_pat_idents(p, out)),
        Pat::Slice(s) => s.elems.iter().for_each(|p| collect_pat_idents(p, out)),
        _ => {}
    }
}

/// 返り値になる式 = block の tail + すべての `return <expr>`。
fn collect_decision_exprs<'a>(block: &'a Block, out: &mut Vec<&'a Expr>) {
    if let Some(tail) = block_tail(block) {
        out.push(tail);
    }
    struct V<'a, 'b>(&'b mut Vec<&'a Expr>);
    impl<'ast, 'b> Visit<'ast> for V<'ast, 'b> {
        fn visit_expr(&mut self, e: &'ast Expr) {
            if let Expr::Return(r) = e {
                if let Some(inner) = &r.expr {
                    self.0.push(inner);
                }
            }
            syn::visit::visit_expr(self, e);
        }
        // NOTE: 内側の closure / item の return は対象外にしない (近似)。
    }
    let mut v = V(out);
    v.visit_block(block);
}

/// `Ok` / `Some` / `Err` のような包むだけの constructor 呼び出しか。
fn is_wrapper_ctor(func: &Expr) -> bool {
    let Expr::Path(p) = func else {
        return false;
    };
    p.path
        .segments
        .last()
        .map(|s| matches!(s.ident.to_string().as_str(), "Ok" | "Some" | "Err"))
        .unwrap_or(false)
}

fn block_tail(block: &Block) -> Option<&Expr> {
    match block.stmts.last() {
        Some(Stmt::Expr(e, None)) => Some(e),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        scan_rust_source(src)
            .expect("parse")
            .into_iter()
            .map(|f| f.function)
            .collect()
    }

    /// 順位 490 の実不具合コード (master `dd86b697` の `diff_at_is_empty`)。
    /// I/O 出力 `out` を `out.trim() == "true"` でその場で解釈している。
    const INCIDENT_490: &str = r#"
        fn diff_at_is_empty() -> bool {
            let (ok, out) = run_cmd_direct("jj", &["log"], &[], 30);
            if !ok {
                log_info("判定失敗");
                return false;
            }
            out.trim() == "true"
        }
    "#;

    /// PR V が入れた修正後の形。解釈は純関数 `interpret_at_emptiness` にある。
    const REMEDY_490: &str = r#"
        fn diff_at_is_empty() -> bool {
            interpret_at_emptiness(query_at_emptiness())
        }
        fn query_at_emptiness() -> Result<String, String> {
            let (ok, out) = run_cmd_direct("jj", &["log"], &[], 30);
            if ok { Ok(out) } else { Err(out) }
        }
        fn interpret_at_emptiness(raw: Result<String, String>) -> bool {
            matches!(raw, Ok(s) if s.trim() == "true")
        }
    "#;

    #[test]
    fn fires_on_the_incident_shape() {
        assert_eq!(names(INCIDENT_490), vec!["diff_at_is_empty"]);
    }

    #[test]
    fn does_not_fire_on_the_remedy_shape() {
        assert!(names(REMEDY_490).is_empty(), "{:?}", names(REMEDY_490));
    }


    /// 解釈を local へ 1 度束縛してから返す形も発火する (CodeRabbit #456)。
    /// `INCIDENT_490` と実質同じで、`let` を 1 行挟むだけで通れては gate の意味が無い。
    #[test]
    fn fires_when_the_interpretation_is_bound_to_a_local() {
        let src = r#"
            fn diff_at_is_empty() -> bool {
                let (ok, out) = run_cmd_direct("jj", &["log"], &[], 30);
                if !ok { return false; }
                let empty = out.trim() == "true";
                empty
            }
        "#;
        assert_eq!(names(src), vec!["diff_at_is_empty"]);
    }

    /// 汚染値をそのまま束縛して返す形は解釈ではない (I/O の成否をそのまま返す wrapper)。
    #[test]
    fn does_not_fire_when_a_bound_local_is_just_the_io_status() {
        let src = r#"
            fn push_ok() -> bool {
                let (ok, _out) = run_cmd_direct("jj", &["git", "push"], &[], 30);
                let succeeded = ok;
                succeeded
            }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }
    /// I/O をヘルパ経由で取ってからインライン解釈する形 (同一ファイル 1 ホップ)。
    #[test]
    fn fires_when_io_comes_through_a_same_file_helper() {
        let src = r#"
            fn query_raw() -> String {
                let (_, out) = std::process::Command::new("jj").output().unwrap();
                out
            }
            fn is_clean() -> bool {
                let raw = query_raw();
                raw.trim().is_empty()
            }
        "#;
        assert_eq!(names(src), vec!["is_clean"]);
    }

    /// parse / deserialize は汚染を通す (解釈の場を作らないため)。
    #[test]
    fn fires_through_deserialization() {
        let src = r#"
            fn meta_status_is_running(p: &Path) -> bool {
                let Ok(content) = std::fs::read_to_string(p) else { return false; };
                let Ok(meta) = serde_json::from_str::<Meta>(&content) else { return false; };
                meta.status.as_deref() == Some("running")
            }
        "#;
        assert_eq!(names(src), vec!["meta_status_is_running"]);
    }

    /// 分岐して literal を返す形は射程外 (§ 射程外)。stage の entry point が全部この形。
    #[test]
    fn does_not_fire_on_branch_then_literal() {
        let src = r#"
            fn run_check() -> bool {
                let raw = match run_jj_list() { Ok(o) => o, Err(_) => return true };
                let items = parse_items(&raw);
                if items.is_empty() {
                    return false;
                }
                true
            }
            fn run_jj_list() -> Result<String, String> {
                let (_, out) = std::process::Command::new("jj").output().unwrap();
                Ok(out)
            }
            fn parse_items(raw: &str) -> Vec<String> { raw.lines().map(str::to_string).collect() }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }

    /// I/O の成否だけを返す wrapper は発火しない。
    #[test]
    fn does_not_fire_on_io_success_wrapper() {
        let src = r#"
            fn write_marker(p: &Path, body: &str) -> bool {
                match std::fs::write(p, body) {
                    Ok(()) => true,
                    Err(e) => { log_info(&format!("{e}")); false }
                }
            }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }

    /// 注入 (closure を受ける同一ファイルの純関数) は望ましい形なので発火しない。
    #[test]
    fn does_not_fire_on_injection() {
        let src = r#"
            fn remote_ref_exists(revset: &str) -> bool {
                cached(revset, || std::process::Command::new("jj").spawn())
            }
            fn cached(key: &str, f: impl Fn() -> std::io::Result<Child>) -> bool { f().is_ok() }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }

    /// bool 以外の判定型は射程外 (thin wrapper の FP を避けるため)。
    #[test]
    fn ignores_non_bool_return_types() {
        let src = r#"
            fn read_meta(p: &Path) -> Option<Meta> {
                let s = std::fs::read_to_string(p).ok()?;
                serde_json::from_str(&s).ok()
            }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }

    /// `#[cfg(test)]` module 内は検査しない。
    #[test]
    fn skips_test_modules() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                fn helper() -> bool {
                    let (_, out) = run_cmd_direct("jj", &[], &[], 30);
                    out.trim() == "x"
                }
            }
        "#;
        assert!(names(src).is_empty(), "{:?}", names(src));
    }

    #[test]
    fn reports_the_function_line() {
        let findings = scan_rust_source(INCIDENT_490).expect("parse");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2, "{findings:?}");
    }

    #[test]
    fn unparsable_source_is_an_error() {
        assert!(scan_rust_source("fn broken( {").is_err());
    }
}
