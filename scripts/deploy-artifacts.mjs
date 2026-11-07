/**
 * ビルド済み Rust 成果物を `<target dir>/release/` から .claude/ へコピーする
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

import { copyFileSync, existsSync, rmSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { computeFingerprints, formatStamp, loadWorkspace, STAMP_SUFFIX } from "./exe-fingerprint.mjs";
import { renameWithRetry } from "./rename-with-retry.mjs";

/** OS 依存の実行ファイル拡張子 (Windows: ".exe" / それ以外: "")。 */
const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

const SCRIPTS_DIR = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, "..");
const CLAUDE_DIR = join(ROOT, ".claude");

/**
 * cargo の成果物ディレクトリ (`<target dir>/release`) を返す (順位 578)。
 *
 * `CARGO_TARGET_DIR` が設定されていればそれを使い、無ければ `<root>/target` を使う。
 * 相対パスは `root` 基準で解決する (cargo は cwd 基準だが、本スクリプト群は `pnpm` 経由で
 * リポジトリルートから呼ばれる)。決め打ちの `target/release` だと、クラウド環境
 * (`CARGO_TARGET_DIR=/opt/cargo-target`) で `cargo build` の成果物を見つけられない。
 *
 * @param {string} root リポジトリルート
 * @param {Record<string, string | undefined>} env 環境変数 (テストで差し替える)
 */
export function resolveReleaseDir(root, env = process.env) {
  const targetDir = env.CARGO_TARGET_DIR;
  return join(targetDir ? resolve(root, targetDir) : join(root, "target"), "release");
}

/**
 * `dest` を `src` の中身へ差し替える。`dest` が実行中でも失敗しない。
 *
 * 実行中の exe は Windows では上書きもできず、Linux では上書きすると ETXTBSY になる。
 * どちらの OS でも**改名はできる**ので、新しい中身を `<dest>.new` へ置いてから、
 * 旧い `dest` を `<dest>.old` へ退避し、`<dest>.new` を `dest` へ改名する。実行中の
 * プロセスは退避した旧いファイルを使い続ける。
 *
 * **不変条件: 旧い中身は、どの時点でも `dest` か `<dest>.old` のどちらかにある。**
 * `<dest>.old` を消すのは `dest` が新しい中身になった後だけ。旧い exe を失うと、その exe を
 * 使う hook が次から毎回「見つからない」で失敗する (CodeRabbit #516)。
 *
 * | 段 | 処理 | 失敗したとき |
 * |---|---|---|
 * | 1 | 前回の `.old` を削除 (1 回だけ) | 続ける (まだ実行中なら消せないのが正常) |
 * | 2 | `src` を `.new` へコピー | `dest` はそのまま。`.new` を消して投げる |
 * | 3 | `dest` を `.old` へ退避 (再試行あり) | `dest` はそのまま。`.new` を消して投げる。`dest` が外から消えていたら退避せずに段 4 へ |
 * | 4 | `.new` を `dest` へ改名 (再試行あり) | 退避していれば段 5 へ。`dest` が外から作られていたら、中身が新旧どちらとも言えないので `.old` を残し、`.new` を消して投げる |
 * | 5 | `.old` を `dest` へ戻す (再試行あり) | 戻せたら段 4 のエラーを投げる。戻せなければ `.old` と `.new` を残し、手で戻す手順を載せて投げる |
 * | 6 | `.old` を削除 (1 回だけ) | 残す (実行中なら消せないのが正常。次回の段 1 で消す) |
 *
 * 段 1 と段 6 を再試行しないのは、旧い exe がまだ実行中なら消せないのが正常で、待つと
 * 毎回 2 秒を使ってから諦めることになるため。
 *
 * **配布方式を変えない理由 (2026-11-07)**: 版ごとのディレクトリへ置いて
 * 指し先を切り替える方式なら実行中の exe を改名せずに済むが、hook の exe パスは
 * テンプレートから固定名で生成しており (ADR-005)、起動経路をすべて変えることになる。
 * 一時的な lock は 200 回に 1 回程度の観測しかないので、境界での再試行で吸収する。
 * `[rename-retry] ... 諦めた` の記録が繰り返し出るようなら、この判断を見直す。
 *
 * @param {string} src
 * @param {string} dest
 * @param {object} [options] テストで差し替える口 (`renameWithRetry` へそのまま渡す)
 */
export function replaceFile(src, dest, options = {}) {
  const staged = `${dest}.new`;
  const aside = `${dest}.old`;
  const rename = (from, to, label) => renameWithRetry(from, to, { ...options, label });
  removeBestEffort(aside);
  try {
    copyFileSync(src, staged);
  } catch (err) {
    removeBestEffort(staged);
    throw err;
  }
  const hasAside = existsSync(dest) && moveAside(dest, aside, staged, rename);
  try {
    rename(staged, dest, "差し替え");
  } catch (err) {
    if (hasAside && !existsSync(dest)) {
      restoreOrExplain(aside, dest, staged, err, rename);
    }
    removeBestEffort(staged);
    throw err;
  }
  removeBestEffort(aside);
}

/**
 * 段 3: `dest` を `.old` へ退避する。退避したら `true`。
 *
 * 確かめた後に `dest` が外から消えていた (`ENOENT`) ときは `false` を返して続ける — 退避すべき
 * 旧い中身がもう無いだけなので、`dest` が無かった場合と同じく新しい中身を置く。ここで投げると
 * exe が 1 つも無い状態を残す (順位 565)。それ以外の失敗は `.new` を消して投げる。
 */
function moveAside(dest, aside, staged, rename) {
  try {
    rename(dest, aside, "退避");
    return true;
  } catch (err) {
    if (err.code === "ENOENT") return false;
    removeBestEffort(staged);
    throw err;
  }
}

/**
 * 段 5: 退避した旧い中身を `dest` へ戻す。戻せなければ、旧い中身の在り処と手で戻す手順を
 * 載せて投げる (このとき `.old` と `.new` は消さない)。
 */
function restoreOrExplain(aside, dest, staged, replaceErr, rename) {
  try {
    rename(aside, dest, "復元");
  } catch (restoreErr) {
    const err = new Error(
      `${dest} の差し替えに失敗し、旧い中身も戻せませんでした。旧い中身は ${aside}、` +
        `新しい中身は ${staged} に残っています。手で ${aside} を ${dest} へ改名してください。\n` +
        `  差し替えの失敗: ${replaceErr.message}\n  復元の失敗: ${restoreErr.message}`,
      { cause: replaceErr },
    );
    err.code = replaceErr.code;
    throw err;
  }
}

/** 消せなくても続ける削除 (段 1・2・3・6。消せないのが正常な場面がある)。 */
function removeBestEffort(path) {
  try {
    rmSync(path, { force: true });
  } catch {
    // 実行中などで消せない。次回の差し替えで消す。
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
    deployArtifacts(names, fingerprints, { releaseDir: resolveReleaseDir(ROOT), claudeDir: CLAUDE_DIR });
  } catch (err) {
    console.error(`error: ${err.message}`);
    return 1;
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
