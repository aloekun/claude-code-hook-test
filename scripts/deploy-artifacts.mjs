/**
 * ビルド済み Rust 成果物を target/release/ から .claude/ へコピーする
 * クロスプラットフォームスクリプト (WP-13: EXE_SUFFIX 抽象化)。
 *
 * 従来 package.json の build:* スクリプトは `cp target/release/<name>.exe .claude/`
 * を使っていたが、これは (1) `.exe` を Windows 決め打ち (2) Git for Windows の
 * usr/bin (`cp.exe`) を PATH に要求する、という 2 つの可搬性の壁があった。
 * 本スクリプトは `process.platform` から実行ファイル拡張子を解決し、Node の
 * copyFileSync でコピーするため、両方の壁を構造的に解消する。
 *
 * コピーした exe ごとに入力フィンガープリントを `.claude/<crate-name>.fingerprint` へ
 * 記録する (ADR-082)。Stop 品質ゲートの `exe-freshness` step がこれを今のソースと
 * 突き合わせ、古い exe を検出して再ビルドする (`rebuild-stale-exes.mjs` も本モジュールの
 * `deployArtifacts` を使う)。
 *
 * 使い方: node scripts/deploy-artifacts.mjs <crate-name> [<crate-name> ...]
 *   例: node scripts/deploy-artifacts.mjs hooks-stop-quality
 */

import { copyFileSync, existsSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { computeFingerprints, formatStamp, loadWorkspace, STAMP_SUFFIX } from "./exe-fingerprint.mjs";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const RELEASE_DIR = join(ROOT, "target", "release");
const CLAUDE_DIR = join(ROOT, ".claude");

/**
 * `dest` を `src` の中身へ差し替える。`dest` が実行中でも失敗しない。
 *
 * 実行中の exe は Windows では上書きもできず、Linux では上書きすると ETXTBSY になる。
 * どちらの OS でも**改名はできる**ので、新しい中身を `<dest>.new` へ置いてから、
 * 旧い `dest` を `<dest>.old` へ退避し、`<dest>.new` を `dest` へ改名する。実行中の
 * プロセスは退避した旧いファイルを使い続ける。`<dest>.old` は次回の差し替えで消す
 * (まだ実行中で消せなければ残す)。
 *
 * 退避の後で `<dest>.new` の改名に失敗したら、旧い `dest` を戻してから投げる。戻さないと
 * exe が無い状態が残り、その exe を使う hook が次から毎回「見つからない」で失敗する
 * (CodeRabbit #516)。`rename` はテストが 2 回目の改名だけを失敗させるための差し替え口。
 */
export function replaceFile(src, dest, rename = renameSync) {
  const staged = `${dest}.new`;
  const aside = `${dest}.old`;
  try {
    rmSync(aside, { force: true });
  } catch {
    // 前回退避した exe がまだ実行中。下の rename が失敗すれば、そこで理由が出る。
  }
  copyFileSync(src, staged);
  const hadDest = existsSync(dest);
  if (hadDest) {
    rename(dest, aside);
  }
  try {
    rename(staged, dest);
  } catch (err) {
    if (hadDest && !existsSync(dest)) {
      try {
        rename(aside, dest);
      } catch {
        // 復元にも失敗した。元の改名エラーのほうが原因を表すので、そちらを投げる。
      }
    }
    throw err;
  }
  try {
    rmSync(aside, { force: true });
  } catch {
    // 実行中なので残す。次回の差し替えで消す。
  }
}

/**
 * `names` の exe を `releaseDir` から `claudeDir` へ置き、記録を書く。問題があれば投げる。
 *
 * @param {string[]} names crate (= bin) 名
 * @param {Map<string, { fingerprint: string }>} fingerprints 記録する値。呼び出し側が
 *   **ビルド前**に計算した値を渡すと、ビルド中にソースが変わっても古い exe に新しい
 *   記録が付かない (ADR-082 § 既知の限界)
 */
export function deployArtifacts(names, fingerprints, { releaseDir, claudeDir }) {
  for (const name of names) {
    const fileName = `${name}${EXE_SUFFIX}`;
    const src = join(releaseDir, fileName);
    if (!existsSync(src)) {
      throw new Error(
        `build artifact not found: ${src}\n       run the corresponding \`cargo build --release -p ${name}\` first`,
      );
    }
    const entry = fingerprints.get(name);
    if (!entry) {
      throw new Error(`${name} is not a bin target of this workspace; cannot record its fingerprint`);
    }
    replaceFile(src, join(claudeDir, fileName));
    // exe の差し替えが済んでから記録する。差し替えに失敗したら旧い記録が旧い exe に残る。
    writeFileSync(join(claudeDir, `${name}${STAMP_SUFFIX}`), formatStamp(entry.fingerprint));
    console.log(`deployed: ${fileName} -> .claude/`);
  }
}

function main() {
  const names = process.argv.slice(2);
  if (names.length === 0) {
    console.error("usage: node scripts/deploy-artifacts.mjs <crate-name> [<crate-name> ...]");
    return 2;
  }

  // 手動の `pnpm build:*` はビルドの後に本スクリプトを呼ぶので、ビルド直後のツリーから
  // 計算する。ビルドから deploy までの数秒の間にソースを編集すると、古い exe に新しい
  // 記録が付く (ADR-082 § 既知の限界。Stop の自動再ビルドはビルド前の値を渡して塞ぐ)。
  let fingerprints;
  try {
    const { graph, workspaceRoot } = loadWorkspace(ROOT);
    fingerprints = computeFingerprints(graph, workspaceRoot);
  } catch (err) {
    console.error(`error: cannot compute exe fingerprints (cargo metadata failed?): ${err.message}`);
    return 1;
  }
  try {
    deployArtifacts(names, fingerprints, { releaseDir: RELEASE_DIR, claudeDir: CLAUDE_DIR });
  } catch (err) {
    console.error(`error: ${err.message}`);
    return 1;
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
