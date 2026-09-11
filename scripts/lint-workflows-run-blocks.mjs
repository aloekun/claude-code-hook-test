// GitHub Actions の `run:` ブロックに対する契約検査 3 (順位 515 / 撤1-①) の純粋層。
//
// GitHub Actions は `run:` を `bash -e {0}` で起動する。スクリプト内で `set -uo pipefail` と
// 書いても **`-e` は外れない** (外すには `set +e` が要る)。この前提を知らずに書くと、
// 正常系のはずの分岐で step ごと落ちる (2026-08-20 PR #428、順位 319: マーカー未投稿という
// 初回は必ず通る状態で `grep` の一致 0 件が step を落とし、backstop の投稿そのものが消えた)。
// pre-push review / CodeRabbit / YAML parse はいずれも通過しており、実 run の red で初めて
// 判明した。旧 dev-conventions.md の同名 convention を決定論層へ移したもので、規約の中身は
// 本ファイルが正となる (dev-conventions.md 側は § 機構への索引 の 1 行だけを持つ、ADR-042)。
//
// 検査は 2 つ。どちらも **`\` 継続行を結合した論理行** 単位で見る (多行パイプラインや
// `if ! grep ... \ || ! grep ...` の条件文脈を物理行で見ると誤判定する)。
//
// (a) `set -` の短フラグに `e` が無い。`-e` を外したつもりの記述は、読む側に「失敗が
//     許容されている」と誤読させるだけで実際には外れていない。`set -euo pipefail` と書き、
//     失敗を許す箇所だけ `|| true` で個別に手当てする。
// (b) `grep` がパイプラインの構成要素になっている。一致 0 件の `grep` は exit 1 を返す。
//     末尾なら `-e` が、途中でも `pipefail` がその失敗を拾い、コマンド置換の代入ごと step が
//     終わる。「まだ無い」ことを調べる検索は一致 0 件が正常系なので必ずここに当たる。
//     `awk '/pattern/'` は一致 0 件でも exit 0 を返すので置き換えられる。
//     **先頭・末尾・中間のどの位置でも拾う** (PR #491 CodeRabbit 指摘: `grep x f | sort` は
//     `pipefail` 下で落ちるが、`| grep` だけを見る初版は素通りさせていた)。
//     条件文脈 (`if` / `elif` / `while` / `until` で始まる論理行) と `|| true` / `|| :` 付きは
//     `-e` が発火しないので除外する。
//
// **パイプ判定はクォートを見る。** `grep -E '^a|^b' file` の `|` は正規表現の一部で
// パイプ演算子ではない。素朴に `|` で分割すると nightly-todo.yml の実在 2 行が即座に
// 誤検知になる (2026-09-10 に実データで確認)。
//
// I/O を持たないのは vitest で fixture 検査を固定するため (`lint-workflows-run-blocks.test.mjs`)。

/**
 * 物理行を `\` 継続で結合した論理行の配列にする。`line` は先頭物理行の 1-origin 番号。
 * @param {string} text
 * @returns {{ line: number, text: string }[]}
 */
export function joinContinuationLines(text) {
  const logical = [];
  let current = null;
  text.split(/\r?\n/).forEach((physical, index) => {
    if (current === null) {
      current = { line: index + 1, text: physical };
    } else {
      current.text += ` ${physical.trim()}`;
    }
    if (/\\$/.test(physical)) {
      current.text = current.text.replace(/\\$/, '');
      return;
    }
    logical.push(current);
    current = null;
  });
  if (current !== null) {
    logical.push(current);
  }
  return logical;
}

/**
 * 論理行をパイプ演算子で分割する。クォート内と `||` (論理 OR) の `|` は演算子ではない。
 *
 * 戻り値の要素数が 1 なら「パイプラインではない」。`grep -E 'a|b' f` を誤って
 * パイプラインと見なさないため、この関数を通してから位置を判定する。
 * @param {string} text
 * @returns {string[]}
 */
export function splitPipelineSegments(text) {
  const segments = [];
  let current = '';
  let quote = null;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (quote === "'") {
      current += ch;
      if (ch === "'") quote = null;
    } else if (quote === '"') {
      if (ch === '\\' && i + 1 < text.length) {
        current += ch + text[i + 1];
        i += 1;
      } else {
        current += ch;
        if (ch === '"') quote = null;
      }
    } else if (ch === '\\' && i + 1 < text.length) {
      current += ch + text[i + 1];
      i += 1;
    } else if (ch === "'" || ch === '"') {
      quote = ch;
      current += ch;
    } else if (ch === '|' && text[i + 1] === '|') {
      current += '||';
      i += 1;
    } else if (ch === '|') {
      segments.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  segments.push(current);
  return segments;
}

/**
 * 論理行を文 (statement) に分割する。区切りは top-level の `;` と `&&`。
 *
 * 免除判定 (`if` の条件文脈 / `|| true`) を**論理行全体**に当てると粒度が粗すぎる:
 * `if cond; then risky | grep x; fi` の 1 行形では then 節の無保護な `grep` が、
 * `rm -f x || true; cat f | grep y` では 2 文目が、それぞれ免除に巻き込まれてすり抜ける。
 * 文単位で判定すればどちらも拾える (2026-09-10 pre-push simplicity review の指摘)。
 *
 * `||` では分割しない — `|| true` は直前のコマンドに係る免除なので、同じ文に留める必要が
 * あるため。`$(...)` と `(...)` の中も分割しない (部分式は 1 つのコマンドとして扱う)。
 * @param {string} text
 * @returns {string[]}
 */
export function splitStatements(text) {
  const statements = [];
  let current = '';
  let quote = null;
  let depth = 0;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (quote === "'") {
      current += ch;
      if (ch === "'") quote = null;
    } else if (quote === '"') {
      if (ch === '\\' && i + 1 < text.length) {
        current += ch + text[i + 1];
        i += 1;
      } else {
        current += ch;
        if (ch === '"') quote = null;
      }
    } else if (ch === '\\' && i + 1 < text.length) {
      current += ch + text[i + 1];
      i += 1;
    } else if (ch === "'" || ch === '"') {
      quote = ch;
      current += ch;
    } else if (ch === '(') {
      depth += 1;
      current += ch;
    } else if (ch === ')') {
      depth = Math.max(0, depth - 1);
      current += ch;
    } else if (depth === 0 && ch === ';') {
      statements.push(current);
      current = '';
    } else if (depth === 0 && ch === '&' && text[i + 1] === '&') {
      statements.push(current);
      current = '';
      i += 1;
    } else {
      current += ch;
    }
  }
  statements.push(current);
  return statements;
}

const CONDITION_CONTEXT = /^(?:if|elif|while|until)\b/;
const FAILURE_TOLERATED = /\|\|\s*(?:true|:)(?=[\s;)]|$)/;
const SET_COMMAND = /^set\s+(.*)$/;

/** パイプライン 1 区間の実行コマンドが `grep` か。`VAR=$(` や `!` の前置きを飛ばす。 */
const GREP_SEGMENT = /^\s*(?:!\s*)?(?:[A-Za-z_][A-Za-z0-9_]*=)?(?:\$\(\s*)?(?:!\s*)?grep\b/;

/**
 * `set -uo pipefail` 形式: 短フラグ群に `e` が含まれないなら true。
 * `set +e` / `set -o pipefail` (短フラグ無し) は対象外 (前者は明示的に外す意図が読める、
 * 後者は `-e` を外したつもりの記述ではない)。
 * @param {string} rest `set ` の後ろ
 */
function isSetWithoutErrexit(rest) {
  const tokens = rest.split(/\s+/).filter(Boolean);
  const shortFlagGroups = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token === '-o' || token === '+o') {
      i += 1;
      continue;
    }
    if (/^-[A-Za-z]+$/.test(token)) {
      shortFlagGroups.push(token.slice(1));
    } else {
      break;
    }
  }
  if (shortFlagGroups.length === 0) {
    return false;
  }
  return !shortFlagGroups.some((group) => group.includes('e'));
}

/**
 * 論理行のどこかに「免除されない `grep` パイプライン」があるか。
 *
 * 免除は**文単位**で見る: `if` 等の条件文脈にある文と、`|| true` を持つ文は `-e` が
 * 発火しないので対象外。同じ行の他の文はそれとは独立に判定する。
 * @param {string} logicalLine 継続行を結合済みの 1 論理行 (trim 済み)
 */
function hasUnprotectedGrepPipeline(logicalLine) {
  for (const statement of splitStatements(logicalLine)) {
    const s = statement.trim();
    if (s === '' || CONDITION_CONTEXT.test(s) || FAILURE_TOLERATED.test(s)) {
      continue;
    }
    const segments = splitPipelineSegments(s);
    if (segments.length > 1 && segments.some((segment) => GREP_SEGMENT.test(segment))) {
      return true;
    }
  }
  return false;
}

/**
 * 1 つの `run:` ブロックを検査し、違反の配列を返す (空配列 = OK)。
 * @param {string} runText `run:` の本文
 * @returns {{ line: number, message: string }[]}
 */
export function checkRunBlock(runText) {
  const findings = [];
  for (const { line, text } of joinContinuationLines(runText)) {
    const trimmed = text.trim();
    if (trimmed === '' || trimmed.startsWith('#')) {
      continue;
    }
    const set = trimmed.match(SET_COMMAND);
    if (set && isSetWithoutErrexit(set[1])) {
      findings.push({
        line,
        message:
          `\`${trimmed}\` は -e を外しません (GitHub Actions は run: を bash -e で起動する)。` +
          '`set -euo pipefail` と書き、失敗を許す箇所だけ `|| true` で個別に手当てしてください',
      });
      continue;
    }
    if (hasUnprotectedGrepPipeline(trimmed)) {
      findings.push({
        line,
        message:
          '`grep` がパイプラインに置かれています。一致 0 件で exit 1 になり、末尾なら -e が、' +
          '途中でも pipefail がその失敗を拾って step が終わります。一致 0 件が正常系なら ' +
          "`awk '/pattern/'` に置き換えるか、`if cmd | grep -q ...; then` の条件文脈に寄せてください",
      });
    }
  }
  return findings;
}

/** GitHub が `bash -e {0}` で起動する shell 指定。`bash --noprofile ...` 等の引数付きも含む。 */
const BASH_SHELL = /^bash(?:\s|$)/;
/** bash 以外と分かっている shell。`run:` の内容が bash 前提でないので検査しない。 */
const KNOWN_NON_BASH_SHELL = /^(?:pwsh|powershell|cmd|python|sh|node|ruby|perl)(?:\s|$)/;
const WINDOWS_LABEL = /windows/i;
const NON_WINDOWS_LABEL = /^(?:ubuntu|macos|self-hosted-linux)/i;
const GITHUB_EXPRESSION = /\$\{\{/;

/**
 * `runs-on` から OS を静的に判定する。
 *
 * - `posix`: 全ラベルが ubuntu / macOS 系。shell 未指定なら GitHub 既定は `bash -e {0}`。
 * - `windows`: windows イメージを含む。shell 未指定の既定は **pwsh** で bash ではない。
 * - `dynamic`: `${{ matrix.os }}` 等の式。実行時まで OS が決まらない。
 * - `unknown`: self-hosted の任意ラベル等。
 * @param {unknown} runsOn
 * @returns {'posix'|'windows'|'dynamic'|'unknown'}
 */
export function classifyRunnerOs(runsOn) {
  const labels = [];
  if (typeof runsOn === 'string') {
    labels.push(runsOn);
  } else if (Array.isArray(runsOn)) {
    labels.push(...runsOn.filter((l) => typeof l === 'string'));
  } else if (runsOn && typeof runsOn === 'object') {
    const nested = /** @type {any} */ (runsOn).labels;
    if (typeof nested === 'string') labels.push(nested);
    else if (Array.isArray(nested)) labels.push(...nested.filter((l) => typeof l === 'string'));
  }
  if (labels.length === 0) return 'unknown';
  if (labels.some((l) => GITHUB_EXPRESSION.test(l))) return 'dynamic';
  if (labels.some((l) => WINDOWS_LABEL.test(l))) return 'windows';
  if (labels.every((l) => NON_WINDOWS_LABEL.test(l))) return 'posix';
  return 'unknown';
}

/**
 * step / job / workflow の `defaults.run.shell` と `runs-on` から実効 shell を判定する。
 * @param {string|undefined} declared 解決済みの shell 宣言 (step → job → workflow の順で先勝ち)
 * @param {'posix'|'windows'|'dynamic'|'unknown'} runnerOs
 * @returns {'bash'|'other'|'ambiguous'}
 */
export function resolveShellKind(declared, runnerOs) {
  if (typeof declared === 'string' && declared.trim() !== '') {
    const shell = declared.trim();
    if (BASH_SHELL.test(shell)) return 'bash';
    if (KNOWN_NON_BASH_SHELL.test(shell)) return 'other';
    return 'other';
  }
  // 宣言なし。GitHub 既定は POSIX ランナーなら bash -e、Windows なら pwsh。
  // OS が静的に決まらない (matrix / self-hosted) 場合は bash 前提の検査を当てられない。
  return runnerOs === 'posix' ? 'bash' : 'ambiguous';
}

/**
 * parse 済み workflow document から `run:` step を分類して集める。
 *
 * - `bash`: 契約検査 3 を当てる step。
 * - `ambiguous`: shell 未宣言で実効 shell が静的に決まらない step。Windows では pwsh に
 *   なるため bash 前提のスクリプトなら実行時に壊れる (ADR-065 の cross-OS 退行そのもの)。
 *   検査を当てず、`shell:` の明示を求める finding として返す。
 * - bash 以外と分かっている step は捨てる。
 * @param {any} document js-yaml で parse した workflow
 * @returns {{ bash: {job:string,step:string,run:string}[], ambiguous: {job:string,step:string,runnerOs:string}[] }}
 */
export function collectRunSteps(document) {
  const workflowShell = document?.defaults?.run?.shell;
  const bash = [];
  const ambiguous = [];
  for (const [jobName, job] of Object.entries(document?.jobs ?? {})) {
    const jobShell = job?.defaults?.run?.shell ?? workflowShell;
    const runnerOs = classifyRunnerOs(job?.['runs-on']);
    (Array.isArray(job?.steps) ? job.steps : []).forEach((step, index) => {
      if (typeof step?.run !== 'string') {
        return;
      }
      const label = step.id ?? step.name ?? `#${index + 1}`;
      const kind = resolveShellKind(step.shell ?? jobShell, runnerOs);
      if (kind === 'bash') {
        bash.push({ job: jobName, step: label, run: step.run });
      } else if (kind === 'ambiguous') {
        ambiguous.push({ job: jobName, step: label, runnerOs });
      }
    });
  }
  return { bash, ambiguous };
}
