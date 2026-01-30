---
name: block-git-commands
enabled: true
event: bash
pattern: ^git\s+
action: block
---

**git コマンドの使用はブロックされました**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
git コマンドを直接使用すると、バージョン履歴に不整合が生じます。

**代わりに jj コマンドを使用してください:**

| git コマンド | jj コマンド |
|-------------|------------|
| git status | jj status |
| git log | jj log |
| git add + commit | jj describe -m "message" |
| git diff | jj diff |
| git branch | jj branch list |

詳細は CLAUDE.md の "Version Control" セクションを参照してください。
