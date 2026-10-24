//! 削除系コマンドを、消す対象のパスがどこにあるかで止める (順位 581、ADR-086)。
//!
//! 懸念は「削除系コマンドが、意図せずプロジェクトの外のファイル・フォルダを消す」こと。
//! 作業中に作った一時ファイルの削除は止めない。コマンドの種類ではなく、対象の場所で決める:
//!
//! | 場所 | 単一の削除 | 再帰の削除 |
//! |---|---|---|
//! | 一時ファイルの置き場 (このセッションの scratchpad、プロジェクト直下の `__*`) | 通す | 通す |
//! | プロジェクトの中 | 通す | 止める |
//! | このプロジェクトの Claude の memory ディレクトリ | 通す | 止める |
//! | プロジェクトの外 | 止める | 止める |
//! | 判定できない (外から来た変数・コマンド置換・パイプで渡された対象) | 確認する | 確認する |
//!
//! コマンドの中に書いたコードを実行させるもの (`node -e` / `eval` / `trap` / `find -exec` / sed の `e` /
//! awk の `system` / heredoc の中のコマンド置換) は、中身を読まずに確認する (ADR-086 決定 7)。
//!
//! 対象は Bash (`rm` / `rmdir` / `unlink` / `find -delete` / `rimraf`)、PowerShell
//! (`Remove-Item` と別名、.NET の `Delete`)、cmd (`rd` / `del`)、およびそれらを
//! `bash -c` / `pwsh -c` / `cmd /c` で包んだもの。
//!
//! 削除の機能を持つコマンドは数えきれないので、確認なしで実行してよいコマンドを [`allowlist`] に
//! 持ち、無いコマンドはユーザーに確認する ([`Inspection::unlisted`])。リストに載せたコマンドの
//! 削除の機能 (`git clean` / `cargo clean` / `mv` の移動元など) は [`commands`] が読む。
//!
//! **見えないものは止められない。** スクリプトファイルの中の削除 (`node x.js` の `fs.rmSync` や
//! `python -c` の `shutil.rmtree`) はコマンド文字列に現れないので、この層では捕まえられない。

mod allowlist;
mod commands;
mod lexer;
mod paths;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cargo;
#[cfg(test)]
mod tests_allowlist;
#[cfg(test)]
mod tests_review;
#[cfg(test)]
mod tests_heredoc;
#[cfg(test)]
mod tests_stop_parsing;
#[cfg(test)]
mod tests_powershell_call_paren;
#[cfg(test)]
mod tests_tool_caches;
#[cfg(test)]
mod tests_find;
#[cfg(test)]
mod tests_code_exec;

pub(crate) use lexer::Dialect;
use commands::{State, Target};
use paths::{classify, has_brace_expansion, normalize, strip_provider, Place, Roots};

/// 判定に使う場所の情報。hook の入力 (`cwd` / `scratchpad_dir`) と環境変数から作る。
#[derive(Clone, Debug, Default)]
pub(crate) struct Context {
    pub(crate) cwd: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) scratch: Option<String>,
    pub(crate) memory: Option<String>,
    pub(crate) home: Option<String>,
}

impl Context {
    /// プロジェクトのルートは `CLAUDE_PROJECT_DIR`、無ければ hook の `cwd`、無ければ
    /// プロセスの作業ディレクトリ。ホームは `HOME`、無ければ `USERPROFILE`。memory は
    /// transcript と同じディレクトリの `memory/`。
    pub(crate) fn from_hook(
        cwd: Option<&str>,
        scratchpad_dir: Option<&str>,
        transcript_path: Option<&str>,
    ) -> Self {
        let memory = transcript_path
            .and_then(|t| std::path::Path::new(t).parent())
            .map(|dir| dir.join("memory").to_string_lossy().into_owned());
        let process_cwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        let cwd = cwd.map(str::to_string).or(process_cwd);
        let project = std::env::var("CLAUDE_PROJECT_DIR").ok().or_else(|| cwd.clone());
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok();
        Context {
            cwd,
            project,
            scratch: scratchpad_dir.map(str::to_string),
            memory,
            home,
        }
    }
}

/// 止めた削除の対象 1 つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Violation {
    pub(crate) shown: String,
    pub(crate) place: Place,
    pub(crate) recursive: bool,
}

/// コマンドを調べた結果。`violations` は止める削除、`unlisted` は許可リストに無いので確認する
/// コマンドの名前 (名前を解決できなかったものは、書かれたままの語)。
#[derive(Debug, Default)]
pub(crate) struct Inspection {
    pub(crate) violations: Vec<Violation>,
    pub(crate) unlisted: Vec<String>,
    /// プロジェクトの外や判定できない場所への上書き (コピー・`ln -f`) の書き込み先。確認する。
    pub(crate) overwrites: Vec<String>,
    /// コマンドの中に書いたコードを実行させたもの (`node -e` / `python -c` / `eval` / `trap` /
    /// `find -exec` / heredoc の中のコマンド置換)。中身は読まずに確認する (ADR-086 決定 7)。
    pub(crate) inline_code: Vec<String>,
    /// どこを消すか判定できない削除の対象 (外から来た変数・パイプで渡された対象など)。確認する
    /// (ADR-086 決定 7)。
    pub(crate) undetermined: Vec<String>,
}

/// `command` の中の削除のうち、止めるべきものを返す。無ければ空。
#[cfg(test)]
pub(crate) fn violations(command: &str, dialect: Dialect, ctx: &Context) -> Vec<Violation> {
    inspect(command, dialect, ctx, &[]).violations
}

/// `command` を調べる。`extra_allowed` は設定 (`allowed_commands`) で許可リストに足したもの。
pub(crate) fn inspect(command: &str, dialect: Dialect, ctx: &Context, extra_allowed: &[String]) -> Inspection {
    let absolute = |p: &Option<String>| p.as_deref().and_then(|p| normalize(p, None, None));
    let roots = Roots {
        project: absolute(&ctx.project),
        scratch: absolute(&ctx.scratch),
        memory: absolute(&ctx.memory),
        home: absolute(&ctx.home),
    };
    let mut state = State::new(absolute(&ctx.cwd), roots.home.clone());
    commands::analyze(command, dialect, &mut state, 0);
    let (overwritten, deleted): (Vec<&Target>, Vec<&Target>) = state.targets.iter().partition(|t| t.overwrite);
    let mut overwrites: Vec<String> = overwritten
        .into_iter()
        .filter(|t| decide(place_of(t, &roots), false) != Decision::Allow)
        .map(|t| t.shown.clone())
        .collect();
    overwrites.extend(deleted.iter().filter_map(|t| moved_destination_overwrite(t, &roots)));
    let mut violations = Vec::new();
    let mut undetermined = Vec::new();
    for t in deleted {
        let place = place_of(t, &roots);
        let recursive = t.recursive || leaves_its_place(t, place, &roots);
        match decide(place, recursive) {
            Decision::Deny => violations.push(Violation { shown: t.shown.clone(), place, recursive }),
            Decision::Ask if !undetermined.contains(&t.shown) => undetermined.push(t.shown.clone()),
            _ => {}
        }
    }
    let mut inline_code = state.inline_code.clone();
    inline_code.dedup();
    let unlisted = unlisted_commands(&state, extra_allowed);
    Inspection { violations, unlisted, overwrites, inline_code, undetermined }
}

impl Inspection {
    /// 止めはしないが、実行の前にユーザーに確認するか。
    pub(crate) fn asks(&self) -> bool {
        !self.unlisted.is_empty() || !self.overwrites.is_empty() || !self.inline_code.is_empty() || !self.undetermined.is_empty()
    }
}

/// 許可リストに無いコマンドの名前を、現れた順に重複なく返す。同じコマンドの中で定義した関数は除く。
fn unlisted_commands(state: &State, extra_allowed: &[String]) -> Vec<String> {
    let listed = |(name, dialect): &&(String, Dialect)| {
        allowlist::is_allowed(name, *dialect, extra_allowed) || state.functions.contains(name)
    };
    let mut unlisted: Vec<String> = Vec::new();
    let invoked = state.invoked.iter().filter(|n| !listed(n)).map(|(name, _)| name);
    for name in invoked.chain(&state.unresolved) {
        if !unlisted.contains(name) {
            unlisted.push(name.clone());
        }
    }
    unlisted
}

/// 確認するときの理由 (permissionDecisionReason)。許可リストに無いコマンドと、プロジェクトの外への上書き。
pub(crate) fn ask_reason(found: &Inspection) -> String {
    let quoted = |items: &[String]| items.iter().take(5).map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
    let mut parts = Vec::new();
    if !found.unlisted.is_empty() {
        parts.push(unlisted_reason(&quoted(&found.unlisted)));
    }
    if !found.inline_code.is_empty() {
        parts.push(format!(
            "コマンドの中に書いたコードを実行します: {}。中身の削除は読めないので確認します (ADR-086)。",
            quoted(&found.inline_code)
        ));
    }
    if !found.overwrites.is_empty() {
        parts.push(format!(
            "プロジェクトの外か、場所を判定できないところへ書き込みます: {}。既にあるファイルは上書きされて元の中身が消えます (ADR-086)。",
            quoted(&found.overwrites)
        ));
    }
    if !found.undetermined.is_empty() {
        parts.push(format!(
            "どこを消すか判定できない削除があります: {}。外から来た変数・コマンド置換・パイプで渡された対象などは場所を読めないので確認します (ADR-086)。",
            quoted(&found.undetermined)
        ));
    }
    parts.join("\n")
}

/// 許可リストに無いコマンドを確認する理由の文。
fn unlisted_reason(names: &str) -> String {
    format!(
        "許可リストに無いコマンドを実行します: {}。削除の機能を持つコマンドを確認なしで実行しないための確認です \
         (ADR-086)。よく使うなら、削除の経路を確かめてから hooks-config.toml の \
         [pre_tool_validate] allowed_commands に足してください。",
        names
    )
}

/// 移動元を、別の種類の場所 (プロジェクトの中から外や scratchpad へ) に移すか。移すなら元の場所から
/// 消えるのと同じなので、再帰の削除として扱う。移動先が分からなければ移すものとする。
fn leaves_its_place(target: &Target, place: Place, roots: &Roots) -> bool {
    let Some(destination) = &target.moved_to else {
        return false;
    };
    let moved = Target { path: destination.clone(), glob: false, moved_to: None, ..target.clone() };
    place_of(&moved, roots) != place
}

/// 移動 (`mv` / `Move-Item` / cmd の `move`) の移動先。既にあるファイルを無条件に置き換えるが、
/// 移動元 (削除として扱う対象) とは別物なので、[`leaves_its_place`] と同じ要領で移動先だけの対象を
/// 作り、移動元とは独立に上書き先として判定する。移動元が scratchpad に分類されると [`blocks`] が
/// `recursive` を無視するため ([`Place::Scratch`] の分岐)、移動元側の保護だけでは `mv
/// <scratchpad>/x ~/.ssh/authorized_keys` のような移動先の上書きを守れない
/// (SEC-NEW-mod-rs-L222)。移動先が分からなければ、どこを上書きするか分からないので確認する
/// (移動元が scratchpad だと、移動元の側では止まらない)。
fn moved_destination_overwrite(target: &Target, roots: &Roots) -> Option<String> {
    let Some(destination) = &target.moved_to else { return None };
    let Some(destination) = destination else {
        return Some(format!("{} の移動先 (どこか分からない)", target.shown));
    };
    let moved = Target {
        shown: destination.clone(),
        path: Some(destination.clone()),
        glob: false,
        moved_to: None,
        overwrite: true,
        ..target.clone()
    };
    let place = place_of(&moved, roots);
    (decide(place, false) != Decision::Allow).then_some(moved.shown)
}

/// 削除の対象の場所 (正規化してから分類する。解決できなければ判定できない)。
fn place_of(target: &Target, roots: &Roots) -> Place {
    let Some(path) = target.path.as_deref() else {
        return Place::Unknown;
    };
    if target.glob && has_brace_expansion(path) {
        return Place::Unknown;
    }
    let path = match strip_provider(path) {
        Ok(p) => p,
        Err(place) => return place,
    };
    match normalize(path, target.cwd.as_deref(), roots.home.as_deref()) {
        Some(normalized) => classify(&normalized, target.glob, roots),
        None => Place::Unknown,
    }
}

/// 削除 1 つの扱い。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Allow,
    Ask,
    Deny,
}

/// 場所と再帰かどうかから、扱いを決める (ADR-086 決定 1 の表と決定 7)。上書きは `recursive` を
/// false にして、通す以外なら確認する。
fn decide(place: Place, recursive: bool) -> Decision {
    match place {
        Place::Outside => Decision::Deny,
        Place::Unknown => Decision::Ask,
        Place::Project | Place::Memory if recursive => Decision::Deny,
        Place::Project | Place::Memory | Place::Scratch | Place::NotFile => Decision::Allow,
    }
}

/// 止めたときに stderr へ出すメッセージ。
pub(crate) fn message(found: &[Violation], ctx: &Context) -> String {
    let lines: Vec<String> = found
        .iter()
        .take(5)
        .map(|v| format!("- `{}` — {}", v.shown, reason(v)))
        .collect();
    let scratch = ctx.scratch.as_deref().unwrap_or("(このセッションの scratchpad)");
    format!(
        "**削除系コマンドがブロックされました**\n\n\
         {}\n\n\
         削除系コマンドが、意図せずプロジェクトの外のファイル・フォルダを消すのを防ぐための検査です (ADR-086)。\n\n\
         **通る削除:**\n\
         - このセッションの scratchpad の中: `{scratch}`\n\
         - プロジェクト直下の `__` で始まる一時ファイル・フォルダ\n\
         - プロジェクトの中と、このプロジェクトの Claude の memory の、単一ファイルの削除 (再帰でないもの)\n\n\
         **対処:**\n\
         - 一時ファイルは scratchpad か `__` で始まる名前で作り、そのパスを直接書いて消す\n\
         - 変数を使うなら、同じコマンドの中で文字列を代入する (`S=\"<path>\"; rm -rf \"$S/x\"`)\n\
         - それ以外の削除が本当に必要なら、ユーザーに確認して手元の端末で実行してもらう",
        lines.join("\n")
    )
}

/// 止めた対象 1 つの理由の文。
fn reason(v: &Violation) -> &'static str {
    match (v.place, v.recursive) {
        (Place::Outside, _) => "プロジェクトの外",
        (Place::Project, _) => "プロジェクトの中の再帰削除 (一時ファイルの置き場以外)",
        (Place::Memory, _) => "Claude の memory の再帰削除 (単一ファイルの削除は通る)",
        _ => "止める対象",
    }
}
