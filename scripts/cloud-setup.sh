#!/usr/bin/env bash
#
# cloud-setup.sh — 使い捨てクラウドセッション用のハーネス有効化スクリプト (WP-15)
#
# 役割: claude.ai/code のような使い捨て Linux 環境で、19 crate をビルドせずに
# hooks / CLI を即時有効化する。release-binaries.yml が公開した rolling release
# (タグ `nightly`) から Linux バイナリを取得し、.claude/ へ配置して settings を生成する。
#
# 使い方 (ADR-060: 2 つのフェーズに分離。旧・注意 (B) の「session フェーズへ登録」は
#   Web UI に対応する設定が存在しないことが判明したため、SessionStart hook 経由に確定):
#
#   --session-phase … .claude/settings.json の SessionStart hook から毎セッション実行される
#                     (scripts/cloud-hook-dispatch.mjs --setup が起動)。バイナリ配置 / jj /
#                     pnpm install。生成物 (.claude/ バイナリ / .jj / node_modules) は git
#                     追跡外で fresh clone のたびに消えるため、毎セッション再構築する。
#                     冪等なので resume 時の再実行も安全。
#   --cache-phase   … Web UI のセットアップスクリプト欄に登録する (環境キャッシュ構築時に
#                     1 回だけ走る)。snapshot に載って意味があるものだけを暖める:
#                     pnpm store / cargo clippy warmup (要 CARGO_TARGET_DIR=リポ外、
#                     例 /opt/cargo-target を Web UI の環境変数欄で設定)。
#   引数なし        … 旧来の全ステップ実行 (後方互換 + ローカル Linux 検証用)。
#                     クラウドの実運用では上記 2 フェーズを使う。
#
# 設計メモ:
# - **認証を要求しない**: public リポジトリの Release asset は素の HTTPS で取得できる。
#   gh CLI の認証を setup に持ち込むとトークンを環境変数で渡す構成が必要になり、
#   クラウドの許可リスト型ネットワーク設定と合わせて失敗点が増える。curl だけで完結させる。
# - **hooks が無言で無効化される経路を fail-closed にする**: バイナリ欠落や settings 生成
#   失敗をスキップして「setup 成功」と報告すると、セッションはハーネス無しで進み、
#   その事実に誰も気付かない (ADR-005 が対処した事故と同じ形)。必須要素は即 exit 1 で落とす。
# - **必須バイナリの一覧は settings.local.json.template から導出する**: どの exe が
#   無いと hooks が発火しないかの正解はテンプレート自身が持っている。ここに一覧を
#   コピーすると hook 追加時に片方だけ更新されて無言で穴が開く。
# - **冪等**: 再実行しても壊れない。既存の jj / node_modules は再利用する。
# - **Ollama 依存機能は非対象**: クラウドに Ollama は無い。lint_screen 等は default OFF かつ
#   fail-open のため、導入せず「skip される」ことを最後に明示するだけに留める。

set -euo pipefail

# ─── 設定 (環境変数で上書き可能) ───

# 取得元。fork や検証用ビルドを指す場合に差し替える。
readonly REPO_SLUG="${CLOUD_SETUP_REPO_SLUG:-aloekun/claude-code-hook-test}"
readonly RELEASE_TAG="${CLOUD_SETUP_RELEASE_TAG:-nightly}"
readonly TARGET_TRIPLE="x86_64-unknown-linux-gnu"

# jj は ADR-011 / ADR-015 / ADR-045 が 0.42 系の挙動 (`jj git push -b` の自動 track 等) に
# 依存しているため、ローカル検証環境と同じバージョンを固定する。
readonly JJ_VERSION="${CLOUD_SETUP_JJ_VERSION:-0.42.0}"
readonly JJ_TARGET_TRIPLE="x86_64-unknown-linux-musl"

# jj/CLI bin の設置先。既定を PATH に確実に載る /usr/local/bin にする (クラウドは root 実行)。
# これで後続の jj / hooks / push-runner が jj を解決でき、旧ラッパーの PATH 確保 (C-1) が本体に入る。
# 非 root 環境では CLOUD_SETUP_BIN_DIR=~/.local/bin 等に上書きする。
readonly INSTALL_BIN_DIR="${CLOUD_SETUP_BIN_DIR:-/usr/local/bin}"

# jj colocated 初期化時に track する既定ブランチ (A-3)。CLOUD_SETUP_DEFAULT_BRANCH で上書き可。
readonly DEFAULT_BRANCH="${CLOUD_SETUP_DEFAULT_BRANCH:-master}"

# pnpm は latest ではなく固定メジャーを入れる (CodeRabbit #318)。pnpm-lock.yaml は
# lockfileVersion 9.0 で `--frozen-lockfile` は pnpm 9+ 前提。`npm install -g pnpm` (latest) だと
# 将来の pnpm が frozen-install / lockfile 互換を壊すリスクがある。ローカル dev の pnpm 11 に
# 合わせる (v9.0 は pnpm 9-11 が互換)。CLOUD_SETUP_PNPM_SPEC で上書き可。
readonly PNPM_SPEC="${CLOUD_SETUP_PNPM_SPEC:-pnpm@11}"

# ─── ログ ───

log()  { printf '[cloud-setup] %s\n' "$*"; }
warn() { printf '[cloud-setup] Warning: %s\n' "$*" >&2; }
die()  { printf '[cloud-setup] Error: %s\n' "$*" >&2; exit 1; }

# ─── リポジトリルートの解決 ───
#
# cwd に依存させない (setup script がどこから呼ばれるか保証が無いため)。
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
readonly CLAUDE_DIR="${REPO_ROOT}/.claude"
readonly SETTINGS_TEMPLATE="${CLAUDE_DIR}/settings.local.json.template"

cd "${REPO_ROOT}"

# ─── 前提コマンドの確認 ───

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "必須コマンドが見つかりません: $1"
}

require_command curl
require_command tar
require_command node

[ -f "${SETTINGS_TEMPLATE}" ] || die "settings テンプレートがありません: ${SETTINGS_TEMPLATE}"

log "リポジトリルート: ${REPO_ROOT}"

# ─── 1. Linux バイナリの取得と配置 ───

install_harness_binaries() {
  local archive="claude-code-hooks-${TARGET_TRIPLE}.tar.gz"
  local base_url="https://github.com/${REPO_SLUG}/releases/download/${RELEASE_TAG}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  # 途中で失敗しても一時ディレクトリを残さない。RETURN trap は関数返却後もシェルに残留し、
  # 後続関数の return で消滅済み local tmp_dir を参照して set -u で落ちるため
  # (--session-phase 再実行が exit 1 になる実測バグ)、発火時に自己解除する。
  # EXIT にも掛けるのは die() (exit 1) 経路では RETURN trap が発火しないため (PR #319 CR)。
  trap 'rm -rf "${tmp_dir}"; trap - RETURN EXIT' RETURN EXIT

  log "バイナリを取得中: ${base_url}/${archive}"
  # --fail: HTTP 404/5xx を silent な空ファイルではなく exit 非 0 にする
  # (これが無いと「取得成功・中身が HTML のエラーページ」で tar が謎の失敗をする)。
  curl --fail --location --silent --show-error \
    --output "${tmp_dir}/${archive}" \
    "${base_url}/${archive}" \
    || die "バイナリの取得に失敗しました。release-binaries.yml が tag '${RELEASE_TAG}' を公開済みか確認してください。"

  # checksum は「あれば検証する」: release 側の生成に失敗していても setup 全体を
  # 止めるほどではないが、取得できたのに不一致なら転送破損なので落とす。
  if curl --fail --location --silent --show-error \
       --output "${tmp_dir}/${archive}.sha256" \
       "${base_url}/${archive}.sha256" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
      ( cd "${tmp_dir}" && sha256sum --check --status "${archive}.sha256" ) \
        || die "checksum 不一致: ダウンロードが破損しています"
      log "checksum 検証: OK"
    else
      warn "sha256sum が無いため checksum 検証を skip します"
    fi
  else
    warn "checksum ファイルを取得できませんでした (検証を skip)"
  fi

  mkdir -p "${CLAUDE_DIR}"
  tar -xzf "${tmp_dir}/${archive}" -C "${CLAUDE_DIR}"

  # tar が実行ビットを保持しない環境でも hooks が起動できるようにする。
  # BUILD_INFO はバイナリではないので対象外。
  find "${CLAUDE_DIR}" -maxdepth 1 -type f ! -name '*.*' ! -name 'BUILD_INFO' \
    -exec chmod +x {} +

  if [ -f "${CLAUDE_DIR}/BUILD_INFO" ]; then
    log "配置したビルドの provenance:"
    sed 's/^/  /' "${CLAUDE_DIR}/BUILD_INFO"
  fi
}

# settings テンプレートが参照する hook exe が実際に配置されたかを検証する。
# 1 つでも欠けると該当 hook が無言で発火しないため fail-closed で落とす。
verify_required_binaries() {
  local missing
  missing="$(
    node --input-type=module -e '
      import { readFileSync } from "node:fs";
      import { existsSync } from "node:fs";

      // `node -e` はスクリプトファイルを argv に含めない (argv = [node, templatePath,
      // claudeDir])。したがって渡した引数は argv[1] から始まり slice(1) で取り出す。
      // slice(2) だと templatePath に claudeDir が入り claudeDir が undefined になり、
      // readFileSync がディレクトリを EISDIR で失敗 → verify が die して install_jj /
      // install_node_dependencies に到達しない (WP-15 cloud-setup 環境半整備バグ)。
      const [templatePath, claudeDir] = process.argv.slice(1);
      const template = readFileSync(templatePath, "utf8");

      // テンプレートの command は "{{PROJECT_DIR}}/.claude/<name>{{EXE_SUFFIX}}" 形式。
      const names = new Set(
        [...template.matchAll(/\.claude\/([A-Za-z0-9._-]+)\{\{EXE_SUFFIX\}\}/g)].map((m) => m[1]),
      );
      if (names.size === 0) {
        console.error("no hook executables referenced by the template");
        process.exit(2);
      }
      for (const name of [...names].sort()) {
        if (!existsSync(`${claudeDir}/${name}`)) console.log(name);
      }
    ' "${SETTINGS_TEMPLATE}" "${CLAUDE_DIR}"
  )" || die "必須バイナリの検証スクリプトが失敗しました"

  if [ -n "${missing}" ]; then
    printf '%s\n' "${missing}" | sed 's/^/  - /' >&2
    die "settings テンプレートが参照する hook バイナリが不足しています (hooks が無言で無効化されます)"
  fi
  log "必須 hook バイナリ: すべて配置済み"
}

# ─── 2. settings.local.json の生成 ───
#
# ADR-005: CLAUDE_PROJECT_DIR が空になる環境があるため、絶対パスを埋め込んだ
# settings.local.json をビルド時に生成する。生成失敗 = hooks 全停止なので fail-closed。
generate_settings() {
  node "${REPO_ROOT}/scripts/build-hooks-settings.mjs" \
    || die "settings.local.json の生成に失敗しました (hooks が有効化されません)"
}

# ─── 3. jj の導入 ───
#
# 既に PATH にあり、かつ目的のバージョンなら何もしない (再実行時の無駄な取得を避ける)。
install_jj() {
  if command -v jj >/dev/null 2>&1 && jj --version 2>/dev/null | grep -q "${JJ_VERSION}"; then
    log "jj ${JJ_VERSION}: 導入済み (skip)"
    return 0
  fi

  local archive="jj-v${JJ_VERSION}-${JJ_TARGET_TRIPLE}.tar.gz"
  local url="https://github.com/jj-vcs/jj/releases/download/v${JJ_VERSION}/${archive}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  # 自己解除 + EXIT 併用の理由は install_harness_binaries の同 trap のコメント参照。
  trap 'rm -rf "${tmp_dir}"; trap - RETURN EXIT' RETURN EXIT

  log "jj ${JJ_VERSION} を取得中"
  if ! curl --fail --location --silent --show-error --output "${tmp_dir}/${archive}" "${url}"; then
    warn "jj の取得に失敗しました。jj 依存の pipeline (pnpm push 等) は使えません。"
    return 0
  fi

  tar -xzf "${tmp_dir}/${archive}" -C "${tmp_dir}"

  # アーカイブのレイアウト (flat / サブディレクトリ入り) に依存しないよう探索して拾う。
  local jj_bin
  jj_bin="$(find "${tmp_dir}" -type f -name jj -print -quit)"
  if [ -z "${jj_bin}" ]; then
    warn "アーカイブ内に jj バイナリが見つかりませんでした"
    return 0
  fi

  if ! mkdir -p "${INSTALL_BIN_DIR}" 2>/dev/null; then
    warn "${INSTALL_BIN_DIR} を作成できませんでした (権限不足の可能性)。jj は未配置のままです。"
    return 0
  fi
  if ! install -m 0755 "${jj_bin}" "${INSTALL_BIN_DIR}/jj" 2>/dev/null; then
    warn "${INSTALL_BIN_DIR}/jj への配置に失敗しました (権限不足の可能性)。jj は未配置のままです。"
    return 0
  fi
  log "jj を配置: ${INSTALL_BIN_DIR}/jj"

  case ":${PATH}:" in
    *":${INSTALL_BIN_DIR}:"*) ;;
    *) warn "${INSTALL_BIN_DIR} が PATH にありません。PATH へ追加してください。" ;;
  esac
}

# ─── 4. Node 依存 (takt を含む) の導入 ───
#
# ADR-017: takt はバージョン完全固定 (package.json に "takt": "0.35.3")。
# --frozen-lockfile で lockfile と一致しない解決を拒否し、固定を機械的に担保する。
install_node_dependencies() {
  if ! command -v pnpm >/dev/null 2>&1; then
    warn "pnpm が無いため takt / node 依存を導入できません (pnpm push 等は使えません)"
    return 0
  fi
  log "pnpm install --frozen-lockfile を実行中 (takt は ADR-017 で固定)"
  pnpm install --frozen-lockfile \
    || warn "pnpm install に失敗しました。takt 依存の pipeline は使えません。"
}

# ─── 5. 環境差分の明示 ───
#
# 「動かない」ではなく「意図して skip される」ことを可視化する。
report_optional_features() {
  if command -v ollama >/dev/null 2>&1; then
    log "Ollama: 検出 (lint_screen / findings classification が利用可能)"
  else
    log "Ollama: 未導入 — lint_screen / findings classification は skip されます (fail-open、想定内)"
  fi
}

# ─── C-1. pnpm の確保 ───
#
# base image に node はあっても pnpm が無いことがある。push / merge pipeline は pnpm 前提。
# 旧: ラッパー側の `npm install -g pnpm`。本体に取り込み「1 スクリプト」化する。
ensure_pnpm() {
  if command -v pnpm >/dev/null 2>&1; then
    log "pnpm: 導入済み (skip)"
    return 0
  fi
  if ! command -v npm >/dev/null 2>&1; then
    warn "npm が無いため pnpm を導入できません (pnpm push / merge-pr は使えません)"
    return 0
  fi
  log "pnpm を導入中 (npm install -g ${PNPM_SPEC})"
  npm install -g "${PNPM_SPEC}" || warn "pnpm の導入に失敗しました (pnpm push / merge-pr は使えません)"
}

# ─── A. jj リポジトリの初期化 (colocated) + identity + bookmark track ───
#
# install_jj は jj バイナリを置くだけでリポジトリを jj 化しない。素の git のままだと
# file-length Stop gate (hooks-post-tool-comment-lint-rust --check-modified-files) が
# "There is no jj repo" で fail-closed し毎回 Stop をブロックする。jj-op-verify PostToolUse /
# PreToolUse の jj プリセット / push-runner / merge-pipeline / todo_staleness も前提を欠く。
init_jj_repo() {
  if ! command -v jj >/dev/null 2>&1; then
    warn "jj が使えないため jj 初期化を skip (file-length gate / jj hooks / push は使えません)"
    return 0
  fi

  # A-3 準備: colocate init 後は git HEAD が detached になり `git branch --show-current` が
  #      空になるため、現ブランチ名は init より前に取得しておく (CodeRabbit #318)。
  local current_branch
  current_branch="$(git -C "${REPO_ROOT}" branch --show-current 2>/dev/null || true)"

  # A-2. identity を git global (launcher hook 由来) から継承。未設定だと commit author が
  #      空になり push 不可。init 前に設定すれば init が作る working-copy commit も正しい author になる。
  if [ -z "$(jj config get user.name 2>/dev/null || true)" ]; then
    local git_name
    git_name="$(git config --global user.name 2>/dev/null || true)"
    if [ -n "${git_name}" ]; then
      if jj config set --user user.name "${git_name}"; then
        log "jj user.name = ${git_name}"
      else
        warn "jj user.name の設定に失敗 (push で commit author が空になる可能性)"
      fi
    else
      warn "git global user.name 未設定 → jj commit author が空になり push できません"
    fi
  fi
  if [ -z "$(jj config get user.email 2>/dev/null || true)" ]; then
    local git_email
    git_email="$(git config --global user.email 2>/dev/null || true)"
    if [ -n "${git_email}" ]; then
      if jj config set --user user.email "${git_email}"; then
        log "jj user.email = ${git_email}"
      else
        warn "jj user.email の設定に失敗 (push で commit author が空になる可能性)"
      fi
    else
      warn "git global user.email 未設定 → jj commit author が空になり push できません"
    fi
  fi

  # A-1. colocated 初期化 (冪等: .jj があれば skip)。.claude/ バイナリ・settings・target・
  #      node_modules はすべて .gitignore 済みのため working-copy commit には入らない。
  if [ -d "${REPO_ROOT}/.jj" ]; then
    log "jj リポジトリ: 既に初期化済み (skip)"
  else
    log "jj リポジトリを colocated 初期化中 (jj git init --colocate)"
    if ! ( cd "${REPO_ROOT}" && jj git init --colocate ); then
      warn "jj git init に失敗 (file-length gate / push は使えません)"
      return 0
    fi
  fi

  # A-4 (ADR-060 リハーサル実測): colocate init は git HEAD を detach し、環境によっては
  #      ローカルブランチ ref も残らない。jj 中心のローカル flow は影響しないが、クラウド
  #      セッションの Claude は git で commit/push するため、そのままだと push が
  #      "src refspec does not match any" で失敗する。init 前に取得した現ブランチへ
  #      HEAD を戻す (直後の working tree は不変なので checkout は安全)。ref が消えて
  #      いる場合は現 HEAD 位置で作り直す。
  if ! git -C "${REPO_ROOT}" symbolic-ref -q HEAD >/dev/null; then
    if [ -n "${current_branch}" ]; then
      if ( cd "${REPO_ROOT}" && { git checkout -q "${current_branch}" 2>/dev/null \
          || git checkout -q -B "${current_branch}" 2>/dev/null; } ); then
        log "git HEAD を ${current_branch} へ再アタッチ"
      else
        warn "git HEAD の再アタッチに失敗 (detached のまま。git push 前に checkout -B が必要)"
      fi
    else
      # 本 run の開始時点で既に detached だった場合は戻し先が分からない。無言 skip に
      # すると push 失敗まで露見しないため、ここで明示する。
      warn "git HEAD が detached でブランチ名も不明のため再アタッチできません (git checkout -B <branch> で復旧)"
    fi
  fi

  # A-3. push ワークフロー (ADR-011/015) 用に既定ブランチ + 現ブランチ (init 前に取得済み) の
  #      remote bookmark を track。remote bookmark 未 import / 不在なら best-effort で無視。
  ( cd "${REPO_ROOT}" && jj bookmark track "${DEFAULT_BRANCH}@origin" >/dev/null 2>&1 ) \
    && log "bookmark track: ${DEFAULT_BRANCH}@origin" || true
  if [ -n "${current_branch}" ] && [ "${current_branch}" != "${DEFAULT_BRANCH}" ]; then
    ( cd "${REPO_ROOT}" && jj bookmark track "${current_branch}@origin" >/dev/null 2>&1 ) \
      && log "bookmark track: ${current_branch}@origin" || true
  fi
}

# ─── C-2. cargo キャッシュのプリウォーム ───
#
# 初回 Stop の lint:rust (cargo clippy --workspace) は cold cache で step_timeout を超過し
# 実 lint を隠したまま偽 failure を出す。ここで一度流して target/ を暖める。
# `-D warnings` は付けない: warmup の目的はキャッシュ暖機なので lint 結果で失敗させない
# (将来コードが warning を入れても setup を止めないため)。現時点で clippy 債務は無く、それは
# release-binaries.yml の CI clippy ゲート (-D warnings) が land 前に担保する (CodeRabbit #318)。
# 効果は本 setup が session-start フェーズで走り target/ が当該セッションに残る場合 (ヘッダ B) に限る。
# 既定 ON。時間超過が問題なら CLOUD_SETUP_SKIP_CARGO_WARMUP=1 で無効化。上限は CLOUD_SETUP_CARGO_WARMUP_TIMEOUT。
warmup_cargo() {
  if [ "${CLOUD_SETUP_SKIP_CARGO_WARMUP:-0}" = "1" ]; then
    log "cargo warmup: skip (CLOUD_SETUP_SKIP_CARGO_WARMUP=1)"
    return 0
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    log "cargo: 未検出 — lint:rust ウォームアップを skip (初回 Stop は cold compile)"
    return 0
  fi
  local timeout_secs="${CLOUD_SETUP_CARGO_WARMUP_TIMEOUT:-240}"
  log "cargo clippy --workspace をウォームアップ中 (最大 ${timeout_secs}s、初回 Stop の cold compile 回避)"
  if command -v timeout >/dev/null 2>&1; then
    ( cd "${REPO_ROOT}" && timeout "${timeout_secs}" cargo clippy --workspace --all-targets --all-features ) \
      || warn "cargo warmup 未完了 (timeout/失敗)。初回 Stop の lint:rust は cold compile になります。"
  else
    ( cd "${REPO_ROOT}" && cargo clippy --workspace --all-targets --all-features ) \
      || warn "cargo warmup 未完了。初回 Stop の lint:rust は cold compile になります。"
  fi
}

# ─── フェーズ実行 (ADR-060) ───

# SessionStart hook (cloud-hook-dispatch.mjs --setup) から毎セッション実行される。
# generate_settings は呼ばない: クラウドの hook 登録は tracked な .claude/settings.json +
# dispatcher が担う (ADR-060 § 決定 2)。settings.local.json を併産すると、起動時 snapshot
# 仕様により当該セッションには効かないまま二重登録リスクだけが残る。
# warmup_cargo も呼ばない: SessionStart の latency 予算に収まらないため cache-phase +
# CARGO_TARGET_DIR (リポ外) の組に委ねる。
run_session_phase() {
  install_harness_binaries
  verify_required_binaries
  ensure_pnpm
  install_jj             # セッション内は非 attach リポの release も取得可 (ADR-060 実測)
  init_jj_repo           # A-1/A-2/A-3: colocated 初期化 + identity + bookmark track
  install_node_dependencies
  report_optional_features
  log "セットアップ完了 (--session-phase)"
}

# Web UI のセットアップスクリプト欄から環境キャッシュ構築時に 1 回だけ実行される。
# fresh clone で消えるリポ内生成物 (バイナリ / settings / .jj) をここで作っても無意味なので
# 作らない。snapshot に載って次セッションを速くするものだけを暖める。
run_cache_phase() {
  ensure_pnpm
  install_node_dependencies   # pnpm store が snapshot に載り session-phase の install が高速化
  warmup_cargo                # 要 CARGO_TARGET_DIR=リポ外。リポ内 target/ は clone で消える
  report_optional_features
  log "セットアップ完了 (--cache-phase)"
}

# 旧来の全ステップ (後方互換 + ローカル Linux 検証用)。
run_legacy_full() {
  install_harness_binaries
  verify_required_binaries
  generate_settings
  ensure_pnpm            # C-1: pnpm 確保 (本体に取り込み)
  install_jj
  init_jj_repo           # A-1/A-2/A-3: colocated 初期化 + identity + bookmark track
  install_node_dependencies
  warmup_cargo           # C-2: lint:rust の cold compile 回避 (best-effort)
  report_optional_features
  log "セットアップ完了"
}

main() {
  case "${1:-}" in
    --session-phase) run_session_phase ;;
    --cache-phase)   run_cache_phase ;;
    "")              run_legacy_full ;;
    *)               die "不明な引数: $1 (--session-phase / --cache-phase / 引数なし)" ;;
  esac
}

main "$@"
