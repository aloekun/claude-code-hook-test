//! 実台帳の検査が使う**索引**を組み立てる層。
//!
//! 宣言先 (`declared_text`) と リポジトリ索引 (`repository_text`) の 2 つを作る。
//! 何を落とすかの判断は [`crate::rust_source`]、識別子の分類は [`crate::identifiers`]。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::deployed_ledger::repo_root;

/// リポジトリ索引から外すディレクトリ。ビルド生成物・VCS 内部・実行ログは実体ではない。
pub(crate) const INDEX_SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".jj", ".takt", "docs"];

/// リポジトリ索引に入れる拡張子。**`.md` は入れない** — 台帳と todo 自身が識別子を
/// 名指しているため、含めると「これから作る識別子」まで「リポジトリに在る」= 漂流と読む
/// (2026-08-25 実測: 除外前は 3 件すべてが漂流に誤分類された)。
pub(crate) const INDEX_EXTENSIONS: &[&str] = &["rs", "toml", "mjs", "ts", "yml", "sh"];

pub(crate) fn has_indexed_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| INDEX_EXTENSIONS.contains(&e))
}

/// 索引に載せる形へ整えた 1 ファイル分のテキスト。
///
/// `.rs` は**テストコードとコメントを落とす** ([`crate::rust_source`])。丸ごと連結すると
/// doc コメントの例示や `#[cfg(test)]` の識別子まで「リポジトリに在る」と読み、漂流判定が
/// 誤検出・見逃しの両方向へ壊れる (F3)。それ以外の拡張子はそのまま使う。
pub(crate) fn indexable_text(path: &Path, text: &str) -> String {
    if path.extension().and_then(|e| e.to_str()) == Some("rs") {
        crate::rust_source::production_code(text)
    } else {
        text.to_string()
    }
}

/// ディレクトリ配下のファイル内容を連結して読む。読めないファイルは飛ばす。
///
/// `excluded` に含まれるファイルは丸ごと飛ばす — [`cfg_test_only_files`] が解決した、
/// 親ファイル側の `#[cfg(test)] mod name;` で丸ごとテスト扱いになるファイル群。
/// [`indexable_text`] は 1 ファイル単体しか見えないため、この判定はできない
/// (SIM-NEW-lib-ledger-rust_source-L75)。
pub(crate) fn concat_files(root: &Path, indexed_only: bool, out: &mut String, excluded: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !INDEX_SKIP_DIRS.contains(&name.as_ref()) {
                concat_files(&path, indexed_only, out, excluded);
            }
        } else if (!indexed_only || has_indexed_extension(&path)) && !excluded.contains(&path) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&indexable_text(&path, &text));
                out.push('\n');
            }
        }
    }
}

/// リポジトリ中の `#[cfg(test)]` 外部 module 宣言が指すファイルを、実ファイルパスへ解決して集める。
///
/// 宣言は宣言元ファイルのディレクトリを基準に解決する (Rust の module 解決規則)。
/// 解決先が実在しなければ捨てる — 壊れた宣言や解釈ミスを誤って除外に混ぜないため。
pub(crate) fn cfg_test_only_files(root: &Path, out: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !INDEX_SKIP_DIRS.contains(&name.as_ref()) {
                cfg_test_only_files(&path, out);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for decl in crate::rust_source::cfg_test_module_declarations(&text) {
            if let Some(resolved) = resolve_cfg_test_module_path(&path, &decl) {
                out.insert(resolved);
            }
        }
    }
}

/// `#[cfg(test)]` 外部 module 宣言 1 件が指す実ファイルへのパスを解決する。
///
/// 基準ディレクトリは Rust の module 解決規則に従う (CodeRabbit #457):
///
/// | 宣言元 | 子 module の探索先 |
/// |---|---|
/// | crate root (`lib.rs` / `main.rs`) / `mod.rs` | 同じディレクトリ |
/// | それ以外の `foo.rs` | `foo/` |
///
/// `src/a/b.rs` の `mod tests;` は `src/a/tests.rs` ではなく **`src/a/b/tests.rs`** を指す。
/// ここを取り違えると解決に失敗し、テスト専用ファイルが索引へ混入する
/// (実測: `stages/bookmark_check/tests.rs` のヘルパーが索引に載っていた)。
///
/// `#[path = ".."]` は例外で、**宣言元ファイルのディレクトリ**からの相対で解決する
/// (Rust reference: inline module 内でない `path` 属性はソースファイルのディレクトリ基準)。
pub(crate) fn resolve_cfg_test_module_path(
    declaring_file: &Path,
    decl: &crate::rust_source::CfgTestModuleDecl,
) -> Option<PathBuf> {
    let dir = declaring_file.parent()?;
    let candidate = match &decl.path {
        Some(path) => dir.join(path),
        None => {
            let module_dir = child_module_dir(declaring_file)?;
            let flat = module_dir.join(format!("{}.rs", decl.name));
            if flat.is_file() {
                flat
            } else {
                module_dir.join(&decl.name).join("mod.rs")
            }
        }
    };
    candidate.is_file().then_some(candidate)
}

/// 宣言元ファイルの子 module が置かれるディレクトリ (I/O なし)。
fn child_module_dir(declaring_file: &Path) -> Option<PathBuf> {
    let dir = declaring_file.parent()?;
    let stem = declaring_file.file_stem()?.to_str()?;
    if matches!(stem, "lib" | "main" | "mod") {
        Some(dir.to_path_buf())
    } else {
        Some(dir.join(stem))
    }
}

/// 宣言パス群の中身を連結して読む (ディレクトリ宣言はその配下すべて)。
///
/// **宣言先はテストコードも数える。** 台帳には「検査を足す」型のタスクがあり、成果物その
/// ものが `#[cfg(test)]` の中に在る (順位 457 = `rule_test_coverage_check` の拡張が実例)。
/// 索引側 ([`repository_text`]) と扱いを変えるのは、問う内容が違うため:
///
/// | 層 | 問い | テストコードの扱い |
/// |---|---|---|
/// | 宣言先 | 宣言した成果物がそのファイルに在るか | **数える** (成果物がテストのことがある) |
/// | 索引 | その識別子がリポジトリの他所に在るか | **数えない** (言及と実体を区別する) |
pub(crate) fn declared_text(paths: &[String]) -> String {
    let root = repo_root();
    let mut text = String::new();
    for relative in paths {
        let path = root.join(relative);
        if path.is_dir() {
            concat_files_verbatim(&path, &mut text);
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            text.push_str(&content);
            text.push('\n');
        }
    }
    text
}

/// ディレクトリ配下を**そのまま**連結する (宣言先用。テストコードも数える)。
pub(crate) fn concat_files_verbatim(root: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !INDEX_SKIP_DIRS.contains(&name.as_ref()) {
                concat_files_verbatim(&path, out);
            }
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

/// リポジトリ索引から除く、`#[cfg(test)]` 外部 module 宣言が指すファイルの集合。
pub(crate) fn repository_index_exclusions() -> HashSet<PathBuf> {
    let mut excluded = HashSet::new();
    cfg_test_only_files(&repo_root(), &mut excluded);
    excluded
}

/// リポジトリ全体のコードを 1 本の文字列として索引する。
pub(crate) fn repository_text() -> String {
    let mut text = String::new();
    concat_files(&repo_root(), true, &mut text, &repository_index_exclusions());
    text
}

/// 素の索引 (テストコード・コメント込み)。probe の比較対象。
pub(crate) fn raw_repository_text() -> String {
    let mut text = String::new();
    concat_files_raw(&repo_root(), &mut text);
    text
}

pub(crate) fn concat_files_raw(root: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !INDEX_SKIP_DIRS.contains(&name.as_ref()) {
                concat_files_raw(&path, out);
            }
        } else if has_indexed_extension(&path) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 非ルート module (`foo.rs`) の子は `foo/` の下 (CodeRabbit #457)。
    /// 取り違えると `stages/bookmark_check/tests.rs` のようなテスト専用ファイルが索引へ混入する。
    #[test]
    fn a_non_root_module_resolves_children_under_its_own_directory() {
        assert_eq!(
            child_module_dir(Path::new("src/a/b.rs")),
            Some(PathBuf::from("src/a/b"))
        );
    }

    /// crate root と `mod.rs` の子は同じディレクトリ。
    #[test]
    fn crate_roots_and_mod_files_resolve_children_beside_themselves() {
        for file in ["src/lib.rs", "src/main.rs", "src/a/mod.rs"] {
            let expected = Path::new(file).parent().unwrap().to_path_buf();
            assert_eq!(child_module_dir(Path::new(file)), Some(expected), "{file}");
        }
    }

    /// **実リポジトリで解決できること**を固定する。`bookmark_check.rs` の `mod tests;` は
    /// `bookmark_check/tests.rs` に在り、この解決が外れると索引汚染が戻る。
    #[test]
    fn the_repository_index_excludes_a_non_root_modules_test_file() {
        let index = repository_text();
        assert!(
            !index.contains("fn parent_without_bookmarks"),
            "非ルート module のテストファイルが索引に載っています"
        );
    }
}
