---
title: "MCPと承認画面"
description: "MCPと承認画面 · omochi"
sidebar:
  order: 4
---

## MCPサーバーを設定する

[CLIとutilsをインストール](../../start/installation/)し、MCP対応エージェントに次を設定します。

```json
{
  "mcpServers": {
    "omochi": {
      "command": "omochi-utils",
      "args": ["mcp", "--service-id", "agent", "--name", "Agent"]
    }
  }
}
```

エージェントからコマンドが見つからない場合は、`command`にインストール先の絶対パスを指定してください。

## エージェントのビューを表示する

別のターミナルで`omochi-ui hub`を起動します。エージェントに「プロジェクトを調べ、概要と次に使えるコマンドを`main` Surfaceとして公開して」と依頼し、Hubの **Add view** から追加します。

SurfaceはMCPサーバーの実行中だけ利用できます。ボタン操作に応答するには、エージェントが`read_events`でイベントを読み、表示を更新する必要があります。ボタンを押すだけでは待機中のエージェントは起動しません。ツールの引数と使用例は[utilsリファレンス](../../reference/utils/#mcp)を参照してください。

## ツール実行の承認画面を作る

[agent approvalの例](https://github.com/omochi-ui/omochi/tree/main/examples/agent-approval)には、Codex・Claude Code・Cursor・OpenCode向けのApprove/Reject画面があります。フックの設定とデモの起動は例のREADMEに従ってください。この例はソースチェックアウトから実行します。

承認画面は各エージェントの権限制御に従います。利用できるフックと、サービス停止時の動作を確認してから使ってください。
