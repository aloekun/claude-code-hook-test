/**
 * 変異確認 (実装を一時的に壊して、テストが落ちることを確かめる) の専用ツール。
 *
 * 「壊す → テストを走らせる → 必ず元に戻す」を 1 コマンドで行う。シェルで
 * `f=...; cp $f $S/x.bak && sed -i ... $f && cargo test ...; cp $S/x.bak $f` と組むと、書き込み先が
 * シェル変数になり、auto mode で毎回ユーザーへの確認が出る (2026-11-14 の transcript で 3 件中 3 件。
 * 書き込み先をリテラルで書いた 5 件は出なかった)。preset `variable-write-target-block` がその形を止めて、
 * このツールへ誘導する。
 *
 * 書き込みと子プロセスの起動をするので、読み取り専用ツールの置き場 (`scripts/tools/`、`readonly.test.ts` が
 * 検査する) には置かない。読んでよい場所の判定だけ `scripts/tools/read-scope.ts` を使う。
 *
 * 使い方:
 *   pnpm -s mutate <spec.json> -- cargo test <args...>
 *
 * spec は `{"mutants":[{"name":"M1","file":"src/a.rs","from":"...","to":"..."}]}` (scratchpad に Write で
 * 書く)。変異は 1 つずつ当て、テストを走らせ、戻してから次へ進む。
 *
 * 置換の文字列をコマンドの引数で受け取らないのは、Windows では pnpm が引数を cmd.exe 経由で渡すため。
 * 実装の断片には `|` `>` `&` `^` `%` がよく現れ、cmd が解釈すると `| was unexpected at this time.` で
 * 止まるか、`>` の後ろの名前で空ファイルができる (2026-11-15 に実走で踏んだ。PR #517 の
 * `setTimeout(d` と同じ型)。ファイルで渡せば置換の文字列はシェルを通らない。
 *
 * 結果は変異ごとに 1 行で出す: `KILLED` (テストが落ちた = テストは変異を捕まえる)、`SURVIVED`
 * (テストが通った = テストに穴がある)、`NO_COMPILE` (変異でビルドが通らない = 確かめられていない)、
 * `NO_RUN` (cargo の引数の誤りなどでテストが走らなかった = 確かめられていない)。
 * 終了コードは、全変異が KILLED なら 0、そうでなければ 1、使い方の誤りは 2。
 *
 * 同じリポジトリでは同時に 1 つしか走らない (OS の一時ディレクトリにロックファイルを置く)。
 *
 * # 確認なしで走るための制限
 *
 * 実行コマンドは `.claude/settings.json` の許可リストにあり、確認なしで走る。そのため:
 *
 * - 書き換えるファイルはリポジトリの配下だけ。`.git` / `.jj` / `.claude` / `node_modules` / `target` の
 *   配下は書き換えない (hook の設定や VCS の状態を変える経路にしない)
 * - 走らせるのは `cargo test` だけ。シェルを通さず argv で起動する。`--config` / `-Z` /
 *   `--manifest-path` / `+toolchain` は受け付けない (`--config target.<triple>.runner=...` で任意の
 *   プログラムを確認なしに起動できてしまうため)
 * - 置換元の文字列は、ファイルの中にちょうど 1 回現れなければならない (どこを壊したかを一意にする)
 * - 元の内容はバイト列のまま持ち、テストが例外で終わっても `finally` で書き戻す。Ctrl+C とタイムアウト
 *   (15 分) では、テストをプロセスツリーごと止め、cargo が終わった時点で例外にして変異ループを止める。
 *   `jj restore` は使わない (コミットしていない編集まで親の内容に戻してしまうため)
 * - `--no-run` は受け付けない (テストを走らせないので、変異を確かめられない)
 */

import { linkSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { pathToFileURL } from "node:url";

import { REPO_ROOT, assertReadable } from "../tools/read-scope.ts";

export class UsageError extends Error {}

export type Mutant = { name: string; file: string; from: string; to: string };

export type Args = { specFile: string; command: string[] };

export type Verdict = "KILLED" | "SURVIVED" | "NO_COMPILE" | "NO_RUN";

export type RunResult = { status: number | null; output: string };

const USAGE = "使い方: pnpm -s mutate <spec.json> -- cargo test <args...>";

/** `--` の前を spec のパス、後ろをテストコマンドとして読む。spec の中身はここでは読まない。 */
export function parseArgs(args: string[]): Args {
  const sep = args.indexOf("--");
  if (sep !== 1) throw new UsageError(USAGE);
  const command = args.slice(sep + 1);
  checkCommand(command);
  return { specFile: args[0], command };
}

const FORBIDDEN_CARGO_ARGS = /^(?:--config(?:=|$)|-Z|--manifest-path(?:=|$)|\+|--no-run$)/;

/** テストコマンドが `cargo test ...` で、任意のプログラムを起動できる引数を含まないことを確かめる。 */
export function checkCommand(command: string[]): void {
  if (command[0] !== "cargo" || command[1] !== "test") {
    throw new UsageError(`テストコマンドは cargo test だけを受け付けます: ${command.join(" ")}`);
  }
  const bad = command.slice(2).find((a) => FORBIDDEN_CARGO_ARGS.test(a));
  if (bad !== undefined) {
    throw new UsageError(`cargo の ${bad} は受け付けません (確認なしで任意のプログラムを起動できるため)`);
  }
}

/** spec の JSON を変異の一覧として読む。形が違えば UsageError。 */
export function parseSpec(text: string): Mutant[] {
  let spec: unknown;
  try {
    spec = JSON.parse(text);
  } catch (e) {
    throw new UsageError(`spec が JSON として読めません: ${(e as Error).message}`);
  }
  const mutants = (spec as { mutants?: unknown })?.mutants;
  if (!Array.isArray(mutants) || mutants.length === 0) {
    throw new UsageError('spec には空でない "mutants" 配列が要ります');
  }
  return mutants.map((m, i) => {
    const { name, file, from, to } = (m ?? {}) as Record<string, unknown>;
    if (typeof file !== "string" || typeof from !== "string" || typeof to !== "string") {
      throw new UsageError(`mutants[${i}] には文字列の file / from / to が要ります`);
    }
    if (from === to) throw new UsageError(`mutants[${i}] の from と to が同じです`);
    return { name: typeof name === "string" ? name : `M${i + 1}`, file, from, to };
  });
}

const FORBIDDEN_DIRS = /^(?:\.git|\.jj|\.claude|node_modules|target)(?:[/\\]|$)|[/\\](?:node_modules|target)(?:[/\\]|$)/i;

/** 書き換えてよいファイルなら実パスを返す。リポジトリの外や、守る場所の配下なら例外。 */
export function assertWritable(file: string, repoRoot: string = REPO_ROOT): string {
  const real = assertReadable(file, [repoRoot]);
  const rel = relative(realpathSync.native(repoRoot), real);
  if (FORBIDDEN_DIRS.test(rel)) {
    throw new UsageError(`書き換えない場所です: ${file} (.git / .jj / .claude / node_modules / target)`);
  }
  return real;
}

/** `from` がちょうど 1 回現れる `content` の、その 1 か所を `to` に置き換える。 */
export function applyMutation(content: string, from: string, to: string): string {
  const first = content.indexOf(from);
  if (from === "" || first === -1) throw new UsageError(`置換元が見つかりません: ${from}`);
  if (content.indexOf(from, first + 1) !== -1) {
    throw new UsageError(`置換元が 2 回以上現れます。前後を含めて一意にしてください: ${from}`);
  }
  return content.slice(0, first) + to + content.slice(first + from.length);
}

/**
 * cargo の終了状態と出力から判定する。KILLED と数えるのは、テストが落ちた証拠があるときだけ。
 *
 * - ビルドの失敗は、コンパイラのエラー (`error[E0308]:`) と `could not compile` で見る。テストが
 *   落ちたときも cargo は `error: test failed, to rerun ...` を出すので、`error:` だけでは区別できない
 * - テストが落ちた証拠は `test result: FAILED` と `error: test failed` / `error: doctest failed`。
 *   後者は、テストのバイナリが集計を出す前に異常終了した場合 (stack overflow 等) にも出る
 * - どちらでもない非 0 終了 (`cargo test --no-such-flag` の引数エラーなど) は、テストを走らせて
 *   いないので NO_RUN。KILLED に数えると、何も確かめずに終了コード 0 を返してしまう
 * - 終了コード 0 でも `test result: ok` が無ければ (`-- --list` など) テストは走っていないので NO_RUN。
 *   SURVIVED に数えると、走らせていないテストを「通った」と報告してしまう
 */
export function classify(result: RunResult): Verdict {
  if (result.status === 0) return /^test result: ok/m.test(result.output) ? "SURVIVED" : "NO_RUN";
  if (/^error\[E\d+\]:|could not compile/m.test(result.output)) return "NO_COMPILE";
  if (/^test result: FAILED|^error: (?:test|doctest) failed/m.test(result.output)) return "KILLED";
  return "NO_RUN";
}

/** 結果の要約に出す行 (落ちたテスト・集計・panic の位置)。 */
export function summaryLines(output: string): string[] {
  return output
    .split(/\r?\n/)
    .filter((l) => /^test .* FAILED$|^test result:|panicked at|^error\[E\d+\]:|could not compile/.test(l));
}

/**
 * `file` に変異を当てて `run` を呼び、何があっても元のバイト列を書き戻す。
 * 書き戻した後に読み直して一致しなければ例外 (黙って壊れたまま終わらせない)。
 */
export async function withMutation<T>(
  file: string,
  from: string,
  to: string,
  run: () => T | Promise<T>,
): Promise<T> {
  const original = readFileSync(file);
  const mutated = applyMutation(original.toString("utf8"), from, to);
  let result: T;
  try {
    // 書き込みは既存の内容を先に切り詰めるので、途中で失敗しても `finally` で原本に戻す
    writeFileSync(file, mutated, "utf8");
    result = await run();
  } finally {
    writeFileSync(file, original);
  }
  if (!readFileSync(file).equals(original)) {
    throw new Error(`書き戻した内容が元と一致しません: ${file}`);
  }
  return result;
}

const TIMEOUT_MS = 15 * 60 * 1000;

/** いま走っているテストのプロセス。Ctrl+C を受けたとき、ツリーごと止めるのに使う。 */
let running: ChildProcess | undefined;

/**
 * `pid` とその子孫を止める。cargo を止めるだけでは、テストのバイナリ (cargo の子) が残って
 * 動き続ける。Windows は `taskkill /T` で、POSIX は `detached` で作ったプロセスグループごと止める。
 */
export function killTree(pid: number): void {
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore", windowsHide: true, timeout: 30_000 });
    return;
  }
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    // グループがもう無い (全員終わっている)
  }
}

/**
 * `command` を走らせ、終了状態と出力を返す。中断とタイムアウトは例外。
 *
 * `spawnSync` は使わない。`spawnSync` の `timeout` は cargo だけを止め、テストのバイナリが出力の
 * パイプを持ったまま残ると戻らない。戻らなければ `withMutation` の `finally` が走らず、変異が
 * ソースに残る。ここでは、タイムアウトのときはツリーごと止め、cargo が終わった時点 (`exit`) で
 * 例外にする。パイプが閉じるのは待たない。
 */
export function runCommand(command: string[], timeoutMs: number): Promise<RunResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(command[0], command.slice(1), {
      cwd: REPO_ROOT,
      detached: process.platform !== "win32",
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    running = child;
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout?.on("data", (c: Buffer) => stdout.push(c));
    child.stderr?.on("data", (c: Buffer) => stderr.push(c));
    let settled = false;
    let exited = false;
    const settle = (fn: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      running = undefined;
      fn();
    };
    let killedByTimeout = false;
    const timedOut = () => new Error(`テストが ${timeoutMs / 1000} 秒で終わらなかったので止めました`);
    const timer = setTimeout(() => {
      killedByTimeout = true;
      if (child.pid !== undefined) killTree(child.pid);
      if (exited) settle(() => reject(timedOut()));
      else child.once("exit", () => settle(() => reject(timedOut())));
    }, timeoutMs);
    child.on("error", (e) => settle(() => reject(e)));
    child.on("exit", (status, signal) => {
      exited = true;
      // タイムアウトで自分が止めた終了は中断ではない。POSIX では SIGKILL で `status` が null になり、
      // この listener がタイマー側の `once("exit")` より先に呼ばれるため、区別しないと中断と報告する。
      if (!killedByTimeout && isInterrupted(status)) {
        if (child.pid !== undefined) killTree(child.pid);
        settle(() => reject(new Error(`cargo が中断されました (status: ${status}, signal: ${signal})`)));
      }
    });
    child.on("close", (status) => {
      const output = `${Buffer.concat(stdout).toString("utf8")}\n${Buffer.concat(stderr).toString("utf8")}`;
      settle(() => resolve({ status, output }));
    });
  });
}

/** Windows で Ctrl+C により終了したプロセスの終了コード (STATUS_CONTROL_C_EXIT = 0xC000013A)。 */
const STATUS_CONTROL_C_EXIT = 0xc000013a;

/**
 * cargo が中断で終わったか。POSIX ではシグナルで死ぬと `status` が `null` になる。Windows の Ctrl+C は
 * シグナルではなく終了コード 0xC000013A で返るので、`null` だけを見ると中断を KILLED と数えてしまう。
 */
export function isInterrupted(status: number | null): boolean {
  return status === null || status === STATUS_CONTROL_C_EXIT;
}

/** リポジトリごとのロックファイルの場所。リポジトリの中 (書き換えない場所を含む) には置かない。 */
export function lockPathFor(repoRoot: string): string {
  const key = createHash("sha256").update(realpathSync.native(repoRoot).toLowerCase()).digest("hex");
  return join(tmpdir(), `claude-mutate-${key.slice(0, 16)}.lock`);
}

export function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

/**
 * ロックを取り、解放する関数を返す。同じリポジトリで別の mutate が走っていれば UsageError。
 *
 * 並行して走ると、後から始めた側が変異済みの内容を原本として読み、書き戻しの後に変異が残る
 * (`x;y;` に A が x→X、B が y→Y を当てると、B は `X;y;` を原本にする)。最初の読み込みの前に取り、
 * 全変異の書き戻しが終わるまで持つ。持ち主のプロセスが死んでいるロックは取り直す。
 *
 * ロックは、PID を書き終えた一時ファイルを `linkSync` で置いて作る。作ってから PID を書くと、
 * 空の瞬間に読んだ別の mutate が古いロックと誤り、生きているロックを奪う。
 */
export function acquireLock(lockPath: string): () => void {
  const pid = String(process.pid);
  const staged = `${lockPath}.${pid}.tmp`;
  writeFileSync(staged, pid);
  try {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        linkSync(staged, lockPath);
        return () => {
          if (readOwner(lockPath) === process.pid) rmSync(lockPath, { force: true });
        };
      } catch (e) {
        if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
        const owner = readOwner(lockPath);
        if (owner !== undefined && isAlive(owner)) {
          throw new UsageError(`別の mutate (pid ${owner}) が実行中です。終わってから実行してください: ${lockPath}`);
        }
        rmSync(lockPath, { force: true });
      }
    }
  } finally {
    rmSync(staged, { force: true });
  }
  throw new UsageError(`ロックを取れませんでした: ${lockPath}`);
}

function readOwner(lockPath: string): number | undefined {
  try {
    const owner = Number.parseInt(readFileSync(lockPath, "utf8"), 10);
    return Number.isInteger(owner) && owner > 0 ? owner : undefined;
  } catch {
    return undefined;
  }
}

async function main(): Promise<void> {
  let release: (() => void) | undefined;
  try {
    const args = parseArgs(process.argv.slice(2));
    const mutants = parseSpec(readFileSync(assertReadable(args.specFile), "utf8"));
    const targets = mutants.map((m) => ({ ...m, real: assertWritable(m.file) }));
    release = acquireLock(lockPathFor(REPO_ROOT));
    for (const t of targets) applyMutation(readFileSync(t.real, "utf8"), t.from, t.to);
    // ハンドラが無いと Node は SIGINT/SIGTERM で即終了し、`finally` の書き戻しが走らない。ハンドラは
    // テストのツリーを止めるだけにし、cargo の終了 (中断) を例外経由で `finally` へ運ばせる。
    // POSIX ではテストを別のプロセスグループで走らせるので、端末の Ctrl+C はここにしか届かない。
    const stopRunning = () => {
      if (running?.pid !== undefined) killTree(running.pid);
    };
    process.on("SIGINT", stopRunning);
    process.on("SIGTERM", stopRunning);
    let allKilled = true;
    for (const t of targets) {
      const result = await withMutation(t.real, t.from, t.to, () => runCommand(args.command, TIMEOUT_MS));
      const verdict = classify(result);
      if (verdict !== "KILLED") allKilled = false;
      process.stdout.write(`${t.name}\t${verdict}\t${t.file}\n`);
      const lines = summaryLines(result.output);
      // NO_RUN は要約の対象行が無いので、原因が分かるよう出力の末尾を出す
      const shown = verdict === "NO_RUN" ? result.output.trimEnd().split(/\r?\n/).slice(-5) : lines;
      for (const line of shown) process.stdout.write(`    ${line}\n`);
    }
    process.exitCode = allKilled ? 0 : 1;
  } catch (e) {
    process.stderr.write(`mutate: ${(e as Error).message}\n`);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  } finally {
    release?.();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  void main();
}
