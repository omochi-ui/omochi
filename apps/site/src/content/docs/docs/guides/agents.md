---
title: "MCPと承認画面"
description: "MCPと承認画面 · omochi"
sidebar:
  order: 4
---

## エージェントにSurfaceを持たせる

チェックアウトをビルドし、MCP対応エージェントに絶対パスで設定します。

```json
{"mcpServers":{"omochi":{"command":"/absolute/path/to/omochi/packages/npm/utils/bin/omochi-utils","args":["mcp","--service-id","quickstart-agent","--name","Agent"]}}}
```

`bun run omochi-ui hub`を起動し、エージェントに「プロジェクトを調べ、概要と次に使えるコマンドを`main` Surfaceとして公開して」と依頼します。Hubのビュー選択から追加してください。Surfaceはエージェントのプロセスが所有し、MCPサーバーの切断時に取り下げられます。

## ツールとの関係

`omochi-utils mcp`はエージェントが起動する標準入出力のMCPサーバーです。通常の検出可能サービスとして公開されるので、SDKアプリやコマンドツールと同じダッシュボードに配置できます。ツールの契約は[utilsリファレンス](../../reference/utils/)にあります。

## 承認UI

[agent approvalの例](https://github.com/omochi-ui/omochi/tree/main/examples/agent-approval)はCodex・Claude Code・Cursor・OpenCodeのツールフック向けにApprove/Reject画面を提供します。拒否理由と、エージェントなしのデモにも対応します。フック別の設定はリンク先READMEに従ってください。承認画面はエージェント自身の権限制御を置き換えません。
