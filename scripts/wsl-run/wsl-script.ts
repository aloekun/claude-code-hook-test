/**
 * WSL (Ubuntu) のミラーの中で、任意の bash スクリプトを走らせる。`wsl-run` の cargo のサブコマンドで
 * 足りない操作 (linker の切り替え、ツールの導入など) に使う。
 *
 * 使い方:
 *   pnpm -s wsl-script <file.sh>
 *
 * スクリプトは Write で scratchpad などに書いて渡す。ミラーを更新し、ミラーのディレクトリで
 * `bash <file>` を走らせる。PATH・ミラー・時間の上限の扱いは `wsl-run.ts` と同じ。
 *
 * 任意のコマンドが走るので、`wsl-run` と違って許可リストには入れない (毎回確認が出る)。入口を別の
 * ファイルにしているのは、`pnpm -s wsl-run *` の許可がこの経路に及ばないようにするため。
 */

import { existsSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { UsageError, buildScript, repoWslPath, runInWsl, shQuote, toWslPath } from "./wsl-run.ts";

function main(): void {
  try {
    if (process.platform !== "win32") throw new UsageError("wsl-script は Windows から WSL を呼ぶためのツールです");
    const args = process.argv.slice(2);
    if (args.length !== 1) throw new UsageError("使い方: pnpm -s wsl-script <file.sh>");
    const file = resolve(args[0]);
    if (!existsSync(file)) throw new UsageError(`スクリプトがありません: ${args[0]}`);
    process.exitCode = runInWsl(buildScript(repoWslPath(), `bash ${shQuote(toWslPath(file))}`));
  } catch (e) {
    process.stderr.write(`wsl-script: ${(e as Error).message}\n`);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main();
}
