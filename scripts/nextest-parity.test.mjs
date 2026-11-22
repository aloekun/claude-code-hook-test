// cargo-nextest の版とテストのコマンドが、CI・クラウド・手元 (push-runner) で揃っていることを見る (順位 615)。
//
// 版は 3 箇所に書いてあり、揃える約束は .config/nextest.toml のコメント (ADR-051 の論理結合) にしかない。
// 1 箇所でも漏れると、どれかの環境が別の版で走り、緑の結果を信用できなくなる。テストのコマンドも
// push-runner-config.toml と ci.yml に別々に書いてある (ADR-081 の同一事実の分散)。
//
// 版やコマンドが取り出せないときは「一致」と見なさず落とす。書式が変わって何も取れないと、
// 比べる対象が無いまま通ってしまうため。

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// js-yaml 5.x は CommonJS のみで default export を持たない (scripts/lint-workflows.mjs と同じ取り方)。
const yaml = createRequire(import.meta.url)('js-yaml');

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (relative) => readFileSync(join(REPO_ROOT, relative), 'utf8');

const CI_YML = '.github/workflows/ci.yml';
const CLOUD_SETUP = 'scripts/cloud-setup.sh';
const NEXTEST_TOML = '.config/nextest.toml';
const PUSH_RUNNER_CONFIG = 'push-runner-config.toml';

/** `text` の中の `pattern` (g フラグ付き、版は 1 番目の group) の一致をすべて返す。1 件も無ければ投げる。 */
function allMatches(text, pattern, where) {
  const found = [...text.matchAll(pattern)].map((m) => m[1]);
  if (found.length === 0) {
    throw new Error(`${where}: ${pattern} に一致する行が無い (書式が変わったら抽出を直す)`);
  }
  return found;
}

/** 版を書いた箇所ごとに、取り出した版を返す。同じファイルに複数あれば、すべてを別々に返す。 */
function versionsByPlace(files) {
  return [
    ...allMatches(files[CI_YML], /tool:\s*cargo-nextest@(\S+)/g, CI_YML).map((v) => [`${CI_YML} (install-action)`, v]),
    ...allMatches(files[CLOUD_SETUP], /NEXTEST_VERSION="\$\{[A-Z_]+:-([^}"]+)\}"/g, CLOUD_SETUP).map((v) => [
      `${CLOUD_SETUP} (NEXTEST_VERSION)`,
      v,
    ]),
    ...allMatches(files[NEXTEST_TOML], /^# 版は (\S+?)。/gm, NEXTEST_TOML).map((v) => [`${NEXTEST_TOML} (版は)`, v]),
    ...allMatches(files[NEXTEST_TOML], /cargo install cargo-nextest --locked --version (\S+?)`/g, NEXTEST_TOML).map(
      (v) => [`${NEXTEST_TOML} (手元の install)`, v],
    ),
  ];
}

const NEXTEST_RUN = 'cargo nextest run';
const collapseSpaces = (command) => command.replace(/\s+/g, ' ').trim();

/**
 * push-runner の quality gate に書いた `cargo nextest run` のコマンド (並びは書いた順)。
 *
 * TOML の配列の 1 要素 = 1 行の文字列だけを読む。コメントでない行に `cargo nextest run` が
 * 読めた数より多く現れたら (複数行の文字列・行末コメント付きなど、読めない書き方)、落とす。
 */
function pushRunnerNextestCommands(configText) {
  const commands = allMatches(configText, /^\s*"(cargo nextest run[^"]*)",?\s*$/gm, PUSH_RUNNER_CONFIG);
  const mentions = configText.split('\n').filter((line) => !line.trim().startsWith('#') && line.includes(NEXTEST_RUN));
  if (mentions.length !== commands.length) {
    throw new Error(
      `${PUSH_RUNNER_CONFIG}: \`${NEXTEST_RUN}\` が ${mentions.length} 行にあるが、読めたのは ${commands.length} 件 (読めない書き方を足したら抽出を直す)`,
    );
  }
  return commands.map(collapseSpaces);
}

/** workflow の全 job・全 step の `run` の値 (YAML として読んだもの)。 */
function runValues(ciText) {
  const workflow = yaml.load(ciText);
  return Object.values(workflow?.jobs ?? {}).flatMap((job) =>
    (job?.steps ?? []).map((step) => step?.run).filter((run) => typeof run === 'string'),
  );
}

/**
 * CI に書いた `cargo nextest run` のコマンドから、CI 専用の `--profile ci` を外したもの (並びは書いた順)。
 *
 * `run` の値を YAML として読むので、複数行の plain scalar の続きの行も引数に入る (YAML は改行を
 * 空白へ畳む。PR #659 の CodeRabbit の指摘: 1 行目だけを読むと、続きの行に足したフラグを見落とす)。
 * `run: |` のブロックは 1 行 = 1 コマンドとして分け、行末の `\` の続きはつなげる。
 */
function ciNextestCommands(ciText) {
  const commands = runValues(ciText)
    .flatMap((run) => run.replace(/\\\n/g, ' ').split('\n'))
    .filter((line) => line.includes(NEXTEST_RUN))
    .map((line) => collapseSpaces(line.slice(line.indexOf(NEXTEST_RUN))));
  if (commands.length === 0) {
    throw new Error(`${CI_YML}: \`${NEXTEST_RUN}\` を走らせる step が無い (書式が変わったら抽出を直す)`);
  }
  return commands.map((command) => {
    if (!/ --profile ci(?= |$)/.test(command)) {
      throw new Error(`${CI_YML}: \`${command}\` に --profile ci が無い (CI は ci profile で走らせる)`);
    }
    return command.replace(/ --profile ci(?= |$)/, '');
  });
}

const realFiles = () => Object.fromEntries([CI_YML, CLOUD_SETUP, NEXTEST_TOML, PUSH_RUNNER_CONFIG].map((f) => [f, read(f)]));

/** 全箇所が 1 つの版に揃っていれば空。揃っていなければ、箇所ごとの版を並べた行。 */
function versionMismatch(files) {
  const places = versionsByPlace(files);
  const distinct = new Set(places.map(([, v]) => v));
  return distinct.size === 1 ? [] : places.map(([place, v]) => `${place}: ${v}`);
}

describe('cargo-nextest の版', () => {
  it('CI・クラウド・手元の全箇所で同じ版を書いている', () => {
    expect(versionMismatch(realFiles())).toEqual([]);
  });

  it('全 4 箇所から版を取り出せる (どれかが抽出から漏れたら落とす)', () => {
    expect(versionsByPlace(realFiles())).toHaveLength(4);
  });

  it.each([
    [CI_YML, /cargo-nextest@[0-9.]+/, 'cargo-nextest@0.0.1'],
    [CLOUD_SETUP, /(NEXTEST_VERSION:-)[0-9.]+/, '$10.0.1'],
    [NEXTEST_TOML, /(# 版は )[0-9.]+/, '$10.0.1'],
    [NEXTEST_TOML, /(--locked --version )[0-9.]+/, '$10.0.1'],
  ])('%s の版を 1 箇所だけ変えると不一致になる (%s)', (file, from, to) => {
    const files = realFiles();
    const changed = files[file].replace(from, to);
    expect(changed).not.toBe(files[file]);
    expect(versionMismatch({ ...files, [file]: changed })).not.toEqual([]);
  });

  it.each([
    [CI_YML, /tool:\s*cargo-nextest@\S+/, 'tool: cargo-nextest'],
    [CLOUD_SETUP, /readonly NEXTEST_VERSION=.*/, 'readonly NEXTEST_VERSION=latest'],
    [NEXTEST_TOML, /^# 版は .*$/m, '# 版は CI に合わせる'],
  ])('%s から版を取り出せなければ、一致と見なさず落とす', (file, from, to) => {
    const files = realFiles();
    const changed = files[file].replace(from, to);
    expect(changed).not.toBe(files[file]);
    expect(() => versionMismatch({ ...files, [file]: changed })).toThrow(/一致する行が無い/);
  });
});

describe('cargo nextest run のコマンド', () => {
  it('push-runner と CI で、--profile ci を除いて同じコマンドを同じ順に走らせる', () => {
    const files = realFiles();
    expect(ciNextestCommands(files[CI_YML])).toEqual(pushRunnerNextestCommands(files[PUSH_RUNNER_CONFIG]));
  });

  it('通常のテストと ignored のテストの 2 つを走らせている (片側の抽出漏れで空どうしが一致しない)', () => {
    expect(pushRunnerNextestCommands(realFiles()[PUSH_RUNNER_CONFIG])).toEqual([
      'cargo nextest run --workspace',
      'cargo nextest run --workspace --run-ignored only',
    ]);
  });

  it('片側だけフラグを足すと食い違う', () => {
    const files = realFiles();
    const changed = files[PUSH_RUNNER_CONFIG].replace(
      '"cargo nextest run --workspace",',
      '"cargo nextest run --workspace --no-fail-fast",',
    );
    expect(changed).not.toBe(files[PUSH_RUNNER_CONFIG]);
    expect(ciNextestCommands(files[CI_YML])).not.toEqual(pushRunnerNextestCommands(changed));
  });

  it('CI のコマンドから --profile ci が消えたら落とす', () => {
    const changed = realFiles()[CI_YML].replace(
      'run: cargo nextest run --workspace --profile ci --run-ignored only',
      'run: cargo nextest run --workspace --run-ignored only',
    );
    expect(changed).not.toBe(realFiles()[CI_YML]);
    expect(() => ciNextestCommands(changed)).toThrow(/--profile ci が無い/);
  });

  /** CI の通常のテストの `run:` 行を、`build(indent)` が返す YAML に置き換える。 */
  const replaceCiRunLine = (build) => {
    const original = realFiles()[CI_YML];
    const changed = original.replace(
      /^( *)run: cargo nextest run --workspace --profile ci$/m,
      (_line, indent) => build(indent),
    );
    expect(changed).not.toBe(original);
    return changed;
  };

  it('CI の run の続きの行に足したフラグも比べる (PR #659 の CodeRabbit の指摘)', () => {
    const changed = replaceCiRunLine(
      (indent) => `${indent}run: cargo nextest run --workspace --profile ci\n${indent}  --no-fail-fast`,
    );
    expect(ciNextestCommands(changed)).toContain('cargo nextest run --workspace --no-fail-fast');
    expect(ciNextestCommands(changed)).not.toEqual(pushRunnerNextestCommands(realFiles()[PUSH_RUNNER_CONFIG]));
  });

  it('CI の run: | のブロックと、行末の \\ の続きも読む', () => {
    const changed = replaceCiRunLine(
      (indent) => `${indent}run: |\n${indent}  set -e\n${indent}  cargo nextest run --workspace \\\n${indent}    --profile ci`,
    );
    expect(ciNextestCommands(changed)).toEqual(pushRunnerNextestCommands(realFiles()[PUSH_RUNNER_CONFIG]));
  });

  it('push-runner 側に読めない書き方の cargo nextest run があれば落とす', () => {
    const original = realFiles()[PUSH_RUNNER_CONFIG];
    const changed = original.replace('"cargo nextest run --workspace",', '"cargo nextest run --workspace", # 通常のテスト');
    expect(changed).not.toBe(original);
    expect(() => pushRunnerNextestCommands(changed)).toThrow(/読めたのは/);
  });
});
