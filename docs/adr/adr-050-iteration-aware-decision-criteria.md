# ADR-050: multi-iteration workflow の decision criteria scope 明示

## ステータス

試験運用 (2026-07-07)

## コンテキスト

takt の review-fix workflow は `Iteration N/max` の反復構造を持ち、各 step (analyze / fix / supervise 等) が
Report Directory に**複数 iteration 分の report** を蓄積する (最新は `{filename}`、過去は `{filename}.{timestamp}`)。

PR #250 (WP-06) で `supervise.md` の Decision Criteria が「all blocking findings が解決したら push」とだけ書かれ、
その「all blocking findings」が **全 iteration の累積か、現 iteration のみか** が曖昧だった。この scope 曖昧性は
「前 iteration で解決済みの finding を再判定して不要な reject」または「現 iteration の未解決を見落として過剰 approve」
という判定ミスに直結する。CodeRabbit Major 指摘となり、supervise.md に scope 宣言を追記して解消した。

この曖昧性は supervise 固有ではなく、**Report Directory を読む全ての multi-iteration step に共通の systemic pattern**
であり、今後の複数 iteration workflow 実装で再発しうる。

## 決定

multi-iteration workflow の各判定 step の decision criteria は、**評価 scope を明示**する。

### scope の 3 分類

| scope | 意味 | 典型例 |
|---|---|---|
| `current-iteration-only` | 現 iteration の report のみを判定対象とする (既定) | fix / supervise の per-iteration 判定 |
| `cumulative` | 全 iteration の累積状態を判定対象とする | convergence 全体の最終判定 |
| `sliding-window` | 直近 N iteration を対象とする | 膠着 (stall) 検出 |

### 原則

1. **既定は `current-iteration-only`** — 特に明示しない限り、判定は現 iteration の report (`{filename}` suffix なし、
   最新) のみを対象とする。過去 iteration の archived report (`{filename}.{timestamp}`) は参照しない
   ([dev-conventions.md](../dev-conventions.md) の Report Directory アクセスパターンと整合)。
2. **scope 宣言を instruction に明記** — decision criteria セクション冒頭で、どの scope で評価するかを 1-2 行の
   negative/positive specification で宣言する (例: supervise.md「本セクションは current iteration の report のみを
   判定対象とし、前 iteration の convergence_verdict とは比較しない」)。
3. **cumulative / sliding-window を使う場合は理由を明記** — 既定から外れる scope を採る step は、なぜ全履歴 / 窓が
   必要かを instruction に書く (膠着検出は反復の history が本質的に必要、等)。

## 帰結

### 利点

- multi-iteration step の判定 scope が暗黙にならず、scope 曖昧性由来の判定ミスを設計段階で排除。
- 新規 iteration workflow / facet 追加時の設計チェック項目として再利用可能。

### 留意点

- takt は scope を機械強制しない (instruction prompt レベルの規約)。宣言忘れの検出は reviewer / dogfood に依存する。

### 関連 ADR

- [ADR-020](adr-020-takt-facets-sharing.md) — facet 共有 (fix/supervise は複数 workflow で共有、scope 宣言も共有される)
- [ADR-030](adr-030-deterministic-post-merge-feedback.md) — 決定論的 post-merge feedback (iteration 構造の文脈)
- [ADR-037](adr-037-takt-fix-trust-shortcut.md) — fix-trust shortcut (convergence_verdict による iteration 短絡)
- [ADR-047](adr-047-prepush-refute-facet.md) — refute facet (loop_monitor の cycle = sliding-window scope の例)
