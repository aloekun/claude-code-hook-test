/**
 * Claude Code Web (クラウドセッション) 用の hook dispatcher (ADR-060)。
 *
 * .claude/settings.json (tracked) の hooks が全イベントで本スクリプトを起動し、
 * 本スクリプトが .claude/ 配下の hook バイナリへ委譲する。登録 (tracked settings) と
 * 実体 (毎セッション消えるバイナリ) のライフサイクル分離が目的。設計根拠・実測事実は
 * docs/adr/adr-060-cloud-harness-sessionstart-dispatcher.md 参照。
 *
 * 使い方 (settings.json の hooks command から):
 *   node scripts/cloud-hook-dispatch.mjs --setup            … SessionStart 専用
 *   node scripts/cloud-hook-dispatch.mjs <hook-exe-name>    … その他の hook イベント
 *
 * ゲート (ADR-039 3 点セット、opt-in):
 *   - CLAUDE_CODE_REMOTE=true (クラウドセッションで自動設定) でなければ即 exit 0。
 *     Windows ローカルでは settings.local.json (テンプレート生成) が実働するため、
 *     本スクリプトは常に不活性 (無音。ローカルの全イベントで発火するためログは出さない)。
 *   - CLOUD_HARNESS が "1" または "true" (case-insensitive) でなければ即 exit 0。
 *     有効化は Web UI の環境変数欄で行う。kill-switch = この env を削除。
 *   - bounded lifetime decision trigger: クラウドセッション 5 回の dogfood で
 *     「SessionStart 完走 + Pre/Post/Stop 発火 + Stop gate 完走」を確認したら
 *     default-ON 昇格 or 却下を判定。2026-09-30 までに判定なしなら却下 (ADR-060 § 決定 4)。
 *
 * バイナリ不在時は exit 0 + stderr 警告 (fail-open)。ADR-043 からの意図的逸脱で、
 * 理由 (復旧コマンド自体が block されるデッドロック回避) は ADR-060 § 決定 5 参照。
 *
 * run-artifact.mjs と同じ自己位置解決 (import.meta.url) を使うが、失敗セマンティクスが
 * 異なる (あちらは不在 = exit 1 の CLI ランチャー) ため別スクリプト (ADR-044)。
 */

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const CLAUDE_DIR = join(ROOT, ".claude");

// ─── ゲート (ADR-060 § 決定 4) ───

const enableValue = (process.env.CLOUD_HARNESS ?? "").toLowerCase();
const enabled = enableValue === "1" || enableValue === "true";
if (process.env.CLAUDE_CODE_REMOTE !== "true" || !enabled) {
  process.exit(0);
}

const [mode, ...forwarded] = process.argv.slice(2);
if (!mode) {
  console.error("usage: node scripts/cloud-hook-dispatch.mjs <--setup | hook-exe-name> [args...]");
  process.exit(2);
}

/** hook バイナリを stdin 指定つきで起動し exit code を正規化して返す。 */
function runHookBinary(name, args, stdinData) {
  const exePath = join(CLAUDE_DIR, name);
  if (!existsSync(exePath)) {
    // fail-open (exit 0)。無言ではなく毎イベント警告する (ADR-060 § 決定 5)。
    console.error(
      `[cloud-hook-dispatch] WARNING: ${exePath} が未配置のため hook をスキップしました。` +
        "SessionStart の setup が失敗しています。復旧: bash scripts/cloud-setup.sh --session-phase",
    );
    return 0;
  }
  const result = spawnSync(exePath, args, {
    stdio: [stdinData === undefined ? "inherit" : "pipe", "inherit", "inherit"],
    input: stdinData,
  });
  if (result.error) {
    console.error(`[cloud-hook-dispatch] ${name} の起動に失敗: ${result.error.message}`);
    return 1;
  }
  // シグナル終了は status = null になるため非ゼロへ正規化 (run-artifact.mjs と同じ規約)。
  return result.status === null ? 1 : result.status;
}

// ─── --setup: SessionStart 専用モード ───
//
// (1) cloud-setup.sh --session-phase でバイナリ配置・jj・pnpm install を実行し、
// (2) 成功後に hooks-session-start バイナリへ SessionStart の stdin JSON を渡す。
// 2 つを同一エントリで直列実行するのは、settings 側で別エントリに分けると並列発火し
// 「配置前にバイナリを呼ぶ」race があるため (ADR-060 § 決定 2)。
if (mode === "--setup") {
  // hooks-session-start 用に stdin を先に全量 buffer する (子 2 つで共有できないため)。
  let stdinJson = "";
  try {
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    stdinJson = Buffer.concat(chunks).toString("utf8");
  } catch {
    stdinJson = "";
  }

  const setup = spawnSync("bash", [join(ROOT, "scripts", "cloud-setup.sh"), "--session-phase"], {
    stdio: ["ignore", "inherit", "inherit"],
  });
  const setupStatus = setup.error ? 1 : (setup.status ?? 1);
  if (setupStatus !== 0) {
    // setup 失敗はここで明示的に落とす (fail-closed)。以降の hook イベントは
    // バイナリ不在の warning (fail-open) として観測される。
    console.error(
      `[cloud-hook-dispatch] cloud-setup.sh --session-phase が exit ${setupStatus} で失敗しました。` +
        "ハーネスは無効のままです。復旧: bash scripts/cloud-setup.sh --session-phase",
    );
    process.exit(2);
  }

  process.exit(runHookBinary("hooks-session-start", [], stdinJson));
}

// ─── 通常モード: <hook-exe-name> [args...] ───

// settings.json (tracked) 以外から名前が来ることは想定しないが、path traversal だけは
// 機械的に拒否する (テンプレートの exe 名文字種と同じ許容集合)。
if (!/^[A-Za-z0-9._-]+$/.test(mode)) {
  console.error(`[cloud-hook-dispatch] 不正な hook 名: ${mode}`);
  process.exit(2);
}

process.exit(runHookBinary(mode, forwarded, undefined));
