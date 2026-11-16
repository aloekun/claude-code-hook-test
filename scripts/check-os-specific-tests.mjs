// 片方の OS でしかコンパイルされないテスト (`#[cfg(unix)]` / `#[cfg(windows)]` など) が、
// その OS の CI で実際に一覧に載っていることを確かめる (順位 423)。
//
// 動機: `#![cfg(unix)]` のテストファイルは、Windows ではテストバイナリが走っても中身が 0 件になる。
// 「0 件実行」と「全件成功」は CI の出力でほとんど見分けがつかないので、cfg で落とした範囲が
// 意図せず広がっても (cfg の書き間違い、テストの移動) 気づけない。ADR-064 の「陽性証拠を要求する」
// と同じ考え方で、OS 専用のテストが**その OS で**一覧に載っていることを証拠として求める。
//
// 方法: CI の rust job が両 OS で `cargo test --workspace --lib --bins --tests -- --list --format terse`
// の出力を artifact に残し、後続の job が本スクリプトで突き合わせる。ソースの `cfg` は読まない —
// コンパイラが出した一覧を比べるので、cfg の書き方の揺れで見落とすことが無い。
//
//   - 片方の OS の一覧にしか無いテストは、`config/os-specific-tests.json` に載っていなければならない
//   - manifest に載ったテストは、指定した OS の一覧に載り、もう片方の一覧には載っていてはならない
//
// 一覧に載ることは「コンパイルされて実行対象になった」ことを示す。`#[ignore]` のテストも一覧に
// 載る (実行は rust job の `--ignored` step)。doctest は対象外 (`--doc` を付けていない)。
//
// 使い方: node scripts/check-os-specific-tests.mjs <Linux の一覧> <Windows の一覧> [manifest]
//
// 判定は export した関数が持ち、`check-os-specific-tests.test.mjs` が固定する。ファイルの読み取りと
// 結果の出力は main だけが行う。

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEFAULT_MANIFEST = join(dirname(fileURLToPath(import.meta.url)), "..", "config", "os-specific-tests.json");

const RUNNING = /^\s*Running (?:unittests )?(\S+) \((.+)\)\s*$/;
const TEST_LINE = /^(.+): test\s*$/;

/**
 * テストバイナリの表示名。`target\debug\deps\lib_subprocess-6f1a2b.exe` と
 * `target/debug/deps/lib_subprocess-6f1a2b` を同じ `lib_subprocess` にする (区切り文字・拡張子・hash の差を消す)。
 */
export function binaryStem(binaryPath) {
  const base = binaryPath.split(/[\\/]/).pop() ?? binaryPath;
  return base.replace(/\.exe$/i, "").replace(/-[0-9a-f]+$/, "");
}

/**
 * `cargo test -- --list --format terse` の出力 (stdout と stderr をまとめたもの) から、
 * `<バイナリ> (<ソース>) <テスト名>` の集合を作る。同じテスト名が別のクレートにあっても区別できる。
 * テストの行がどのバイナリにも属さない、またはテストが 1 件も無い出力は、壊れているとみなして例外にする
 * (一覧が空のまま照合すると、すべてが「片方にしか無い」か「どちらにも無い」になり、判定が成り立たない)。
 */
export function parseTestList(text) {
  const tests = new Set();
  let current;
  let binaries = 0;
  for (const raw of text.split(/\r?\n/)) {
    const running = RUNNING.exec(raw);
    if (running) {
      current = `${binaryStem(running[2])} (${running[1].replaceAll("\\", "/")})`;
      binaries += 1;
      continue;
    }
    const test = TEST_LINE.exec(raw);
    if (!test) continue;
    if (current === undefined) {
      throw new Error(`テストの行がどのバイナリにも属していない: ${raw}`);
    }
    tests.add(`${current} ${test[1]}`);
  }
  if (binaries === 0 || tests.size === 0) {
    throw new Error(`テストの一覧が空 (バイナリ ${binaries} 件、テスト ${tests.size} 件)。cargo test --list の出力か確かめる`);
  }
  return tests;
}

/** manifest の JSON を検査して、OS ごとのテスト名の集合にする。形が違えば例外にする。 */
export function parseManifest(text) {
  const data = JSON.parse(text);
  const result = {};
  for (const os of ["linux", "windows"]) {
    const entries = data[os];
    if (!Array.isArray(entries)) throw new Error(`manifest に "${os}" の配列が無い`);
    result[os] = new Set();
    for (const entry of entries) {
      if (typeof entry?.test !== "string" || typeof entry?.reason !== "string" || entry.reason.trim() === "") {
        throw new Error(`manifest の "${os}" の要素は { "test": 文字列, "reason": 空でない文字列 } にする: ${JSON.stringify(entry)}`);
      }
      if (result[os].has(entry.test)) throw new Error(`manifest の "${os}" に重複がある: ${entry.test}`);
      result[os].add(entry.test);
    }
  }
  return result;
}

const LABEL = { linux: "Linux", windows: "Windows" };

/** 両 OS の一覧と manifest を突き合わせ、問題を文の配列で返す。空なら合格。 */
export function compareTestLists(lists, manifest) {
  const problems = [];
  for (const [os, other] of [
    ["linux", "windows"],
    ["windows", "linux"],
  ]) {
    for (const test of [...lists[os]].sort()) {
      if (!lists[other].has(test) && !manifest[os].has(test)) {
        problems.push(
          `${LABEL[os]} でしか一覧に載らないテストが manifest の "${os}" に無い: ${test}` +
            ` (意図した OS 専用なら理由を添えて載せる。意図していなければ cfg を見直す)`,
        );
      }
    }
    for (const test of [...manifest[os]].sort()) {
      if (!lists[os].has(test)) {
        problems.push(`manifest の "${os}" にあるテストが ${LABEL[os]} の一覧に無い (実行された証拠が無い): ${test}`);
      } else if (lists[other].has(test)) {
        problems.push(`manifest の "${os}" にあるテストが ${LABEL[other]} の一覧にも載っている (OS 専用ではない): ${test}`);
      }
    }
  }
  return problems;
}

function main(argv) {
  const [linuxPath, windowsPath, manifestPath = DEFAULT_MANIFEST] = argv;
  if (!linuxPath || !windowsPath) {
    process.stderr.write("使い方: node scripts/check-os-specific-tests.mjs <Linux の一覧> <Windows の一覧> [manifest]\n");
    return 2;
  }
  const lists = {
    linux: parseTestList(readFileSync(linuxPath, "utf8")),
    windows: parseTestList(readFileSync(windowsPath, "utf8")),
  };
  const manifest = parseManifest(readFileSync(manifestPath, "utf8"));
  const problems = compareTestLists(lists, manifest);
  process.stdout.write(
    `[os-specific-tests] Linux ${lists.linux.size} 件 / Windows ${lists.windows.size} 件、` +
      `manifest: Linux 専用 ${manifest.linux.size} 件 / Windows 専用 ${manifest.windows.size} 件\n`,
  );
  if (problems.length > 0) {
    process.stderr.write(`[os-specific-tests] ${problems.length} 件の問題:\n${problems.map((p) => `  - ${p}`).join("\n")}\n`);
    return 1;
  }
  process.stdout.write("[os-specific-tests] OK\n");
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    process.stderr.write(`[os-specific-tests] ${e.message}\n`);
    process.exitCode = 1;
  }
}
