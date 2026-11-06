// Claude Code の permission rule に `Write(path)` 指定子が書かれていたら失敗する (順位 389)。
//
// 動機: `Write(path)` 指定子はファイル権限チェックにマッチせず no-op である。ファイル編集の
// scope は `Edit(path)` が Write を含む全編集ツールをまとめて表す (CLI 2.1.218 で実測、CLI 自身も
// 「Use Edit(path) instead」と警告する。ADR-072 決定 12)。allow 側に書くと飾りで済むが、
// **deny 側に書くと「拒否しているつもりで何も拒否していない」silent fail-open になる**。
// 書いた人にも読む人にも効いているように見えるため、症状が出ないまま潜伏する。
//
// warning では足りない。deny が黙って無効になる種類の失敗は、気づいた時には既に書き込まれて
// いる。よって error にして push gate と CI の両方を止める (ADR-043 の fail-closed)。
//
// 検査対象 (permission rule と同じ構文が現れる場所):
//   1. `.claude/settings*.json` の `permissions.allow` / `deny` / `ask`
//   2. `.github/workflows/*.yml` の `--allowedTools` / `--disallowedTools` の引数
//      (claude-code-action の `claude_args` と、`run:` から直接 CLI を呼ぶ形の両方)
//
// `Edit(path)` / `Read(path)` と、括弧の無い `Write` (ツール全体の許可・拒否) は対象外。
// 後者はツール名の照合なので有効に働く。
//
// 検査の判定は export した関数が持ち、`lint-permission-rules.test.mjs` が fixture で固定する。
// ファイルの読み取りと結果の出力は main だけが行う。

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export const SETTINGS_DIR = '.claude';
export const WORKFLOW_DIR = '.github/workflows';

const PERMISSION_LISTS = ['allow', 'deny', 'ask'];

// `NotebookWrite(` のような別ツール名の一部には一致させない。
const WRITE_SPECIFIER = /\bWrite\(/;
const TOOLS_FLAG = /--(?:allowed|disallowed)-?tools\b/i;

const REMEDY =
  '`Write(path)` 指定子は no-op で、deny に書くと拒否が黙って無効になります。' +
  'ファイル編集の scope は `Edit(path)` で表してください (Write を含む全編集ツールに効く、ADR-072 決定 12)';

/**
 * settings.json の本文を検査し、違反メッセージの配列を返す (空配列 = OK)。
 * @param {string} text
 * @returns {string[]}
 */
export function checkSettingsText(text) {
  let settings;
  try {
    settings = JSON.parse(text);
  } catch (e) {
    // 読めない設定を「違反なし」と扱うと検査が素通りになる (fail-closed)。
    return [`JSON として読めません (${e.message})。permission rule を検査できないため失敗扱いにします`];
  }
  const permissions = settings?.permissions;
  if (permissions === undefined || permissions === null) {
    return [];
  }
  const findings = [];
  for (const list of PERMISSION_LISTS) {
    const rules = permissions[list];
    if (rules === undefined) {
      continue;
    }
    if (!Array.isArray(rules)) {
      findings.push(`permissions.${list} が配列ではありません。permission rule を検査できないため失敗扱いにします`);
      continue;
    }
    for (const rule of rules) {
      if (typeof rule === 'string' && WRITE_SPECIFIER.test(rule)) {
        findings.push(`permissions.${list} の "${rule}": ${REMEDY}`);
      }
    }
  }
  return findings;
}

/**
 * workflow の本文から、tools 指定フラグの引数に `Write(` が現れる行を探す。
 *
 * フラグの位置から行末までを引数とみなす (`--allowedTools "A" "B"` の可変長引数も同じ行に
 * 並ぶため)。引用符が行内で閉じない場合は、閉じる行まで引数が続くとみなす。shell の行継続
 * (`\` で終わる行) も同様に次の行へ続ける。
 *
 * shell のコメントは対象外 (行頭の `#` も、空白の後ろに続く末尾の `#` も)。引用符の中の `#` は
 * 引数の一部として扱う。コメント内のアポストロフィ (`# don't`) を引用符の開始と取り違えない
 * よう、引用符の状態を 1 文字ずつ追って読む (PR #622 CodeRabbit 指摘)。
 * @param {string} text
 * @returns {{ line: number, message: string }[]}
 */
export function checkWorkflowText(text) {
  const lines = text.split(/\r?\n/);
  const findings = [];
  for (let i = 0; i < lines.length; i++) {
    const code = scanShell(lines, i, 0, { stopAtLineEnd: true }).text;
    const flag = TOOLS_FLAG.exec(code);
    if (!flag) {
      continue;
    }
    const { text: argument, end } = scanShell(lines, i, flag.index + flag[0].length, { stopAtLineEnd: false });
    if (WRITE_SPECIFIER.test(argument)) {
      findings.push({ line: i + 1, message: `${flag[0]} の引数に Write(...) があります。${REMEDY}` });
    }
    i = end;
  }
  return findings;
}

/**
 * `lines[start]` の `col` 文字目から shell の字句として読み、コメントを除いた本文を返す。
 *
 * `stopAtLineEnd` が偽なら、行末で引用符が開いているか行継続 (`\`) のとき次の行へ続ける。
 * 返す `end` は最後に読んだ行の index。
 * @param {string[]} lines
 * @param {number} start
 * @param {number} col
 * @param {{ stopAtLineEnd: boolean }} options
 * @returns {{ text: string, end: number }}
 */
function scanShell(lines, start, col, { stopAtLineEnd }) {
  let quote = null;
  let text = '';
  for (let index = start; index < lines.length; index++) {
    const line = lines[index];
    let continued = false;
    for (let c = index === start ? col : 0; c < line.length; c++) {
      const ch = line[c];
      if (quote === "'") {
        if (ch === "'") {
          quote = null;
        }
        text += ch;
        continue;
      }
      if (ch === '\\') {
        if (c === line.length - 1 && quote === null) {
          continued = true;
          break;
        }
        text += line.slice(c, c + 2);
        c++;
        continue;
      }
      if (quote === '"') {
        if (ch === '"') {
          quote = null;
        }
        text += ch;
        continue;
      }
      if (ch === '"' || ch === "'") {
        quote = ch;
      } else if (ch === '#' && (c === 0 || /\s/.test(line[c - 1]))) {
        break;
      }
      text += ch;
    }
    if (stopAtLineEnd || (quote === null && !continued) || index + 1 === lines.length) {
      return { text, end: index };
    }
    text += '\n';
  }
  return { text, end: start };
}

/**
 * settings と workflow の全ファイルを検査する。
 * @param {{ settingsDir: string, workflowDir: string }} dirs
 * @returns {{ findings: { file: string, message: string }[], checked: number }}
 */
export function lintPermissionRules({ settingsDir, workflowDir }) {
  const findings = [];

  const settingsFiles = readdirSync(settingsDir)
    .filter((f) => /^settings.*\.json$/.test(f))
    .sort();
  // 0 件で成功を返すと、ディレクトリの指定を誤ったときに検査が消えたことに気づけない。
  if (!settingsFiles.includes('settings.json')) {
    findings.push({ file: settingsDir, message: 'settings.json がありません (ディレクトリの指定を確認)' });
  }
  for (const name of settingsFiles) {
    const path = join(settingsDir, name);
    for (const message of checkSettingsText(readFileSync(path, 'utf8'))) {
      findings.push({ file: path, message });
    }
  }

  const workflowFiles = readdirSync(workflowDir)
    .filter((f) => /\.ya?ml$/.test(f))
    .sort();
  if (workflowFiles.length === 0) {
    findings.push({ file: workflowDir, message: 'workflow ファイルが 1 つもありません (ディレクトリの指定を確認)' });
  }
  for (const name of workflowFiles) {
    const path = join(workflowDir, name);
    for (const { line, message } of checkWorkflowText(readFileSync(path, 'utf8'))) {
      findings.push({ file: `${path}:${line}`, message });
    }
  }

  return { findings, checked: settingsFiles.length + workflowFiles.length };
}

function main() {
  const { findings, checked } = lintPermissionRules({ settingsDir: SETTINGS_DIR, workflowDir: WORKFLOW_DIR });
  for (const { file, message } of findings) {
    console.error(`[lint-permission-rules] ${file}: ${message}`);
  }
  if (findings.length > 0) {
    console.error(`[lint-permission-rules] ${findings.length} 件の検査に失敗しました`);
    process.exit(1);
  }
  console.log(`[lint-permission-rules] OK (${checked} ファイルに Write(path) 指定子が無いことを確認)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
