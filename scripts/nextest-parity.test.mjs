// cargo-nextest の版とテストのコマンドが、CI・クラウド・手元 (push-runner) で揃っていることを見る (順位 615)。
//
// 版は 3 箇所に書いてあり、揃える約束は .config/nextest.toml のコメント (ADR-051 の論理結合) にしかない。
// 1 箇所でも漏れると、どれかの環境が別の版で走り、緑の結果を信用できなくなる。テストのコマンドも
// push-runner-config.toml と ci.yml に別々に書いてある (ADR-081 の同一事実の分散)。
//
// 版やコマンドが取り出せないときは「一致」と見なさず落とす。書式が変わって何も取れないと、
// 比べる対象が無いまま通ってしまうため。

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

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

/** push-runner の quality gate に書いた `cargo nextest run` のコマンド (並びは書いた順)。 */
function pushRunnerNextestCommands(configText) {
  return allMatches(configText, /^\s*"(cargo nextest run[^"]*)",?\s*$/gm, PUSH_RUNNER_CONFIG);
}

/** CI に書いた `cargo nextest run` のコマンドから、CI 専用の `--profile ci` を外したもの (並びは書いた順)。 */
function ciNextestCommands(ciText) {
  return allMatches(ciText, /^\s*run:\s*(cargo nextest run.*?)\s*$/gm, CI_YML).map((command) => {
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
});
