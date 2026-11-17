/**
 * WSL (Ubuntu) で、このリポジトリの cargo コマンドを走らせる専用ツール。実 Linux での検証と、Linux 向け
 * CI の性能の試行に使う。
 *
 * WSL を直接呼ぶと、次の落とし穴を毎回踏む。このツールはそれを全部コードの側で吸収する。hook の preset
 * `wsl-direct-block` が `wsl` の直接実行を止めて、このツールへ誘導する。
 *
 * - Git Bash から呼ぶと、`/mnt/c/...` の引数が Windows のパスに書き換えられ、WSL 側でファイルが
 *   見つからない (2026-11-16、順位 423 の検証)。ここでは node から wsl.exe を argv で直接起動し、
 *   パスの変換もここで行う
 * - PowerShell から `wsl -- bash -lc '複数行'` を渡すと引数が壊れる (`$HOME` が空になる)。ここでは
 *   スクリプトを一時ファイルに書いて `bash <file>` で渡す
 * - `/mnt/c` (drvfs) の上で直接ビルドすると遅く、fd の継承や `ETXTBSY` のような POSIX 固有の競合が
 *   再現しない (2026-08-19 の順位 396 は ext4 で 43/200 再現し、`/mnt/c` では 0)。ここでは ext4 の
 *   `~/mirror/<リポジトリ名>` に rsync で複製してから走らせる。`target` はミラーに残すので差分ビルドが効く
 * - rustup の cargo (`~/.cargo/bin`) と jj (`~/.local/bin`) はログインシェルの PATH に載っていない。
 *   既定の PATH には Windows 側の PATH も混ざる (Windows の npx を拾う)。ここでは PATH を明示して置き換える
 *
 * 使い方:
 *   pnpm -s wsl-run sync                                   ミラーを更新するだけ
 *   pnpm -s wsl-run [--env NAME=VALUE]... cargo <sub> ...  ミラーを更新して cargo を走らせる
 *
 * `<sub>` は test / build / check / clippy / nextest。出力はそのまま流し、最後に終了コードと所要時間を
 * 1 行出す。終了コードは cargo のもの (使い方の誤りは 2)。
 *
 * # 確認なしで走るための制限
 *
 * このコマンドは `.claude/settings.json` の許可リストにあり、確認なしで走る。WSL の中からは Windows の
 * ファイル (`/mnt/c`) も書けるので、任意のコマンドを走らせる経路にしない:
 *
 * - 走らせるのは上の cargo のサブコマンドだけ。`--config` / `-Z` / `--manifest-path` / `+toolchain` と、
 *   nextest の `--config-file` / `--tool-config-file` は受け付けない (任意のプログラムを起動できるため)
 * - `--env` で渡せる環境変数は、ビルドとテストの性能に関わるものだけ (`ALLOWED_ENV`)。`RUSTFLAGS` /
 *   `RUSTC_WRAPPER` / `CARGO_TARGET_*_RUNNER` のような、プログラムを指定できるものは受け付けない
 * - 任意のスクリプトは `pnpm -s wsl-script <file.sh>` (`wsl-script.ts`) で走らせる。こちらは許可リストに
 *   入れないので、毎回確認が出る
 */

import { spawnSync } from "node:child_process";
import { realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const REPO_ROOT = fileURLToPath(new URL("../..", import.meta.url));

/** 検証に使う WSL のディストロ。rustup / build-essential / jj 0.42 を入れてある。 */
export const DISTRO = "Ubuntu-24.04";

/** WSL の中で走らせる時間の上限 (秒)。WSL 側の `timeout` が止める。 */
export const TIMEOUT_SECS = 30 * 60;

export class UsageError extends Error {}

export type Request = { kind: "sync" } | { kind: "cargo"; env: [string, string][]; args: string[] };

const USAGE = "使い方: pnpm -s wsl-run sync | pnpm -s wsl-run [--env NAME=VALUE]... cargo <test|build|check|clippy|nextest> ...";

const CARGO_SUBCOMMANDS = new Set(["test", "build", "check", "clippy", "nextest"]);

const FORBIDDEN_CARGO_ARGS = /^(?:--config(?:=|$)|-Z|--manifest-path(?:=|$)|\+|--config-file(?:=|$)|--tool-config-file(?:=|$))/;

/**
 * `--env` で渡せる環境変数。ビルドの設定 (並列度・増分・プロファイルの最適化とデバッグ情報) と
 * テストの実行 (スレッド数・バックトレース) に限る。プロファイルの設定は、プログラムを指定できない
 * キーだけを許す。
 */
export const ALLOWED_ENV =
  /^(?:CARGO_INCREMENTAL|CARGO_BUILD_JOBS|CARGO_PROFILE_[A-Z0-9_]+_(?:DEBUG|OPT_LEVEL|CODEGEN_UNITS|INCREMENTAL|STRIP|LTO|SPLIT_DEBUGINFO)|RUST_TEST_THREADS|RUST_BACKTRACE|NEXTEST_[A-Z_]*(?:THREADS|RETRIES|PROFILE))$/;

export function parseArgs(args: string[]): Request {
  if (args.length === 1 && args[0] === "sync") return { kind: "sync" };
  const env: [string, string][] = [];
  let i = 0;
  while (args[i] === "--env") {
    const pair = args[i + 1];
    const eq = pair?.indexOf("=") ?? -1;
    if (pair === undefined || eq <= 0) throw new UsageError(`--env は NAME=VALUE の形で渡します: ${pair ?? ""}`);
    const name = pair.slice(0, eq);
    if (!ALLOWED_ENV.test(name)) {
      throw new UsageError(`環境変数 ${name} は受け付けません (ビルドとテストの性能に関わるものだけ。wsl-run.ts の ALLOWED_ENV)`);
    }
    env.push([name, pair.slice(eq + 1)]);
    i += 2;
  }
  const rest = args.slice(i);
  if (rest[0] !== "cargo" || !CARGO_SUBCOMMANDS.has(rest[1] ?? "")) throw new UsageError(USAGE);
  const bad = rest.slice(2).find((a) => FORBIDDEN_CARGO_ARGS.test(a));
  if (bad !== undefined) {
    throw new UsageError(`cargo の ${bad} は受け付けません (確認なしで任意のプログラムを起動できるため)`);
  }
  return { kind: "cargo", env, args: rest.slice(1) };
}

/** Windows の絶対パスを WSL の `/mnt/<drive>/...` に変換する。 */
export function toWslPath(windowsPath: string): string {
  const m = /^([A-Za-z]):[\\/]?(.*)$/.exec(windowsPath);
  if (!m) throw new UsageError(`Windows の絶対パスではありません: ${windowsPath}`);
  const rest = m[2].replaceAll("\\", "/").replace(/\/+$/, "");
  return `/mnt/${m[1].toLowerCase()}${rest === "" ? "" : `/${rest}`}`;
}

/** bash の単語として安全に埋め込む (単一引用符で囲み、中の `'` は `'\''` にする)。 */
export function shQuote(s: string): string {
  return `'${s.replaceAll("'", `'\\''`)}'`;
}

/**
 * WSL で走らせる bash スクリプトを組み立てる。ミラーの更新と、`command` (bash の 1 行) の実行。
 *
 * rsync で除くのは、ビルドの出力 (`target`、ミラー側のものを残して差分ビルドを効かせる)、依存
 * (`node_modules`)、VCS (`.git` / `.jj`) と、takt の実行記録 (`.takt/runs`)。`--delete` で、
 * リポジトリから消えたファイルはミラーからも消す。
 */
export function buildScript(repoWslPath: string, command: string | undefined): string {
  const mirror = `$HOME/mirror/${basename(repoWslPath)}`;
  const lines = [
    "set -euo pipefail",
    'export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"',
    `mirror="${mirror}"`,
    'mkdir -p "$mirror"',
    "t0=$(date +%s%N)",
    `rsync -a --delete --exclude=/.git --exclude=/.jj --exclude=/.takt/runs --exclude=target/ --exclude=node_modules/ ${shQuote(`${repoWslPath}/`)} "$mirror/"`,
    `echo "[wsl-run] ミラーを更新: $mirror ($(( ($(date +%s%N) - t0) / 1000000 )) ms)" >&2`,
  ];
  if (command !== undefined) {
    lines.push(
      'cd "$mirror"',
      "t1=$(date +%s%N)",
      "set +e",
      `timeout --kill-after=10s ${TIMEOUT_SECS}s ${command}`,
      "status=$?",
      "set -e",
      'ms=$(( ($(date +%s%N) - t1) / 1000000 ))',
      `if [ "$status" -eq 124 ]; then echo "[wsl-run] ${TIMEOUT_SECS} 秒で終わらなかったので止めました" >&2; fi`,
      'echo "[wsl-run] 終了コード $status、$((ms / 1000)).$(printf %03d $((ms % 1000))) 秒" >&2',
      'exit "$status"',
    );
  }
  return `${lines.join("\n")}\n`;
}

/** cargo の要求を、bash の 1 行 (`env NAME=VALUE ... cargo ...`) にする。 */
export function cargoCommand(env: [string, string][], args: string[]): string {
  const prefix = env.length === 0 ? "" : `env ${env.map(([k, v]) => shQuote(`${k}=${v}`)).join(" ")} `;
  return `${prefix}cargo ${args.map(shQuote).join(" ")}`;
}

/**
 * スクリプトを一時ファイルに書き、WSL の bash で走らせて終了コードを返す。出力はそのまま流す。
 * `--exec` で既定のシェルを通さずに bash を起動するので、引数がもう一度解釈されることは無い。
 */
export function runInWsl(script: string): number {
  const file = join(tmpdir(), `claude-wsl-run-${process.pid}.sh`);
  writeFileSync(file, script.replaceAll("\r\n", "\n"), "utf8");
  try {
    const result = spawnSync("wsl.exe", ["-d", DISTRO, "--exec", "bash", toWslPath(file)], {
      stdio: "inherit",
      windowsHide: true,
      timeout: (TIMEOUT_SECS + 120) * 1000,
    });
    if (result.error) throw result.error;
    return result.status ?? 1;
  } finally {
    rmSync(file, { force: true });
  }
}

export function repoWslPath(): string {
  return toWslPath(resolve(REPO_ROOT));
}

function main(): void {
  try {
    if (process.platform !== "win32") throw new UsageError("wsl-run は Windows から WSL を呼ぶためのツールです");
    const request = parseArgs(process.argv.slice(2));
    const command = request.kind === "cargo" ? cargoCommand(request.env, request.args) : undefined;
    process.exitCode = runInWsl(buildScript(repoWslPath(), command));
  } catch (e) {
    process.stderr.write(`wsl-run: ${(e as Error).message}\n`);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main();
}
