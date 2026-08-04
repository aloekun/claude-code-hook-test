# Phase B 実走スモークテスト用の一時ドキュメント

> **このファイルは削除前提の検証用アーティファクトです。** ADR-067 (Phase B 無人 fix push) の
> スモーク段 2 で、CodeRabbit の docs 指摘 → fix agent の編集 → 4 軸ゲート → workflow による
> push、という allow 経路を実走観測するために作成しました。観測完了後、この PR はマージせず
> クローズし、ブランチごと削除します。プロダクションの手順書として参照しないでください。

## このテストで観測すること

Phase B の fix job が `claude/` prefix ブランチに対して起動し、以下を通過することを確認します。

1. `Decide whether Phase B applies` で `proceed=true` になる
2. CodeRabbit の指摘が決定論的な著者フィルタを通って findings になる
3. fix agent が `pr/docs/**` の範囲で編集する
4. `cli-fix-push-gate` の 4 軸 AND がすべて満たされ exit 0 になる

以上の 3 ステップを順に観測します。

## 実行手順

Actions タブから pr-monitor workflow を手動起動します。ref は master を選び、`pr_number` に
この PR の番号を入れてください。variable の設定は任意です。

なお `AUTONOMY_ENABLED` が `true` でなければ fix job は起動しないため、事前設定は必須です。

## 観測後の後始末

- この PR をクローズする
- ブランチを削除する
- TODO: 観測結果の記録先を決める

## 補足

段 1 では非 `claude/` ブランチに対して prefix 層の deny を確認済みです。段 2 はその対になる
allow 経路の確認にあたります。ゲートの 4 軸の詳細は [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)
を参照してください。
