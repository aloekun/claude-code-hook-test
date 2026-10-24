//! `=` を使わずに変数の値を変えるコマンド (`for x in …` / `read x` / `unset x` / PowerShell の
//! `foreach ($x in …)` / `Remove-Variable x` など)。変わる変数は値が分からないので不明にする
//! (CodeRabbit の指摘: `d=<scratch>/x; for d in /outside; do rm -rf "$d"; done` を scratch の中の削除と読んでいた)。

use super::super::lexer::{Dialect, Segment, Word};
use super::State;

/// `name` が変数の値を変えるコマンドなら、変わる変数を不明 (`None`) にする。
pub(crate) fn forget_rebound_variables(name: &str, args: &[Word], segment: &Segment, dialect: Dialect, state: &mut State) {
    let lits: Vec<&str> = args.iter().map(Word::leading_lit).collect();
    let names = match dialect {
        Dialect::Bash => bash_rebound_names(name, &lits, state),
        Dialect::PowerShell => powershell_rebound_names(name, &lits, segment),
        Dialect::Cmd => Vec::new(),
    };
    for var in names {
        state.vars.insert(State::key(&var, dialect), None);
    }
}

/// Bash で値が変わる変数の名前。
fn bash_rebound_names(name: &str, lits: &[&str], state: &mut State) -> Vec<String> {
    match name {
        "for" | "select" => lits.first().filter(|v| !v.starts_with("((")).map(|v| vec![v.to_string()]).unwrap_or_default(),
        "read" => names_after_options(lits, &["-a", "-d", "-i", "-n", "-N", "-p", "-t", "-u"], &["-a"]),
        "unset" => names_after_options(lits, &[], &[]),
        "mapfile" | "readarray" => {
            let names = names_after_options(lits, &["-d", "-n", "-O", "-s", "-u", "-C", "-c"], &[]);
            vec![names.last().cloned().unwrap_or_else(|| "MAPFILE".to_string())]
        }
        "getopts" => lits.get(1).map(|v| vec![v.to_string()]).unwrap_or_default(),
        "printf" => names_after_options(lits, &["-v"], &["-v"]).into_iter().take(1).collect(),
        "local" | "declare" | "typeset" => bash_declare_rebound_names(lits, state),
        _ => Vec::new(),
    }
}

/// `declare`/`local`/`typeset` で値が変わる変数名。`NAME=value` の形は呼び出し元の `record_assignment`
/// が直接扱う (値を記録できる) ので、ここで二重に不明化しない。
///
/// nameref (`-n` を含むオプション、`declare -rn ref=S` のようにまとめた形も) を宣言したら、以後は
/// どの変数の値も追わない ([`State::vars_untracked`]、ADR-086 決定 7)。nameref への書き込みは
/// 結んだ実体の変数を書き換えるので、結び先を個別に追っていた頃は、2 文に分けた宣言や実体への
/// 入れ直しを読み落として確認なしで通していた (CodeRabbit 指摘、SIM-NEW-rebinds-rs-L45)。
fn bash_declare_rebound_names(lits: &[&str], state: &mut State) -> Vec<String> {
    let nameref = lits.iter().any(|l| *l == "--nameref" || (l.starts_with(['-', '+']) && !l.starts_with("--") && l.contains('n')));
    if nameref {
        state.vars_untracked = true;
    }
    names_after_options(lits, &[], &[]).into_iter().filter(|n| !n.contains('=')).collect()
}

/// PowerShell で値が変わる変数の名前。`foreach ($x in …)` の `(…)` は segment のコマンド置換に入る。
fn powershell_rebound_names(name: &str, lits: &[&str], segment: &Segment) -> Vec<String> {
    match name {
        "foreach" | "for" => segment
            .substitutions
            .iter()
            .filter_map(|body| body.trim_start().strip_prefix('$'))
            .map(|rest| rest.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':')).collect::<String>())
            .filter(|n| !n.is_empty())
            .collect(),
        "remove-variable" | "rv" | "clear-variable" | "clv" | "set-variable" | "sv" | "new-variable" | "nv" => {
            let named = lits.iter().position(|l| l.len() >= 2 && "-name".starts_with(&l.to_lowercase()));
            match named {
                Some(i) => lits.get(i + 1).map(|v| vec![v.to_string()]).unwrap_or_default(),
                None => lits.iter().find(|l| !l.starts_with('-')).map(|v| vec![v.to_string()]).unwrap_or_default(),
            }
        }
        _ => Vec::new(),
    }
}

/// オプションを除いた位置引数。`takes_value` のオプションは値ごと読み飛ばし、そのうち `value_is_name`
/// のオプションの値 (`read -a arr` の `arr`) は名前として返す。
fn names_after_options(lits: &[&str], takes_value: &[&str], value_is_name: &[&str]) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0;
    while let Some(lit) = lits.get(i) {
        if takes_value.contains(lit) {
            if value_is_name.contains(lit) {
                names.extend(lits.get(i + 1).map(|v| v.to_string()));
            }
            i += 2;
            continue;
        }
        if !lit.starts_with('-') {
            names.push(lit.to_string());
        }
        i += 1;
    }
    names
}
