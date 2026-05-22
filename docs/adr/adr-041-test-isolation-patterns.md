# ADR-041: Test Isolation Patterns for Multi-Condition Guards

## ステータス

試験運用 (2026-05-22)

> 本 ADR は `~/.claude/rules/common/code-review.md` checklist (順位 84 land 済) を **project-level rationale + 具体実装例 + history** で補完する layer。global checklist は「複合 AND (OR) の early-return guard を持つ関数のテストは、各条件を独立 variant で検証している (1 guard だけ壊したとき 1 テストだけ落ちる構造)」と要件を 1 行で示すが、本 ADR は PR #120 W-001 の vacuous assertion 失敗事例と、PR #168 で sentinel pattern により構造的に解決した実装例を記録する。

## コンテキスト

`src/cli-pr-monitor/src/stages/poll.rs` の `enrich_with_classifier(state, config)` は OR-guard による早期 return を持つ:

```rust
fn enrich_with_classifier(state: &mut PrMonitorState, config: &ClassifierConfig) {
    if !config.enabled || state.findings.is_empty() {
        return;
    }
    // ... mutate state.classified_findings ...
}
```

PR #120 W-001 で初発見された問題: 当初のテスト `enrich_with_classifier_skips_when_disabled` は `enabled = false` + `findings` 空 (= `ClassifierConfig::default()` + `PrMonitorState::new()` の素直な組み合わせ) で `assert!(state.classified_findings.is_empty())` を assert していた。これは **責務混在の vacuous assertion**:

- 2 つの OR guard が同時発火しても test は pass
- 検証対象 field `classified_findings` を空のまま渡しているため、「早期 return 由来で空のまま」か「mutation 経路を通過したが結果として空」か判別不能
- 片方の guard を消した mutation テスト (`!config.enabled` だけ削除) を入れても、`findings.is_empty()` が代替発火して test は依然 pass = guard が壊れても落ちない

同型の責務混在は今後も 2+ 条件の OR/AND 早期 return を持つ pure function 系 test で再発しうるため、PR #168 で実装した sentinel pattern + 直交 precondition setup を project ADR として codify する。

## 決定

複合 AND/OR の early-return guard を持つ関数のテストは、以下 2 原則を満たす形で実装する。

### 原則 1: Sentinel pattern (検証対象 field の pre-populate)

検証対象 field を test setup 段階で `sentinel` 値で pre-populate し、「mutation 経路を通過した場合は sentinel が上書きされて消える」構造にする。assertion は `assert_eq!(field, vec![sentinel])` (生存確認) で行う。

空のまま渡す anti-pattern では、「不変=空」が早期 return 由来か他経路由来か判別不能。sentinel pattern により「早期 return が発火した → mutation が起きなかった → sentinel が survive した」と一意に推論可能になる。

### 原則 2: OR-guard precondition assertion (直交 setup)

`if !A || B { return; }` 形式の OR guard を test する場合、各 variant で **片方の guard だけが発火する直交 precondition** を組む:

- Variant 1 (左 arm): `A` を偽にする (= `!A` 発火) + `B` を偽にする (= `B` 不発)
- Variant 2 (右 arm): `A` を真にする (= `!A` 不発) + `B` を真にする (= `B` 発火)

片方が不発になる条件を test 内で **明示的に assert** または **コメント** で記述すると、将来 guard 順序が変わったり追加 guard が増えたりした場合に「test の前提が崩れた」ことを早期検出できる。

### 実装例 (poll.rs の `enrich_with_classifier` 2 variants)

`enrich_with_classifier_skips_when_disabled` (左 arm = `!enabled` 単独発火):

```rust
#[test]
fn enrich_with_classifier_skips_when_disabled() {
    let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
    // 右 arm 不発条件: findings 非空 (= findings.is_empty() guard が偽)
    state.findings = vec![Finding { /* ... */ }];
    // sentinel pattern: mutation 経路を通過すれば上書きされて消える
    let sentinel = ClassifiedFinding { /* ... */ };
    state.classified_findings = vec![sentinel.clone()];
    // 左 arm 発火条件: enabled = false
    let disabled = ClassifierConfig { enabled: false, ..ClassifierConfig::default() };

    enrich_with_classifier(&mut state, &disabled);

    assert_eq!(
        state.classified_findings,
        vec![sentinel],
        "!config.enabled guard should early return before any mutation"
    );
}
```

`enrich_with_classifier_skips_when_findings_empty` (右 arm = `findings.is_empty()` 単独発火):

```rust
#[test]
fn enrich_with_classifier_skips_when_findings_empty() {
    let mut state = PrMonitorState::new(Some(1), Some("o/r".into()), "t".into());
    // 左 arm 不発条件: 明示 assert で precondition を文書化 + 機械検証
    assert!(
        state.findings.is_empty(),
        "test precondition: findings must be empty so `!enabled` guard stays unfired"
    );
    // sentinel pattern (同上)
    let sentinel = ClassifiedFinding { /* ... */ };
    state.classified_findings = vec![sentinel.clone()];
    // 右 arm 発火条件: enabled = true (= !enabled 不発)
    let enabled = ClassifierConfig { enabled: true, ..ClassifierConfig::default() };

    enrich_with_classifier(&mut state, &enabled);

    assert_eq!(
        state.classified_findings,
        vec![sentinel],
        "findings.is_empty() guard should early return before any mutation"
    );
}
```

注目点:

- **左 arm test**: `findings` を 1 件で pre-populate して右 arm の発火条件 (`is_empty()`) を偽にする
- **右 arm test**: `PrMonitorState::new()` の自然な初期値 (`findings` 空) を使うが、**precondition assert** で「左 arm が確実に不発になる」ことを test 内で文書化 + 機械検証
- 両 variant とも `classified_findings` を sentinel で pre-populate し、mutation 不発を survival assert で検出

## 帰結

### Pros

- guard 順序が変わっても test は依然有効 (各 variant が独立に短絡 path を検証)
- mutation testing (cargo-mutants 等) で片方の guard を `true` にした mutant が確実に 1 つの test を落とす = test 救命率が上がる
- 将来複合 guard 関数を実装する際の reference として poll.rs テストを直接 cite できる
- `~/.claude/rules/common/code-review.md` checklist (順位 84) を project context (poll.rs 実装 + PR #120 W-001 history) で補完し、checklist 項目の意図が新規実装者に伝わる

### Cons / リスク

- test 関数が 1 → 2 に増える (boilerplate 増加)
- sentinel 用 dummy 値 (`ClassifiedFinding` の field を埋める) を毎回作る必要があるため、helper macro / fixture が将来必要になる可能性 (PR #168 post-merge-feedback Tier 2 #1 で「複合 guard test 再登場時に precondition assert helper 抽出を再評価」と 様子見扱い)
- 副作用検証を伴う test (write_state / println / file IO 等) には本 pattern は適用外で、別途副作用観測の test pattern が必要 (本 ADR の scope 外)

### 適用範囲

- ✅ **対象**: 2+ 条件の OR/AND 早期 return を持つ pure function 系 test (返り値または引数 mutation で結果を観測するもの)
- ❌ **対象外**: 副作用 (file IO / network / println) で結果を観測する test、単一 guard の test (1 条件のみの場合は vacuous 化リスクが本質的に存在しない)

### 既存資料との関係

- `~/.claude/rules/common/code-review.md` checklist (順位 84 land 済): global rule として「複合 guard の独立 variant 検証」要件を 1 行で codify。本 ADR は project-level の rationale + 実装例 + history で補完する layer
- PR #120 W-001 (cli-pr-monitor `enrich_with_classifier` 初期テスト): 本 ADR の動機事例
- PR #168 (順位 83 = sentinel pattern + 直交 precondition 実装): 本 ADR の reference 実装
- `src/cli-pr-monitor/src/stages/poll.rs` (`enrich_with_classifier_skips_when_disabled` / `_skips_when_findings_empty`): canonical 実装例 (関数 doc-comment から本 ADR へリンク可能)

## 改訂履歴

- 2026-05-22: 初版 (順位 139 採用 PR で land、PR #120 W-001 + PR #168 の 2 PR 横断観測ベース)
