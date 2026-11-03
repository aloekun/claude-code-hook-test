/**
 * `scripts/tools/` の調査用ツールが読んでよい場所を決める。
 *
 * ツールの実行コマンドは `.claude/settings.json` の許可リストにあり、確認なしで走る。どこでも読めると、
 * 作業ディレクトリ外を読むときの確認 (`blockReadsOutsideWorkingDirectories`) を素通りする経路になる
 * (pre-push の security レビューの指摘)。そこで読める場所を、調査に要る次の 3 か所の配下に限る:
 *
 * - このリポジトリ (ツール自身の位置から決める。cwd には依存しない)
 * - Claude の scratchpad (`<tmpdir>/claude/`)
 * - このリポジトリに対応する Claude Code の transcript 1 つ (`~/.claude/projects/<project-id>/`)
 *
 * transcript の root は `~/.claude/projects/` 全体 (このマシンの全プロジェクト) ではなく、
 * このリポジトリに対応する 1 フォルダだけに絞る。全体を許すと、prompt injection で agent を
 * 操られたときに無関係な他プロジェクトの transcript (secrets や顧客固有コードを含み得る) まで
 * 確認なしで読めてしまう (pre-push の security レビューの指摘、SEC-NEW-read-scope-L627)。
 * project-id の作り方・解決の仕方は `src/cli-merge-pipeline/src/feedback/project_dir.rs` の
 * `cwd_to_project_id` / `resolve_project_dir` と同じ事実 (絶対パスを小文字化して区切り文字等を
 * `-` に置換した文字列が project-id。実フォルダ名は元の cwd の大文字小文字をそのまま保存するため、
 * case-insensitive な FS でなければ直接 join では一致しないことがあり、列挙して小文字比較で
 * 探す必要がある) を指す (pre-push の simplicity レビューの指摘、SIM-NEW-read-scope-L31)。
 *
 * 判定は実パス (`realpathSync.native`) で行うので、`..` や symlink で外へ出る指定も拒否する。
 */

import { readdirSync, realpathSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = fileURLToPath(new URL("../..", import.meta.url));

/** `~/.claude/projects/<project-id>/` の `project-id` 部分。`cwd_to_project_id` と同じ置換規則。 */
function projectId(repoRoot: string): string {
  return repoRoot.replace(/[\\/]+$/, "").toLowerCase().replace(/[\\/:]/g, "-");
}

/**
 * `projectsRoot` 配下で `id` に対応する project-id フォルダを探す。
 *
 * まず直接 `join` した候補を試す (Windows は FS が case-insensitive なので大半はこれで解決する)。
 * それが実在しなければ `readdirSync` で実フォルダ名を列挙し、小文字比較で一致するものを探す —
 * 実フォルダ名は元の cwd の大文字小文字をそのまま保存しており、`id` (小文字化済み) と文字列として
 * 一致しないことがあるため (`project_dir.rs` の `resolve_project_dir` と同じ理由)。見つからなければ
 * 直接候補をそのまま返す (`realRoots()` が `realpathSync` の例外で候補から外す)。
 */
function resolveProjectDir(projectsRoot: string, id: string): string {
  const direct = join(projectsRoot, id);
  try {
    realpathSync.native(direct);
    return direct;
  } catch {
    // 列挙にフォールバックする。
  }
  let entries: string[];
  try {
    entries = readdirSync(projectsRoot);
  } catch {
    return direct;
  }
  const match = entries.find((name) => name.toLowerCase() === id);
  return match === undefined ? direct : join(projectsRoot, match);
}

/** 読んでよい場所の既定値。 */
export function defaultReadRoots(): string[] {
  const projectsRoot = join(homedir(), ".claude", "projects");
  return [REPO_ROOT, join(tmpdir(), "claude"), resolveProjectDir(projectsRoot, projectId(REPO_ROOT))];
}

/** 実在しない root は候補から外す (実パスを取れないため)。 */
function realRoots(roots: string[]): string[] {
  const found: string[] = [];
  for (const root of roots) {
    try {
      found.push(realpathSync.native(root));
    } catch {
      continue;
    }
  }
  return found;
}

function isWithin(file: string, root: string): boolean {
  const rel = relative(root, file);
  return rel === "" || (!rel.startsWith("..") && !/^[A-Za-z]:|^[/\\]/.test(rel));
}

/**
 * `file` の実パスがどれかの root の配下になければ例外を投げる。配下にあれば実パスを返す。
 *
 * 呼び出し元は判定に使った実パスをそのまま読む (check-then-open ではなく resolve-once-then-use)。
 * 判定後に別パスで `readFileSync` すると、判定から読み取りの間に symlink を差し替えられて
 * スコープ外ファイルを読める隙が生まれる (TOCTOU、pre-push の security レビューの指摘、
 * SEC-NEW-read-scope-L649)。
 */
export function assertReadable(file: string, roots: string[] = defaultReadRoots()): string {
  const real = realpathSync.native(file);
  const key = (p: string) => (process.platform === "win32" ? p.toLowerCase() : p);
  if (!realRoots(roots).some((root) => isWithin(key(real), key(root)))) {
    throw new Error(
      `読み取りを許可した場所の外です: ${file} (リポジトリ / scratchpad / transcript の配下だけ読めます)`,
    );
  }
  return real;
}
