//! コマンド検証フック (設定駆動型)
//!
//! Bashコマンド実行前に危険なコマンドをブロックします。
//! .claude/hooks-config.toml からプリセット選択・追加保護ファイルを読み込みます。
//!
//! 終了コード:
//!   0 - コマンドを許可
//!   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
//!
//! MIT License - based on xiaobei930/claude-code-best-practices

use regex::Regex;
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
    file_path: Option<String>,
    path: Option<String>,
    old_string: Option<String>,
    new_string: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct Config {
    pre_tool_validate: Option<PreToolValidateConfig>,
}

#[derive(Deserialize, Default)]
struct PreToolValidateConfig {
    blocked_patterns: Option<Vec<String>>,
    extra_protected_files: Option<Vec<String>>,
    todo_staleness: Option<TodoStalenessConfig>,
}

/// 順位 136 案 B: `docs/todo*.md` Edit/Write 時の staleness 検知 + 既実装 grep 提示。
/// ADR-039 experimental pattern 準拠 (default-OFF in source、repo config で明示 enable)。
/// fail-closed (lineage 判定不能 = stale 扱いで安全側) per entry 設計決定。
#[derive(Deserialize, Default)]
struct TodoStalenessConfig {
    enabled: Option<bool>,
    default_branch: Option<String>,
    grep_recent_limit: Option<u64>,
}

const TODO_STALENESS_DEFAULT_BRANCH: &str = "master";
const TODO_STALENESS_DEFAULT_GREP_LIMIT: u64 = 20;
const TODO_STALENESS_JJ_TIMEOUT_SECS: u64 = 5;

// --- ブロックパターン ---

struct BlockedPattern {
    pattern: Regex,
    /// 順位 144 (PR #171 T3-#8 採用): pattern match 後にこの regex が hit する場合は allow。
    /// Rust 標準 regex crate は negative lookahead 非対応のため 2 段判定で「pattern match
    /// AND exception 不一致」の semantic を実現する。`None` の場合は従来通り pattern match で block。
    exception: Option<Regex>,
    message: &'static str,
}

/// プリセット: default (rm -rf, cd /d)
fn preset_default() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r)\s").unwrap(),
            exception: None,
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*(\s|$)").unwrap(),
            exception: None,
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*(\s|$)").unwrap(),
            exception: None,
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?im)(^|&&|;|\|\||\||&)\s*cd\s+/d\s").unwrap(),
            exception: None,
            message: r#"**cd /d コマンドがブロックされました**

`cd /d` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: `cd <path>`
- または絶対パスでコマンドを実行してください

**例:**
```
# NG: cd /d e:\work\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
```"#,
        },
    ]
}

/// プリセット: git (直接 + シェルラッパー経由)
fn preset_git() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgit\s+"#).unwrap(),
            exception: None,
            message: r#"**git コマンドがブロックされました（シェルラッパー経由）**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
`bash -c 'git ...'` 等のラッパー経由でも git コマンドは使用できません。

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*git(?:\s+|$)"#).unwrap(),
            exception: None,
            message: r#"**git コマンドがブロックされました**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
git コマンドを直接使用すると、バージョン履歴に不整合が生じる可能性があります。

**jj コマンドの代替:**
| git コマンド | jj コマンド |
|-------------|------------|
| git status | jj status |
| git log | jj log |
| git diff | jj diff |
| git add + commit | jj describe -m "message" && jj new |
| git push | jj git push |
| git fetch | jj git fetch |

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
    ]
}

/// プリセット: jj-immutable
fn preset_jj_immutable() -> Vec<BlockedPattern> {
    vec![BlockedPattern {
        pattern: Regex::new(r"(?is)\bjj\b.*--ignore-immutable").unwrap(),
        exception: None,
        message: r#"**jj --ignore-immutable がブロックされました**

immutable commits（main 等）の書き換え保護を無効化するオプションのため、使用が禁止されています。

immutable commits を変更する必要がある場合は、ユーザーに確認を取ってください。"#,
    }]
}

/// プリセット: jj-main-guard (jj new main / jj edit main)
fn preset_jj_main_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+new|pnpm\s+jj-new)\s+(?:"main"|'main'|main)(?:\s|$)"#)
                .unwrap(),
            exception: None,
            message: r#"**jj new main がブロックされました**

ローカルの main ブックマークをベースに change を作成することは禁止されています。
ローカル main はリモートより古い可能性があり、先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main を fetch してから新しい change を作成します。"#,
        },
        BlockedPattern {
            pattern: Regex::new(
                r#"(?i)(jj\s+edit|pnpm\s+jj-edit)\s+(?:"main"|'main'|main)(?:\s|$)"#,
            )
            .unwrap(),
            exception: None,
            message: r#"**jj edit main がブロックされました**

main ブックマークが指す commit を直接編集することは禁止されています。
編集すると main の内容が変わり、履歴の破損や先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main をベースに新しい change を作成します。"#,
        },
    ]
}

/// プリセット: electron (Electron GUI 実行ブロック)
fn preset_electron() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|\s)(npm\s+(run\s+)?start|electron\b|npx\s+electron|yarn\s+start|npm\s+run\s+test:e2e:electron|pnpm\s+(run\s+)?start|pnpm\s+(run\s+)?test:e2e:electron)(\s|$)").unwrap(),
            exception: None,
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |
| ビルド確認 | npm run build |
| 開発サーバー (Renderer) | npm run dev |

**Note:** npm run start や npm run test:e2e:electron はユーザー環境でのみ実行可能です。

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\b(npx|pnpm\s+exec)\s+playwright\s+test\b.*\belectron\b").unwrap(),
            exception: None,
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
    ]
}

/// プリセット: jj-push-guard (jj git push / jj push を禁止し pnpm push に誘導)
fn preset_jj_push_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+git\s+push(\s|$)"#).unwrap(),
            exception: None,
            message: r#"**jj git push がブロックされました**

直接の push は禁止されています。push 前パイプライン（テスト・レビュー）を通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+push(\s|$)"#).unwrap(),
            exception: None,
            message: r#"**jj push がブロックされました**

`jj push` は非推奨です。代わりに `jj git push` を使用しますが、
直接の push は禁止されています。push 前パイプラインを通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#,
        },
    ]
}

/// プリセット: gh-pr-create-guard (gh pr create を禁止し pnpm create-pr に誘導)
fn preset_gh_pr_create_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+create(\s|$)"#).unwrap(),
            exception: None,
            message: r#"**gh pr create がブロックされました**

PR 作成は pnpm create-pr 経由で行ってください。
pnpm create-pr は PR 作成後に CI・CodeRabbit の自動監視も開始します。

**代わりに以下を実行してください:**
```
pnpm create-pr -- --title "タイトル" --body "本文"
```

-- 以降の引数はそのまま gh pr create に転送されます。"#,
        },
    ]
}

/// プリセット: polling-anti-pattern (rate-limit 浪費を招く polling ループを禁止)
///
/// 検出対象:
///   - `until <cond>; do ... sleep N ... done` (条件達成までの polling)
///   - `while ! <cond>; do ... sleep N ... done` (条件達成までの polling、while 版)
///
/// 動機: 同一セッション内で `run_in_background: true` の Bash 起動直後に
/// `until ... sleep` で polling する pattern が頻発し、Claude Code Max (5x) の
/// レートリミットを 1 時間で 40% 浪費した実例がある (PR #86)。
/// 背景タスクは task-notification ベースで自走するため polling は不要。
fn preset_polling_anti_pattern() -> Vec<BlockedPattern> {
    let msg = r#"**Polling ループがブロックされました**

`until ... sleep` / `while ! ... sleep` 形式の polling は、Claude Code の
レートリミットを大量に消費するため禁止されています (1 セッションで 40% 浪費の実例あり)。

**代替手段:**
| 用途 | 推奨方法 |
|------|---------|
| 背景タスクの完了待機 | `run_in_background: true` で起動 → task-notification 経由で自動通知される |
| ログ/イベントのストリーミング | `Monitor` tool を使用 (until ループ不要) |
| 状態の単発確認 | `gh pr view --json` 等の構造化データ取得を 1 回だけ実行 |
| 長時間プロセス | `run_in_background: true` で起動し、完了通知を待つ |

**設計原則:** Claude Code の background task と task-notification はイベント駆動で
完了通知を配信する。polling は token を浪費するだけで何も加速しない。

詳細: ADR-018 (post-pr-monitor は daemon + state file で自走) を参照。"#;
    // \bdo\b 制約により以下の false positive を排除:
    //   - echo "wait until ready"; sleep 1  (string 中の until)
    //   - git log --until=yesterday; sleep 1  (フラグ引数の until)
    //   - コメント / 文字列に until/while を含むスクリプト
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?is)\buntil\b.*?\bdo\b.*?\bsleep\s+\d").unwrap(),
            exception: None,
            message: msg,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?is)\bwhile\s+!\s.*?\bdo\b.*?\bsleep\s+\d").unwrap(),
            exception: None,
            message: msg,
        },
    ]
}

/// プリセット: exe-help-block (本リポジトリの Rust 製 exe + 単独 --help/-h/? をブロック)
///
/// 動機: PR #109 SIGPIPE 事故の直接トリガは `cli-merge-pipeline.exe --help` を AI が
/// 打ったこと。本リポジトリの Rust 製 exe (`.claude/*.exe`) は `--help` を未実装のため、
/// 実行すると help を表示せず実体 (例: cli-merge-pipeline は merge 本体) が即座に起動する。
/// `| head -40` 等の出力 truncate と相互作用して SIGPIPE で abrupt 終了 → Drop guard 不発 →
/// `.failed` marker 未生成 → ADR-030 仕様違反、という連鎖の起点。
///
/// 設計:
/// - `<path-prefix>?<name>.exe` + 単独 `--help|-h|/?` (subcommand 形式 `exe foo --help` は対象外)
/// - 引数 `--version` 等は block 対象外 (本 preset の責務は --help 系の trigger のみ)
/// - 順位 65 (PR #109 post-merge-feedback 採用、Bundle c)
fn preset_exe_help_block() -> Vec<BlockedPattern> {
    let msg = r#"**exe + --help がブロックされました**

本リポジトリの Rust 製 exe (`.claude/*.exe`) は `--help` を未実装のため、
実行すると help を表示せず実体が起動します (PR #109 SIGPIPE 事故の直接トリガ)。

**代替経路 — exe の使い方を確認するには:**
- 引数定義の Read: `src/<exe-name>/src/main.rs` (clap struct または手動パースを確認)
- 既存 docs を検索: `grep -r "<exe-name>" docs/`

**例:**
```
# NG: cli-merge-pipeline.exe --help
# NG: .claude/cli-merge-pipeline.exe -h
# OK: Read src/cli-merge-pipeline/src/main.rs
# OK: grep -r cli-merge-pipeline docs/
```

詳細: ADR-030 (SIGPIPE 事故の根因と Drop guard / reaper による recovery 機構)。"#;
    vec![BlockedPattern {
        pattern: Regex::new(
            r#"(?im)(^|&&|;|\|\||\||&|\n)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*(?:\S*?[/\\])?(?:cli-[\w-]+|hooks-[\w-]+|check-ci-[\w-]+)\.exe\s+(?:--help|-h|/\?)(\s|$)"#,
        )
        .unwrap(),
        exception: None,
        message: msg,
    }]
}

/// プリセット: gh-pr-merge-guard (gh pr merge を禁止し pnpm merge-pr に誘導)
fn preset_gh_pr_merge_guard() -> Vec<BlockedPattern> {
    let msg = r#"**gh pr merge がブロックされました**

PR マージは pnpm merge-pr 経由で行ってください。
pnpm merge-pr は PR のマージに加え、ローカル環境の同期も自動で行います。

**代わりに以下を実行してください:**
```
pnpm merge-pr
```

現在のブックマークから PR を自動検出してマージします。"#;
    vec![
        // 直接実行パターン
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+merge(\s|$)"#).unwrap(),
            exception: None,
            message: msg,
        },
        // シェルラッパー経由パターン (bash -c 'gh pr merge ...')
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgh\s+(?:.*\s+)?pr\s+merge"#).unwrap(),
            exception: None,
            message: msg,
        },
    ]
}

/// プリセット: jj-message-required (jj new / jj split を `-m` / `--message` なしで block)
///
/// 順位 144 (PR #171 T3-#8 採用): PR #171 セッションで `jj new` 忘れによる混合 commit 事故を
/// 実観測したのを契機に、message 必須化を機械強制する mechanical enforcement 層を導入。
/// memory rule `feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で
/// Claude 判断介入を排除。
///
/// 設計判断 (2026-05-24 ユーザー承認済):
/// - A: `jj new` 引数なしも block (= `-m` を強制)
/// - B: `jj new <revision>` (例: `jj new master`) で `-m` なしも block
/// - C: `jj split` interactive (= `-m` なし) は editor hang issue があるため strong block
/// - D: scope は `jj` 直接呼び出しのみ (`pnpm jj-new` 等のラッパーは scope 外)
///
/// `BlockedPattern.exception` を活用し「pattern match + exception 不一致」の 2 段判定で
/// `-m`/`--message` 存在時の allow を実現する (Rust 標準 regex crate は negative lookahead 非対応)。
fn preset_jj_message_required() -> Vec<BlockedPattern> {
    let msg = r#"**jj new / jj split に -m 引数なしがブロックされました**

理由:
- `jj new` (引数なし or revision 指定) で message を省略すると description 未設定の commit が作成され、
  後続の編集が意図しない commit に混入する事故が起こる (PR #171 で実観測)
- `jj split` を `-m` なしで実行すると interactive editor が起動し、Claude セッションが hang する

**正しい使い方:**
```
jj new -m "WIP: <description>"               # 新 commit 開始
jj new master -m "WIP: <description>"        # revision 指定
jj split -m "<message>" <files>              # commit 分離
```

設計判断 (順位 144、PR #171 T3-#8): `pnpm jj-new` 等の wrapper は scope 外。"#;
    let exception = Regex::new(r"\s(-m|--message)\b").unwrap();
    vec![BlockedPattern {
        pattern: Regex::new(r"(?im)(^|&&|;|\|\||\||&|\n)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+(new|split)\b").unwrap(),
        exception: Some(exception),
        message: msg,
    }]
}

/// プリセット: secret-detection (AWS / OpenAI / GitHub / Anthropic 等の hardcoded secret 検出)
///
/// 順位 146 (PR #200 follow-up、`~/.claude/rules/common/security.md` § Secret Management 移管):
/// 「NEVER hardcode secrets in source code」を機械強制する mechanical enforcement 層。
/// session 毎の rule load コスト排除 + 漏洩観測前の preventive 層として Tier 1 採用。
/// memory `feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で
/// Claude 判断介入を排除、session 毎の rule load コスト不要。
///
/// 設計判断 (順位 146、PR #200 follow-up):
/// - Bash command + Edit/Write の new_string/content の両方をスキャン (handle_write_edit_tool で呼び出し)
/// - false positive 軽減: AWS Secret Key は env-var-assignment 形式 (`aws_secret_access_key = "..."`) に限定
/// - OpenAI `sk-` 系は Anthropic の `sk-ant-` を `exception` field で除外 (Rust regex は negative lookahead 非対応)
/// - 漏洩の非対称性 (= 1 度漏れたら手遅れ) のため `default_preset_names()` に含め、config 不在環境でも default-on
fn preset_secret_detection() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(
                r#"(?i)aws_secret_access_key\s*[:=]\s*["']?[A-Za-z0-9/+=]{40}["']?"#,
            )
            .unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\bsk-[A-Za-z0-9_-]{40,}\b").unwrap(),
            exception: Some(Regex::new(r"\bsk-ant-").unwrap()),
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\b(ghp|github_pat)_[A-Za-z0-9_]{20,}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\b(gho|ghs|ghu|ghr)_[A-Za-z0-9]{36}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"\bsk-ant-[A-Za-z0-9_-]{20,}\b").unwrap(),
            exception: None,
            message: SECRET_DETECTION_MSG,
        },
    ]
}

const SECRET_DETECTION_MSG: &str = r#"**機密情報 (secret) が検出されました**

ハードコードされた API key / token / credential を検出しました。漏洩は重大なセキュリティ事故に繋がり、git history から完全除去するには force push が必要になります。

**対応方法:**
- 環境変数に移管: Rust なら `std::env::var("API_KEY")`、Node.js なら `process.env.API_KEY`
- Secret manager (1Password / Doppler / AWS Secrets Manager / GitHub Actions Secrets 等) を使用
- `.env` ファイル + `.gitignore` で local-only 管理 (本番は別途)
- test fixture でも、regex に match する形式 (16 chars 以上の AKIA... 等) は避け、`AKIATEST` 等の明らかに無効な短い形を使う

設計判断 (順位 146、PR #200 follow-up): `~/.claude/rules/common/security.md` § Secret Management の機械強制層。"#;

fn default_preset_names() -> Vec<String> {
    vec![
        "default".to_string(),
        "git".to_string(),
        "jj-immutable".to_string(),
        "jj-main-guard".to_string(),
        "jj-push-guard".to_string(),
        "electron".to_string(),
        "secret-detection".to_string(),
    ]
}

fn resolve_preset_or_custom(name: &str) -> Vec<BlockedPattern> {
    match name {
        "default" => preset_default(),
        "git" => preset_git(),
        "jj-immutable" => preset_jj_immutable(),
        "jj-main-guard" => preset_jj_main_guard(),
        "jj-push-guard" => preset_jj_push_guard(),
        "gh-pr-create-guard" => preset_gh_pr_create_guard(),
        "gh-pr-merge-guard" => preset_gh_pr_merge_guard(),
        "jj-message-required" => preset_jj_message_required(),
        "secret-detection" => preset_secret_detection(),
        "polling-anti-pattern" => preset_polling_anti_pattern(),
        "exe-help-block" => preset_exe_help_block(),
        "electron" => preset_electron(),
        custom => custom_regex_pattern(custom),
    }
}

fn custom_regex_pattern(custom: &str) -> Vec<BlockedPattern> {
    match Regex::new(custom) {
        Ok(re) => vec![BlockedPattern {
            pattern: re,
            exception: None,
            message: "**カスタムパターンによりブロックされました**\n\nこのコマンドは hooks-config.toml のカスタムルールによりブロックされています。",
        }],
        Err(_) => {
            eprintln!(
                "[validate-command] Warning: Invalid regex in blocked_patterns: {}",
                custom
            );
            Vec::new()
        }
    }
}

fn build_blocked_patterns(config: &Config) -> Vec<BlockedPattern> {
    let preset_names: Vec<String> = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.blocked_patterns.as_ref())
        .cloned()
        .unwrap_or_else(default_preset_names);
    preset_names
        .iter()
        .flat_map(|name| resolve_preset_or_custom(name.as_str()))
        .collect()
}

fn validate_command(command: &str, patterns: &[BlockedPattern]) -> Option<&'static str> {
    for pattern in patterns {
        if pattern.pattern.is_match(command) {
            if let Some(exc) = &pattern.exception {
                if exc.is_match(command) {
                    continue;
                }
            }
            return Some(pattern.message);
        }
    }
    None
}

/// リンター/フォーマッター設定ファイルとして保護する対象 (デフォルトリスト)
const PROTECTED_CONFIG_FILES: &[&str] = &[
    // JavaScript / TypeScript
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.json",
    ".eslintrc.yml",
    ".eslintrc.yaml",
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
    ".prettierrc",
    ".prettierrc.js",
    ".prettierrc.cjs",
    ".prettierrc.json",
    ".prettierrc.yml",
    ".prettierrc.yaml",
    "prettier.config.js",
    "prettier.config.cjs",
    "biome.json",
    "biome.jsonc",
    "tsconfig.json",
    "tsconfig.build.json",
    // Git hooks / pre-commit
    "lefthook.yml",
    "lefthook.yaml",
    ".pre-commit-config.yaml",
    ".husky",
    // Python
    "pyproject.toml",
    ".flake8",
    ".pylintrc",
    "setup.cfg",
    // Rust
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    // Go
    ".golangci.yml",
    ".golangci.yaml",
    // Swift
    ".swiftlint.yml",
    ".swiftlint.yaml",
    // Secrets / Environment
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    ".env.staging",
    ".env.test",
];

/// ファイルパスが保護対象の設定ファイルに該当するか判定
fn is_protected_config(file_path: &str, extra_files: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    let normalized_lower = normalized.to_ascii_lowercase();

    let file_name = normalized_lower
        .rsplit('/')
        .next()
        .unwrap_or(&normalized_lower);

    // デフォルトリスト + 追加ファイルを結合してチェック
    let check_name = |protected: &str| -> bool {
        let protected_lower = protected.to_ascii_lowercase();
        if protected == ".husky" {
            let dir_prefix = format!("{}/", protected_lower);
            file_name == protected_lower
                || normalized_lower.contains(&format!("/{}", dir_prefix))
                || normalized_lower.starts_with(&dir_prefix)
        } else if protected_lower.contains('/') {
            // パス付き指定: 完全一致 or スラッシュ境界での末尾一致
            normalized_lower == protected_lower
                || normalized_lower.ends_with(&format!("/{}", protected_lower))
        } else {
            file_name == protected_lower
        }
    };

    PROTECTED_CONFIG_FILES.iter().any(|&p| check_name(p))
        || extra_files.iter().any(|p| check_name(p))
}

/// 設定ファイルのパス解決: exe のあるディレクトリ / hooks-config.toml
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む (存在しない場合はデフォルト)
fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "[validate-command] Warning: Failed to parse {}: {}",
                path.display(),
                e
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

fn is_docs_todo_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    let re = match Regex::new(r"(^|/)docs/todo[\w-]*\.md$") {
        Ok(r) => r,
        Err(_) => return false,
    };
    re.is_match(&normalized)
}

fn extract_heading_keywords(text: &str) -> Vec<String> {
    let prefix_re = Regex::new(r"^順位\s*\d+\s*[:：]?\s*").ok();
    text.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .map(|heading| {
            let stripped = match &prefix_re {
                Some(re) => re.replace(heading.trim(), "").to_string(),
                None => heading.trim().to_string(),
            };
            stripped
                .split(['(', '（', '['])
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| s.len() >= 3)
        .collect()
}

fn run_jj_with_timeout(args: &[&str], timeout_secs: u64) -> Option<String> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut child = Command::new("jj")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut buf = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_end(&mut buf);
                }
                return if status.success() {
                    String::from_utf8(buf).ok()
                } else {
                    None
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

fn count_commits_branch_ahead(branch: &str) -> Option<usize> {
    let revset = format!("@-..{}", branch);
    let output = run_jj_with_timeout(
        &[
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-T",
            "commit_id ++ \"\\n\"",
        ],
        TODO_STALENESS_JJ_TIMEOUT_SECS,
    )?;
    Some(output.lines().filter(|l| !l.trim().is_empty()).count())
}

fn parse_jj_log_records(raw: &str) -> Vec<(String, String)> {
    raw.split('\x1e')
        .filter_map(|record| {
            let mut parts = record.splitn(2, '\x1f');
            let commit_id = parts.next()?.trim().to_string();
            let description = parts.next()?.trim().to_string();
            if commit_id.is_empty() || description.is_empty() {
                None
            } else {
                Some((commit_id, description))
            }
        })
        .collect()
}

fn jj_log_recent_descriptions(limit: u64) -> Vec<(String, String)> {
    let limit_str = limit.to_string();
    let template = "commit_id.shortest(8) ++ \"\\x1f\" ++ description ++ \"\\x1e\"";
    match run_jj_with_timeout(
        &["log", "--limit", &limit_str, "--no-graph", "-T", template],
        TODO_STALENESS_JJ_TIMEOUT_SECS,
    ) {
        Some(raw) => parse_jj_log_records(&raw),
        None => Vec::new(),
    }
}

fn first_line(s: &str) -> &str {
    s.split('\n').next().unwrap_or("").trim()
}

fn find_matching_commits<'a>(
    keyword: &str,
    commits: &'a [(String, String)],
) -> Vec<&'a (String, String)> {
    let needle = keyword.to_lowercase();
    commits
        .iter()
        .filter(|(_, desc)| desc.to_lowercase().contains(&needle))
        .take(3)
        .collect()
}

fn build_todo_staleness_message(
    file_path: &str,
    behind: Option<usize>,
    keyword_matches: &[(String, Vec<(String, String)>)],
    branch: &str,
) -> Option<String> {
    let stale = behind.is_none_or(|n| n > 0);
    let any_matches = keyword_matches.iter().any(|(_, m)| !m.is_empty());
    if !stale && !any_matches {
        return None;
    }
    let mut lines = vec![format!("[docs/todo edit context] {}", file_path)];
    if let Some(b) = behind {
        if b > 0 {
            lines.push(format!(
                "stale parent detected: {} は @- より {} commits ahead",
                branch, b
            ));
            lines.push(format!(
                "修正手順: `jj git fetch && jj new {} -m \"WIP: <description>\"`",
                branch
            ));
        }
    } else {
        lines.push(
            "stale parent detected: lineage 判定不能のため fail-closed で block".to_string(),
        );
    }
    for (keyword, matches) in keyword_matches {
        if matches.is_empty() {
            continue;
        }
        lines.push(format!("関連既実装の可能性 (keyword: \"{}\"):", keyword));
        for (commit_id, desc) in matches {
            lines.push(format!("  {} {}", commit_id, first_line(desc)));
        }
    }
    Some(lines.join("\n"))
}

fn check_todo_staleness(
    file_path: &str,
    text_for_keywords: &str,
    config: &TodoStalenessConfig,
) -> Option<TodoStalenessResult> {
    if !config.enabled.unwrap_or(false) {
        return None;
    }
    if !is_docs_todo_path(file_path) {
        return None;
    }
    let branch = config
        .default_branch
        .as_deref()
        .unwrap_or(TODO_STALENESS_DEFAULT_BRANCH);
    let limit = config
        .grep_recent_limit
        .unwrap_or(TODO_STALENESS_DEFAULT_GREP_LIMIT);

    let behind = count_commits_branch_ahead(branch);
    let stale = behind.is_none_or(|n| n > 0);

    let keywords = extract_heading_keywords(text_for_keywords);
    let keyword_matches: Vec<(String, Vec<(String, String)>)> = if keywords.is_empty() {
        Vec::new()
    } else {
        let commits = jj_log_recent_descriptions(limit);
        keywords
            .iter()
            .take(3)
            .map(|kw| {
                let matches: Vec<(String, String)> =
                    find_matching_commits(kw, &commits).into_iter().cloned().collect();
                (kw.clone(), matches)
            })
            .collect()
    };

    let message = build_todo_staleness_message(file_path, behind, &keyword_matches, branch)?;
    Some(TodoStalenessResult { message, stale })
}

struct TodoStalenessResult {
    message: String,
    stale: bool,
}

fn collect_text_for_keywords(tool_input: &ToolInput) -> String {
    let mut parts = Vec::new();
    if let Some(old) = &tool_input.old_string {
        parts.push(old.as_str());
    }
    if let Some(new_s) = &tool_input.new_string {
        parts.push(new_s.as_str());
    }
    if let Some(content) = &tool_input.content {
        parts.push(content.as_str());
    }
    parts.join("\n")
}

/// Edit/Write 時の secret scan 対象テキスト (new_string + content のみ、old_string は除外)。
/// 順位 146 (PR #200 follow-up): old_string は「既存ファイル内の文字列 = 削除対象 or 置換元」
/// であり、ここを scan すると「secret を削除する Edit」までも block してしまうため除外する。
fn collect_text_for_secret_scan(tool_input: &ToolInput) -> String {
    let mut parts = Vec::new();
    if let Some(new_s) = &tool_input.new_string {
        parts.push(new_s.as_str());
    }
    if let Some(content) = &tool_input.content {
        parts.push(content.as_str());
    }
    parts.join("\n")
}

fn is_secret_detection_enabled(config: &Config) -> bool {
    let preset_names: Vec<String> = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.blocked_patterns.as_ref())
        .cloned()
        .unwrap_or_else(default_preset_names);
    preset_names.iter().any(|n| n == "secret-detection")
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

fn handle_bash_tool(config: &Config, tool_input: &ToolInput) -> ExitCode {
    let command = tool_input.command.clone().unwrap_or_default();
    if command.trim().is_empty() {
        return ExitCode::SUCCESS;
    }
    let patterns = build_blocked_patterns(config);
    if let Some(message) = validate_command(&command, &patterns) {
        let _ = io::stderr().write_all(message.as_bytes());
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn resolve_edit_file_path(tool_input: &ToolInput) -> String {
    tool_input
        .file_path
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| tool_input.path.clone())
        .unwrap_or_default()
}

fn check_protected_file(config: &Config, file_path: &str) -> Option<ExitCode> {
    let extra_protected = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.extra_protected_files.as_ref())
        .cloned()
        .unwrap_or_default();
    if file_path.is_empty() || !is_protected_config(file_path, &extra_protected) {
        return None;
    }
    let msg = format!(
        "**保護されたファイルの編集がブロックされました**\n\n\
         `{}` は保護対象ファイル（設定ファイル/機密ファイル）のため、編集が禁止されています。\n\n\
         リンター設定の場合: 設定を変更するのではなく **コード側を修正** してください。\n\
         機密ファイルの場合: 秘密情報の漏洩を防ぐため、編集できません。\n\n\
         変更が本当に必要な場合は、ユーザーに確認を取ってください。",
        file_path.rsplit(['/', '\\']).next().unwrap_or(file_path)
    );
    let _ = io::stderr().write_all(msg.as_bytes());
    Some(ExitCode::from(2))
}

fn check_secret_in_content(config: &Config, tool_input: &ToolInput) -> Option<ExitCode> {
    if !is_secret_detection_enabled(config) {
        return None;
    }
    let scan_text = collect_text_for_secret_scan(tool_input);
    if scan_text.is_empty() {
        return None;
    }
    let secret_patterns = preset_secret_detection();
    let message = validate_command(&scan_text, &secret_patterns)?;
    let _ = io::stderr().write_all(message.as_bytes());
    Some(ExitCode::from(2))
}

fn check_todo_staleness_for_edit(
    config: &Config,
    tool_input: &ToolInput,
    file_path: &str,
) -> Option<ExitCode> {
    let staleness_config = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.todo_staleness.as_ref())?;
    let text = collect_text_for_keywords(tool_input);
    let result = check_todo_staleness(file_path, &text, staleness_config)?;
    let _ = io::stderr().write_all(result.message.as_bytes());
    if result.stale {
        Some(ExitCode::from(2))
    } else {
        None
    }
}

fn handle_write_edit_tool(config: &Config, tool_input: &ToolInput) -> ExitCode {
    let file_path = resolve_edit_file_path(tool_input);
    if let Some(code) = check_protected_file(config, &file_path) {
        return code;
    }
    if let Some(code) = check_secret_in_content(config, tool_input) {
        return code;
    }
    if let Some(code) = check_todo_staleness_for_edit(config, tool_input, &file_path) {
        return code;
    }
    ExitCode::SUCCESS
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
    match tool_name.as_str() {
        "Bash" => handle_bash_tool(&config, &tool_input),
        "Write" | "Edit" | "Replace" => handle_write_edit_tool(&config, &tool_input),
        _ => ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- プリセットベースのパターン構築テスト ---

    fn all_patterns() -> Vec<BlockedPattern> {
        build_blocked_patterns(&Config::default())
    }

    fn patterns_with_presets(presets: &[&str]) -> Vec<BlockedPattern> {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(presets.iter().map(|s| s.to_string()).collect()),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        build_blocked_patterns(&config)
    }

    fn is_blocked(command: &str) -> bool {
        validate_command(command, &all_patterns()).is_some()
    }

    fn is_blocked_with(command: &str, presets: &[&str]) -> bool {
        let patterns = patterns_with_presets(presets);
        validate_command(command, &patterns).is_some()
    }

    // --- 設定ベースのプリセット選択テスト ---

    #[test]
    fn default_config_enables_all_presets() {
        // Config::default() (設定ファイル無し) → 全プリセット有効
        assert!(is_blocked("git push"));
        assert!(is_blocked("rm -rf /tmp"));
        assert!(is_blocked("jj --ignore-immutable rebase"));
        assert!(is_blocked("jj new main"));
        assert!(is_blocked("electron ."));
    }

    #[test]
    fn only_default_preset_allows_git() {
        // "default" のみ → git は許可される
        assert!(!is_blocked_with("git push", &["default"]));
        // rm -rf は引き続きブロック
        assert!(is_blocked_with("rm -rf /tmp", &["default"]));
    }

    #[test]
    fn git_preset_blocks_git() {
        assert!(is_blocked_with("git push", &["git"]));
        assert!(is_blocked_with("bash -c 'git push'", &["git"]));
    }

    #[test]
    fn jj_presets_independent() {
        // jj-immutable のみ有効
        assert!(is_blocked_with(
            "jj --ignore-immutable rebase",
            &["jj-immutable"]
        ));
        assert!(!is_blocked_with("jj new main", &["jj-immutable"]));

        // jj-main-guard のみ有効
        assert!(is_blocked_with("jj new main", &["jj-main-guard"]));
        assert!(!is_blocked_with(
            "jj --ignore-immutable rebase",
            &["jj-main-guard"]
        ));
    }

    #[test]
    fn empty_presets_blocks_nothing() {
        let patterns = patterns_with_presets(&[]);
        assert!(validate_command("git push", &patterns).is_none());
        assert!(validate_command("rm -rf /tmp", &patterns).is_none());
    }

    #[test]
    fn custom_regex_pattern() {
        assert!(is_blocked_with("docker rm -f container", &[r"docker\s+rm"]));
        assert!(!is_blocked_with("docker ps", &[r"docker\s+rm"]));
    }

    const JJ_MSG_REQ: &[&str] = &["jj-message-required"];

    #[test]
    fn jj_message_required_blocks_bare_jj_new() {
        assert!(is_blocked_with("jj new", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_new_with_revision() {
        assert!(is_blocked_with("jj new master", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_split_without_message() {
        assert!(is_blocked_with("jj split file.rs", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_new_after_double_ampersand() {
        assert!(is_blocked_with("cd /tmp && jj new", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_split_after_newline() {
        assert!(is_blocked_with("echo ok\njj split src/main.rs", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_m_flag() {
        assert!(!is_blocked_with("jj new -m \"WIP: foo\"", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_revision_and_m_flag() {
        assert!(!is_blocked_with(
            "jj new master -m \"WIP: foo\"",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_long_message_flag() {
        assert!(!is_blocked_with(
            "jj new --message \"WIP: foo\"",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_split_with_m_flag() {
        assert!(!is_blocked_with(
            "jj split -m \"split message\" file.rs",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_split_with_long_message_flag() {
        assert!(!is_blocked_with(
            "jj split --message \"split message\" file.rs",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_with_main_guard_still_blocks_jj_new_main_even_with_m() {
        assert!(is_blocked_with(
            "jj new main -m \"WIP\"",
            &["jj-main-guard", "jj-message-required"]
        ));
    }

    #[test]
    fn jj_message_required_does_not_affect_other_jj_subcommands() {
        assert!(!is_blocked_with("jj status", JJ_MSG_REQ));
        assert!(!is_blocked_with("jj log", JJ_MSG_REQ));
        assert!(!is_blocked_with("jj describe -m \"x\"", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_scope_excludes_pnpm_wrappers() {
        assert!(!is_blocked_with("pnpm jj-new", JJ_MSG_REQ));
        assert!(!is_blocked_with("pnpm jj-start-change", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_not_in_default_fallback_is_opt_in() {
        let patterns = build_blocked_patterns(&Config::default());
        assert!(
            validate_command("jj new", &patterns).is_none(),
            "default fallback should NOT include jj-message-required (opt-in via hooks-config.toml)"
        );
    }

    const SECRET_DETECT: &[&str] = &["secret-detection"];

    #[test]
    fn secret_detection_blocks_aws_access_key() {
        assert!(is_blocked_with(
            "let aws = \"AKIAIOSFODNN7EXAMPLE\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_aws_secret_assignment() {
        assert!(is_blocked_with(
            r#"aws_secret_access_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY""#,
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_openai_api_key() {
        assert!(is_blocked_with(
            "const key = \"sk-proj-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWX_-\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_github_pat_classic() {
        assert!(is_blocked_with(
            "let token = \"ghp_abcdefghijklmnopqrstuvwxyzABCDEFGHIJ\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_github_pat_finegrained() {
        assert!(is_blocked_with(
            "let token = \"github_pat_11AAAAAAA0abcdefghijK\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_github_oauth_token() {
        assert!(is_blocked_with(
            "let token = \"gho_abcdefghijklmnopqrstuvwxyz0123456789\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_github_server_token() {
        assert!(is_blocked_with(
            "let token = \"ghs_abcdefghijklmnopqrstuvwxyz0123456789\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_anthropic_api_key() {
        assert!(is_blocked_with(
            "let key = \"sk-ant-api03-AAAAAAAA_BBBBBBBB_CCCCCCCC\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_blocks_in_bash_command_via_echo() {
        assert!(is_blocked_with(
            "echo \"AKIAIOSFODNN7EXAMPLE\" > .env",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_allows_short_test_fixture_value_below_threshold() {
        assert!(!is_blocked_with("let key = \"AKIATEST\";", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_short_sk_prefix_below_threshold() {
        assert!(!is_blocked_with("let x = \"sk-test\";", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_short_ghp_prefix_below_threshold() {
        assert!(!is_blocked_with("let x = \"ghp_short\";", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_variable_name_secret_or_key() {
        assert!(!is_blocked_with(
            "let api_key = config.api_key;",
            SECRET_DETECT
        ));
        assert!(!is_blocked_with("self.secret = None;", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_allows_env_var_reference() {
        assert!(!is_blocked_with(
            "std::env::var(\"AWS_SECRET_ACCESS_KEY\")",
            SECRET_DETECT
        ));
        assert!(!is_blocked_with("process.env.GITHUB_TOKEN", SECRET_DETECT));
    }

    #[test]
    fn secret_detection_aws_secret_pattern_requires_assignment_form_for_fp_reduction() {
        assert!(!is_blocked_with(
            "let blob = \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\";",
            SECRET_DETECT
        ));
    }

    #[test]
    fn secret_detection_in_default_fallback_is_default_on_security_critical() {
        let patterns = build_blocked_patterns(&Config::default());
        assert!(
            validate_command("let k = \"AKIAIOSFODNN7EXAMPLE\";", &patterns).is_some(),
            "default fallback should include secret-detection (Tier 1 security-critical default-on, 漏洩の非対称性のため)"
        );
    }

    #[test]
    fn is_secret_detection_enabled_returns_true_when_listed_in_blocked_patterns() {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(vec!["secret-detection".to_string()]),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        assert!(is_secret_detection_enabled(&config));
    }

    #[test]
    fn is_secret_detection_enabled_returns_false_when_excluded_from_blocked_patterns() {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(vec!["default".to_string(), "git".to_string()]),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        assert!(!is_secret_detection_enabled(&config));
    }

    #[test]
    fn is_secret_detection_enabled_returns_true_for_default_config_default_on() {
        assert!(is_secret_detection_enabled(&Config::default()));
    }

    #[test]
    fn collect_text_for_secret_scan_excludes_old_string_to_allow_secret_removal() {
        let tool_input = ToolInput {
            command: None,
            file_path: Some("foo.rs".to_string()),
            path: None,
            old_string: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
            new_string: Some("AKIATEST".to_string()),
            content: None,
        };
        let scanned = collect_text_for_secret_scan(&tool_input);
        assert!(!scanned.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(scanned.contains("AKIATEST"));
    }

    #[test]
    fn collect_text_for_secret_scan_includes_both_new_string_and_content() {
        let tool_input = ToolInput {
            command: None,
            file_path: Some("foo.rs".to_string()),
            path: None,
            old_string: None,
            new_string: Some("new-text".to_string()),
            content: Some("full-content".to_string()),
        };
        let scanned = collect_text_for_secret_scan(&tool_input);
        assert!(scanned.contains("new-text"));
        assert!(scanned.contains("full-content"));
    }

    #[test]
    fn secret_detection_does_not_affect_other_presets_non_regression() {
        assert!(is_blocked_with("git push", &["git", "secret-detection"]));
        assert!(is_blocked_with(
            "rm -rf /tmp",
            &["default", "secret-detection"]
        ));
        assert!(!is_blocked_with("git status", &["default", "secret-detection"]));
    }

    #[test]
    fn blocks_git_at_start() {
        assert!(is_blocked("git push"));
    }

    #[test]
    fn blocks_git_status() {
        assert!(is_blocked("git status"));
    }

    // --- git: chained after shell operators (should block) ---

    #[test]
    fn blocks_git_after_ampersand_ampersand() {
        assert!(is_blocked("cd /e/work && git push"));
    }

    #[test]
    fn blocks_git_after_semicolon() {
        assert!(is_blocked("true; git status"));
    }

    #[test]
    fn blocks_git_after_or() {
        assert!(is_blocked("false || git log"));
    }

    #[test]
    fn blocks_git_after_pipe() {
        assert!(is_blocked("echo data | git apply"));
    }

    #[test]
    fn blocks_git_in_triple_chain() {
        assert!(is_blocked("cd /path && echo ok && git commit -m 'test'"));
    }

    #[test]
    fn blocks_git_after_single_ampersand() {
        assert!(is_blocked("echo ok & git status"));
    }

    // --- git: multiline and bare git (should block) ---

    #[test]
    fn blocks_git_after_newline() {
        assert!(is_blocked("echo ok\ngit push"));
    }

    #[test]
    fn blocks_bare_git() {
        assert!(is_blocked("git"));
    }

    #[test]
    fn blocks_cd_d_after_newline() {
        assert!(is_blocked("echo ok\ncd /d e:\\work"));
    }

    // --- git: env/command prefix bypass (should block) ---

    #[test]
    fn blocks_git_with_env_prefix() {
        assert!(is_blocked("GIT_TRACE=1 git status"));
    }

    #[test]
    fn blocks_git_with_command_builtin() {
        assert!(is_blocked("command git push"));
    }

    #[test]
    fn blocks_git_with_env_builtin() {
        assert!(is_blocked("env VAR=value git log"));
    }

    #[test]
    fn blocks_git_env_prefix_after_chain() {
        assert!(is_blocked("echo x; GIT_TRACE=1 git diff"));
    }

    // --- git: allowed commands (should NOT block) ---

    #[test]
    fn blocks_jj_git_push() {
        // jj-push-guard プリセットにより、直接の jj git push はブロックされる
        // pnpm push 経由でのみ push を許可する設計
        assert!(is_blocked("jj git push"));
    }

    #[test]
    fn allows_jj_git_fetch() {
        assert!(!is_blocked("jj git fetch"));
    }

    #[test]
    fn allows_gh_pr_create() {
        assert!(!is_blocked("gh pr create --title 'test'"));
    }

    // --- gh-pr-create-guard ---

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create() {
        assert!(is_blocked_with(
            "gh pr create --title 'test'",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create_in_chain() {
        assert!(is_blocked_with(
            "cd /tmp && gh pr create --title 'test'",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_with_repo_pr_create() {
        assert!(is_blocked_with(
            "gh -R owner/repo pr create",
            &["gh-pr-create-guard"]
        ));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_merge() {
        assert!(!is_blocked_with("gh pr merge 42", &["gh-pr-create-guard"]));
    }

    // --- gh-pr-merge-guard ---

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge() {
        assert!(is_blocked_with("gh pr merge 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_squash() {
        assert!(is_blocked_with(
            "gh pr merge 42 --squash",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_in_chain() {
        assert!(is_blocked_with(
            "cd /tmp && gh pr merge 42",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_with_repo_pr_merge() {
        assert!(is_blocked_with(
            "gh -R owner/repo pr merge 42",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_create() {
        assert!(!is_blocked_with(
            "gh pr create --title 'test'",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_bash_c_gh_pr_merge() {
        assert!(is_blocked_with(
            "bash -c 'gh pr merge 42'",
            &["gh-pr-merge-guard"]
        ));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_sh_lc_gh_pr_merge() {
        assert!(is_blocked_with(
            "sh -lc 'gh pr merge 42 --squash'",
            &["gh-pr-merge-guard"]
        ));
    }

    // --- polling-anti-pattern ---

    #[test]
    fn polling_blocks_until_sleep_oneliner() {
        // PR #86 で実証された具体的な polling pattern
        assert!(is_blocked_with(
            "until grep -q done /tmp/log; do sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_until_sleep_multiline() {
        // 複数行で書かれた polling
        let cmd = "until grep -q ready /tmp/state\ndo\n  sleep 3\ndone";
        assert!(is_blocked_with(cmd, &["polling-anti-pattern"]));
    }

    #[test]
    fn polling_blocks_until_with_test_bracket() {
        // [ ... ] 形式の条件
        assert!(is_blocked_with(
            "until [ -f /tmp/done ]; do sleep 2; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_while_not_sleep() {
        // while ! 形式の polling
        assert!(is_blocked_with(
            "while ! grep -q done /tmp/log; do sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_until_with_cat_state_file() {
        // pr-monitor-state.json への polling (実際に頻発した pattern)
        assert!(is_blocked_with(
            "until cat .claude/pr-monitor-state.json | grep -q complete; do sleep 10; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_for_loop_with_sleep() {
        // for ループ + sleep は countdown / 順次実行のため polling ではない
        assert!(!is_blocked_with(
            "for i in $(seq 1 3); do echo $i; sleep 1; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_simple_sleep() {
        // 単純な sleep のみは polling ではない
        assert!(!is_blocked_with("sleep 5", &["polling-anti-pattern"]));
    }

    #[test]
    fn polling_does_not_block_until_without_sleep() {
        // sleep を含まない until は polling 判定外 (CPU spin だが別問題)
        assert!(!is_blocked_with(
            "until [ -f /tmp/done ]; do echo waiting; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_echo_string_with_until() {
        // 文字列リテラル中の until は誤検出しない (\bdo\b 制約により)
        assert!(!is_blocked_with(
            "echo 'wait until ready' && sleep 5",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_git_log_until_flag() {
        // --until=DATE フラグは git log の引数で polling ではない
        assert!(!is_blocked_with(
            "git log --until=yesterday; sleep 1",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_string_with_while() {
        // 文字列中の while を含むコマンドも誤検出しない
        assert!(!is_blocked_with(
            "echo 'a while later we sleep'; sleep 2",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_does_not_block_while_true_loop() {
        // while true; do ... sleep ... done は daemon-like で polling とは別パターン
        // (false positive を避けるため明示的に除外)
        assert!(!is_blocked_with(
            "while true; do work; sleep 5; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_blocks_in_chained_command() {
        // chain の中の polling もブロック
        assert!(is_blocked_with(
            "echo start && until grep -q done; do sleep 3; done",
            &["polling-anti-pattern"]
        ));
    }

    #[test]
    fn polling_default_config_does_not_enable() {
        // 後方互換: デフォルトフォールバックには polling-anti-pattern を含めない
        // (既存リポジトリへの影響を避ける、明示 opt-in)
        let config = Config::default();
        let patterns = build_blocked_patterns(&config);
        // 既存リポでは polling pattern は通る (config が無い場合の挙動)
        assert!(validate_command("until grep -q done; do sleep 5; done", &patterns).is_none());
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_short_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe -h",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_cli_merge_pipeline_windows_help() {
        assert!(is_blocked_with(
            "cli-merge-pipeline.exe /?",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_dot_slash_claude_prefix() {
        assert!(is_blocked_with(
            "./.claude/cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_claude_prefix() {
        assert!(is_blocked_with(
            ".claude/check-ci-coderabbit.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_hooks_exe() {
        assert!(is_blocked_with(
            "hooks-pre-tool-validate.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_after_chain() {
        assert!(is_blocked_with(
            "cd /tmp && cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_with_env_prefix() {
        assert!(is_blocked_with(
            "RUST_LOG=debug cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_windows_path() {
        assert!(is_blocked_with(
            r"e:\work\.claude\cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_blocks_after_pipe() {
        assert!(is_blocked_with(
            "echo x | cli-merge-pipeline.exe --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_subcommand_help() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe foo --help",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_cargo_run_help() {
        assert!(!is_blocked_with("cargo run --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_gh_pr_view_help() {
        assert!(!is_blocked_with("gh pr view --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_pnpm_build_help() {
        assert!(!is_blocked_with("pnpm build --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_exe_without_help_arg() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_exe_with_version() {
        assert!(!is_blocked_with(
            "cli-merge-pipeline.exe --version",
            &["exe-help-block"]
        ));
    }

    #[test]
    fn exe_help_block_allows_unrelated_exe() {
        assert!(!is_blocked_with("foo.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_cargo_exe_help() {
        assert!(!is_blocked_with("cargo.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_python_exe_help() {
        assert!(!is_blocked_with("python.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_node_exe_help() {
        assert!(!is_blocked_with("node.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_allows_notepad_exe_help() {
        assert!(!is_blocked_with("notepad.exe --help", &["exe-help-block"]));
    }

    #[test]
    fn exe_help_block_default_config_does_not_enable() {
        let config = Config::default();
        let patterns = build_blocked_patterns(&config);
        assert!(
            validate_command("cli-merge-pipeline.exe --help", &patterns).is_none(),
            "exe-help-block should be opt-in via hooks-config.toml"
        );
    }

    #[test]
    fn allows_pnpm_lint() {
        assert!(!is_blocked("pnpm lint"));
    }

    #[test]
    fn allows_jj_status() {
        assert!(!is_blocked("jj status"));
    }

    // --- cd /d: direct and chained (should block) ---

    #[test]
    fn blocks_cd_d_at_start() {
        assert!(is_blocked(r"cd /d e:\work"));
    }

    #[test]
    fn blocks_cd_d_after_ampersand_ampersand() {
        assert!(is_blocked(r"echo ok && cd /d e:\work"));
    }

    // --- rm -rf (should block regardless of position) ---

    #[test]
    fn blocks_rm_rf_at_start() {
        assert!(is_blocked("rm -rf /tmp/test"));
    }

    #[test]
    fn blocks_rm_rf_after_chain() {
        assert!(is_blocked("cd /path && rm -rf /tmp"));
    }

    #[test]
    fn blocks_rm_split_r_then_f() {
        assert!(is_blocked("rm -r -f /tmp/test"));
    }

    #[test]
    fn blocks_rm_split_f_then_r() {
        assert!(is_blocked("rm -f -r /tmp/test"));
    }

    // --- git in shell wrapper (should block) ---

    #[test]
    fn blocks_bash_c_git() {
        assert!(is_blocked("bash -c 'git push'"));
    }

    #[test]
    fn blocks_bash_lc_git() {
        assert!(is_blocked("bash -lc 'git status'"));
    }

    #[test]
    fn blocks_sh_c_git() {
        assert!(is_blocked(r#"sh -c "git log""#));
    }

    // --- Electron E2E (should block) ---

    #[test]
    fn blocks_npm_run_test_e2e_electron() {
        assert!(is_blocked("npm run test:e2e:electron"));
    }

    #[test]
    fn blocks_npx_playwright_electron() {
        assert!(is_blocked(
            "npx playwright test --config=playwright-electron.config.ts"
        ));
    }

    #[test]
    fn blocks_electron_with_path_arg() {
        assert!(is_blocked("electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_exec_electron() {
        assert!(is_blocked("pnpm exec electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_start() {
        assert!(is_blocked("pnpm start"));
    }

    #[test]
    fn blocks_pnpm_run_start() {
        assert!(is_blocked("pnpm run start"));
    }

    #[test]
    fn blocks_pnpm_run_test_e2e_electron() {
        assert!(is_blocked("pnpm run test:e2e:electron"));
    }

    #[test]
    fn blocks_pnpm_exec_playwright_electron() {
        assert!(is_blocked(
            "pnpm exec playwright test --config=playwright-electron.config.ts"
        ));
    }

    // --- jj new main: 第2層 (should block) ---

    #[test]
    fn blocks_jj_new_main() {
        assert!(is_blocked("jj new main"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main() {
        assert!(is_blocked("pnpm jj-new main"));
    }

    #[test]
    fn blocks_jj_new_main_with_flag() {
        assert!(is_blocked("jj new main --no-edit"));
    }

    #[test]
    fn allows_jj_new_origin_main() {
        assert!(!is_blocked("jj new origin/main"));
    }

    #[test]
    fn allows_jj_new_main_at_origin() {
        assert!(!is_blocked("jj new main@origin"));
    }

    #[test]
    fn blocks_jj_new_main_single_quoted() {
        assert!(is_blocked("jj new 'main'"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main_double_quoted() {
        assert!(is_blocked("pnpm jj-new \"main\""));
    }

    #[test]
    fn allows_jj_new_feature_branch() {
        assert!(!is_blocked("jj new feature/foo"));
    }

    #[test]
    fn allows_jj_new_mainline() {
        assert!(!is_blocked("jj new mainline"));
    }

    // --- jj edit main: 第3層 (should block) ---

    #[test]
    fn blocks_jj_edit_main() {
        assert!(is_blocked("jj edit main"));
    }

    #[test]
    fn blocks_pnpm_jj_edit_main() {
        assert!(is_blocked("pnpm jj-edit main"));
    }

    #[test]
    fn allows_jj_edit_feature_branch() {
        assert!(!is_blocked("jj edit feature/foo"));
    }

    // --- jj --ignore-immutable (should block) ---

    #[test]
    fn blocks_jj_ignore_immutable() {
        assert!(is_blocked("jj --ignore-immutable rebase -r abc -d main"));
    }

    #[test]
    fn blocks_jj_ignore_immutable_after_subcommand() {
        assert!(is_blocked("jj rebase --ignore-immutable -r abc -d main"));
    }

    #[test]
    fn allows_jj_rebase_without_ignore_immutable() {
        assert!(!is_blocked("jj rebase -r abc -d main"));
    }

    // --- safe commands (should NOT block) ---

    #[test]
    fn allows_empty_command() {
        assert!(!is_blocked(""));
    }

    #[test]
    fn allows_ls() {
        assert!(!is_blocked("ls -la"));
    }

    #[test]
    fn allows_cd_normal() {
        assert!(!is_blocked("cd /e/work/project"));
    }

    // --- protected config files (should block Write/Edit) ---

    #[test]
    fn protects_eslint_config() {
        assert!(is_protected_config("eslint.config.js", &[]));
    }

    #[test]
    fn protects_eslintrc_json() {
        assert!(is_protected_config(".eslintrc.json", &[]));
    }

    #[test]
    fn protects_biome_json() {
        assert!(is_protected_config("biome.json", &[]));
    }

    #[test]
    fn protects_prettierrc() {
        assert!(is_protected_config(".prettierrc", &[]));
    }

    #[test]
    fn protects_tsconfig() {
        assert!(is_protected_config("tsconfig.json", &[]));
    }

    #[test]
    fn protects_pyproject_toml() {
        assert!(is_protected_config("pyproject.toml", &[]));
    }

    #[test]
    fn protects_rustfmt_toml() {
        assert!(is_protected_config("rustfmt.toml", &[]));
    }

    #[test]
    fn protects_golangci_yml() {
        assert!(is_protected_config(".golangci.yml", &[]));
    }

    #[test]
    fn protects_lefthook_yml() {
        assert!(is_protected_config("lefthook.yml", &[]));
    }

    #[test]
    fn protects_pre_commit_config() {
        assert!(is_protected_config(".pre-commit-config.yaml", &[]));
    }

    #[test]
    fn protects_with_windows_path() {
        assert!(is_protected_config(r"e:\work\project\biome.json", &[]));
    }

    #[test]
    fn protects_with_unix_path() {
        assert!(is_protected_config(
            "/home/user/project/.eslintrc.json",
            &[]
        ));
    }

    #[test]
    fn allows_regular_ts_file() {
        assert!(!is_protected_config("src/app.ts", &[]));
    }

    #[test]
    fn allows_regular_json_file() {
        assert!(!is_protected_config("src/data.json", &[]));
    }

    #[test]
    fn allows_package_json() {
        assert!(!is_protected_config("package.json", &[]));
    }

    #[test]
    fn protects_env() {
        assert!(is_protected_config(".env", &[]));
    }

    #[test]
    fn protects_env_local() {
        assert!(is_protected_config(".env.local", &[]));
    }

    #[test]
    fn protects_env_production() {
        assert!(is_protected_config(".env.production", &[]));
    }

    #[test]
    fn protects_env_with_path() {
        assert!(is_protected_config(r"e:\work\project\.env", &[]));
    }

    // --- .husky ディレクトリ内ファイルの保護 ---

    #[test]
    fn protects_husky_pre_commit() {
        assert!(is_protected_config(".husky/pre-commit", &[]));
    }

    #[test]
    fn protects_husky_with_absolute_path() {
        assert!(is_protected_config(
            "/home/user/project/.husky/pre-commit",
            &[]
        ));
    }

    #[test]
    fn protects_husky_with_windows_path() {
        assert!(is_protected_config(
            r"e:\work\project\.husky\pre-commit",
            &[]
        ));
    }

    // --- case-insensitive 保護 ---

    #[test]
    fn protects_uppercase_husky() {
        assert!(is_protected_config(".HUSKY/pre-commit", &[]));
    }

    #[test]
    fn protects_uppercase_eslintrc() {
        assert!(is_protected_config(".ESLINTRC.JSON", &[]));
    }

    #[test]
    fn protects_mixed_case_biome() {
        assert!(is_protected_config("Biome.Json", &[]));
    }

    // --- jj --ignore-immutable dotall ---

    #[test]
    fn blocks_jj_ignore_immutable_multiline() {
        assert!(is_blocked("jj rebase\n--ignore-immutable -r abc -d main"));
    }

    // --- extra_protected_files テスト ---

    #[test]
    fn extra_protected_files_blocks() {
        let extra = vec!["settings.local.json".to_string()];
        assert!(is_protected_config("settings.local.json", &extra));
        assert!(is_protected_config(
            r"e:\work\.claude\settings.local.json",
            &extra
        ));
    }

    #[test]
    fn extra_protected_files_does_not_affect_default() {
        let extra = vec!["settings.local.json".to_string()];
        // デフォルトリストは引き続き有効
        assert!(is_protected_config("biome.json", &extra));
        // 追加リストにないファイルは許可
        assert!(!is_protected_config("src/app.ts", &extra));
    }

    // --- extra_protected_files パス付き指定テスト ---

    #[test]
    fn extra_protected_path_matches_full_path() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        assert!(is_protected_config(
            r"e:\work\project\.claude\hooks-config.toml",
            &extra
        ));
        assert!(is_protected_config(
            "/home/user/project/.claude/hooks-config.toml",
            &extra
        ));
    }

    #[test]
    fn extra_protected_path_does_not_match_different_dir() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        // 別ディレクトリの同名ファイルはマッチしない
        assert!(!is_protected_config("other/hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_path_does_not_match_bare_basename() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        // パス付き指定はベースネームだけではマッチしない
        assert!(!is_protected_config("hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_basename_still_works() {
        let extra = vec!["hooks-config.toml".to_string()];
        assert!(is_protected_config("hooks-config.toml", &extra));
        assert!(is_protected_config(
            r"e:\work\.claude\hooks-config.toml",
            &extra
        ));
        assert!(is_protected_config("other/hooks-config.toml", &extra));
    }

    fn build_todo_path(suffix: &str) -> String {
        format!("docs/todo{}.md", suffix)
    }

    fn build_todo_path_with_prefix(prefix: &str, suffix: &str) -> String {
        format!("{}/docs/todo{}.md", prefix, suffix)
    }

    fn build_windows_todo_path(suffix: &str) -> String {
        format!("docs\\todo{}.md", suffix)
    }

    #[test]
    fn is_docs_todo_path_detects_repo_layout() {
        assert!(is_docs_todo_path(&build_todo_path("")));
        assert!(is_docs_todo_path(&build_todo_path("2")));
        assert!(is_docs_todo_path(&build_todo_path("-summary")));
        assert!(is_docs_todo_path(&build_todo_path_with_prefix(
            "e:/work/repo",
            "9"
        )));
    }

    #[test]
    fn is_docs_todo_path_handles_windows_separators() {
        assert!(is_docs_todo_path(&build_windows_todo_path("")));
        assert!(is_docs_todo_path(&format!(
            r"e:\work\repo\docs\todo{}.md",
            "8"
        )));
    }

    #[test]
    fn is_docs_todo_path_rejects_unrelated_paths() {
        assert!(!is_docs_todo_path("README.md"));
        assert!(!is_docs_todo_path("docs/adr/adr-041.md"));
        assert!(!is_docs_todo_path(&format!("notes/todo{}.md", "")));
        assert!(!is_docs_todo_path("src/main.rs"));
    }

    #[test]
    fn extract_heading_keywords_strips_rank_prefix() {
        let text = "### 順位 136 working copy staleness 検出 hook\n\n本文";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 1);
        assert!(
            keywords[0].contains("working copy staleness"),
            "got: {:?}",
            keywords
        );
        assert!(!keywords[0].contains("順位 136"));
    }

    #[test]
    fn extract_heading_keywords_handles_multiple_headings() {
        let text = "### 順位 1 first heading\n\n### 順位 2 second heading\n";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 2);
        assert!(keywords[0].contains("first heading"));
        assert!(keywords[1].contains("second heading"));
    }

    #[test]
    fn extract_heading_keywords_returns_empty_when_no_headings() {
        let text = "## sub heading\nplain text without ### prefix";
        assert!(extract_heading_keywords(text).is_empty());
    }

    #[test]
    fn extract_heading_keywords_filters_too_short() {
        let text = "### \n### ab\n### 順位 1 longer title";
        let keywords = extract_heading_keywords(text);
        assert_eq!(keywords.len(), 1);
        assert!(keywords[0].contains("longer title"));
    }

    #[test]
    fn parse_jj_log_records_basic() {
        let raw = "abc1234\x1ffirst commit description\x1edef5678\x1fsecond commit\x1e";
        let records = parse_jj_log_records(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "abc1234");
        assert_eq!(records[0].1, "first commit description");
        assert_eq!(records[1].0, "def5678");
        assert_eq!(records[1].1, "second commit");
    }

    #[test]
    fn parse_jj_log_records_skips_malformed() {
        let raw = "abc\x1fdesc1\x1eonlyid_no_separator\x1exyz\x1fdesc2\x1e";
        let records = parse_jj_log_records(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "abc");
        assert_eq!(records[1].0, "xyz");
    }

    #[test]
    fn find_matching_commits_case_insensitive() {
        let commits = vec![
            ("abc1".to_string(), "feat: ADD STALENESS hook".to_string()),
            ("abc2".to_string(), "unrelated change".to_string()),
            ("abc3".to_string(), "fix(staleness): tweak".to_string()),
        ];
        let matches = find_matching_commits("staleness", &commits);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn find_matching_commits_limits_to_three() {
        let commits: Vec<_> = (0..5)
            .map(|i| (format!("c{}", i), format!("feat: keyword #{}", i)))
            .collect();
        let matches = find_matching_commits("keyword", &commits);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn first_line_extracts_first_line() {
        assert_eq!(first_line("first\nsecond\nthird"), "first");
        assert_eq!(first_line("single"), "single");
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("  spaced  \nrest"), "spaced");
    }

    #[test]
    fn build_todo_staleness_message_stale_with_matches() {
        let path = build_todo_path("");
        let matches = vec![(
            "test keyword".to_string(),
            vec![("abc1234".to_string(), "feat: implement test".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, Some(3), &matches, "master");
        let msg = msg.expect("message should be generated");
        assert!(msg.contains(&path));
        assert!(msg.contains("3 commits ahead"));
        assert!(msg.contains("関連既実装の可能性"));
        assert!(msg.contains("test keyword"));
        assert!(msg.contains("abc1234"));
    }

    #[test]
    fn build_todo_staleness_message_stale_only() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, Some(2), &[], "main");
        let msg = msg.expect("stale should produce message");
        assert!(msg.contains("main"));
        assert!(msg.contains("2 commits ahead"));
        assert!(!msg.contains("関連既実装の可能性"));
    }

    #[test]
    fn build_todo_staleness_message_grep_only() {
        let path = build_todo_path("");
        let matches = vec![(
            "kw".to_string(),
            vec![("abc1234".to_string(), "feat: kw impl".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, Some(0), &matches, "master");
        let msg = msg.expect("grep match alone should produce message");
        assert!(msg.contains("関連既実装の可能性"));
        assert!(!msg.contains("stale parent detected"));
    }

    #[test]
    fn build_todo_staleness_message_neither_returns_none() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, Some(0), &[], "master");
        assert!(msg.is_none());
    }

    #[test]
    fn build_todo_staleness_message_returns_some_when_behind_is_none() {
        let path = build_todo_path("");
        let msg = build_todo_staleness_message(&path, None, &[], "master");
        let msg = msg.expect("None behind should fail-closed and produce message");
        assert!(msg.contains(&path));
        assert!(msg.contains("判定不能"));
        assert!(msg.contains("fail-closed"));
        assert!(!msg.contains("commits ahead"));
    }

    #[test]
    fn build_todo_staleness_message_behind_none_with_matches_includes_both_sections() {
        let path = build_todo_path("");
        let matches = vec![(
            "kw".to_string(),
            vec![("abc1234".to_string(), "feat: kw impl".to_string())],
        )];
        let msg = build_todo_staleness_message(&path, None, &matches, "master");
        let msg = msg.expect("None behind always produces message regardless of matches");
        assert!(msg.contains("判定不能"));
        assert!(msg.contains("fail-closed"));
        assert!(msg.contains("関連既実装の可能性"));
        assert!(msg.contains("abc1234"));
    }

    #[test]
    fn collect_text_for_keywords_combines_fields() {
        let input = ToolInput {
            command: None,
            file_path: Some(build_todo_path("")),
            path: None,
            old_string: Some("old text".to_string()),
            new_string: Some("new text".to_string()),
            content: Some("full content".to_string()),
        };
        let text = collect_text_for_keywords(&input);
        assert!(text.contains("old text"));
        assert!(text.contains("new text"));
        assert!(text.contains("full content"));
    }

    #[test]
    fn check_todo_staleness_skip_when_disabled() {
        let config = TodoStalenessConfig {
            enabled: Some(false),
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness(&build_todo_path(""), "### something", &config);
        assert!(result.is_none());
    }

    #[test]
    fn check_todo_staleness_skip_when_enabled_field_missing() {
        let config = TodoStalenessConfig {
            enabled: None,
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness(&build_todo_path(""), "### something", &config);
        assert!(result.is_none(), "ADR-039 § 1 準拠で default-OFF");
    }

    #[test]
    fn check_todo_staleness_skip_when_not_todo_path() {
        let config = TodoStalenessConfig {
            enabled: Some(true),
            default_branch: None,
            grep_recent_limit: None,
        };
        let result = check_todo_staleness("docs/adr/adr-041.md", "### test", &config);
        assert!(result.is_none());
    }
}
