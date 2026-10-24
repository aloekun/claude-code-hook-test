//! コマンド検証フック (設定駆動型)
//!
//! Bash / PowerShell コマンド実行前に危険なコマンドをブロックします。
//! .claude/hooks-config.toml からプリセット選択・追加保護ファイルを読み込みます。
//!
//! 終了コード:
//!   0 - コマンドを許可
//!   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
//!
//! MIT License - based on xiaobei930/claude-code-best-practices

use serde::Deserialize;
use std::io::{self, Read};
use std::process::ExitCode;

mod blocked_patterns;
mod config;
mod deletion_guard;
mod handlers;
mod presets;
mod protected_files;
#[cfg(test)]
mod settings_wiring;
mod todo_staleness;

use config::{load_config, Config};
use handlers::{handle_bash_tool, handle_powershell_tool, handle_write_edit_tool};

/// `main` が振り分けるツール名 (下の `match` の腕と同じ集合)。
///
/// settings の PreToolUse matcher がこのどれかを欠くと、そのツールは本 hook を素通りする。
/// PowerShell は handler だけ先に入り (順位 212、#215)、matcher への登録が漏れたまま
/// 2026-10-20 まで一度も呼ばれなかった。`settings_wiring` のテストが settings と突き合わせる。
#[cfg(test)]
pub(crate) const HANDLED_TOOLS: [&str; 5] = ["Bash", "PowerShell", "Write", "Edit", "Replace"];

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
    /// セッションの作業ディレクトリ。削除の対象の相対パスを解決する起点 (deletion_guard)。
    cwd: Option<String>,
    /// このセッションの scratchpad。中の削除は一時ファイルの削除として通す (deletion_guard)。
    scratchpad_dir: Option<String>,
    /// transcript のパス。同じディレクトリの `memory/` を Claude の memory として扱う (deletion_guard)。
    transcript_path: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ToolInput {
    pub(crate) command: Option<String>,
    pub(crate) file_path: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) old_string: Option<String>,
    pub(crate) new_string: Option<String>,
    pub(crate) content: Option<String>,
}

fn read_hook_input() -> Result<HookInput, ExitCode> {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[validate-command] Error: Failed to read stdin: {}", e);
        return Err(ExitCode::FAILURE);
    }
    serde_json::from_str(&input).map_err(|e| {
        eprintln!("[validate-command] Error: Failed to parse JSON: {}", e);
        ExitCode::FAILURE
    })
}

fn main() -> ExitCode {
    let config = load_config();
    let hook_input = match read_hook_input() {
        Ok(v) => v,
        Err(code) => return code,
    };
    let tool_name = hook_input.tool_name.unwrap_or_default();
    let tool_input = hook_input.tool_input.unwrap_or(ToolInput {
        command: None,
        file_path: None,
        path: None,
        old_string: None,
        new_string: None,
        content: None,
    });
    let places = deletion_guard::Context::from_hook(
        hook_input.cwd.as_deref(),
        hook_input.scratchpad_dir.as_deref(),
        hook_input.transcript_path.as_deref(),
    );
    dispatch_tool(&tool_name, &config, &tool_input, &places)
}

/// ツール名で振り分けて実行する。`deletion_guard` の解析 (未知の入力でも再帰で潰れないよう
/// [`deletion_guard::lexer::scan`] に深さの上限を設けたが、ほかの未知の panic にも備えて)
/// `catch_unwind` で受け止め、`Err` を「判定できない対象は止める」(ADR-043 fail-closed) に合わせて
/// ブロック (exit code 2) にする。これが無いとクラッシュは exit code 2 以外で終了し、PreToolUse は
/// ブロックと見なさずフェイルオープンになる (SEC-NEW-scan-rs-L72)。
fn dispatch_tool(tool_name: &str, config: &Config, tool_input: &ToolInput, places: &deletion_guard::Context) -> ExitCode {
    let run = || match tool_name {
        "Bash" => handle_bash_tool(config, tool_input, places),
        "Write" | "Edit" | "Replace" => handle_write_edit_tool(config, tool_input),
        "PowerShell" => handle_powershell_tool(config, tool_input, places),
        _ => ExitCode::SUCCESS,
    };
    std::panic::catch_unwind(run).unwrap_or_else(|_| {
        eprintln!(
            "**ツール呼び出しがブロックされました**\n\n\
             検査が途中で異常終了しました。判定できない呼び出しは止めます (ADR-043)。\n\n\
             コマンドや入力を簡略化するか、ユーザーに確認して手元の端末で実行してください。"
        );
        ExitCode::from(2)
    })
}
