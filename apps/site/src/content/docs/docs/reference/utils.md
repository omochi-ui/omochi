---
title: "utilsリファレンス"
description: "utilsリファレンス · omochi"
sidebar:
  order: 3
---

`omochi-utils`はコマンドや入力を通常の検出可能サービスとして公開します。Hubを起動し、公開されたSurfaceを追加して使います。ソースでは`bun run omochi-utils`として実行します。

## モード

| モード | 入力 | 画面でできること |
| --- | --- | --- |
| `button` | `--`以降のコマンド | 押下して実行 |
| `watch` | `--interval`とコマンド | 定期実行、`--diff`で差分 |
| `process` | 長時間動くコマンド | 起動・停止、状態確認 |
| `tail` | ファイル | ログを追跡 |
| `stream` | 標準入力 | UTF-8テキストを表示 |
| `http` | URL | HTTPの正常性を確認 |
| `mcp` | MCPの標準入出力 | エージェントがSurfaceを公開 |

```sh
omochi-utils button --name "Build" -- bun run build
omochi-utils watch --diff --interval 2s -- git status --short
omochi-utils http http://127.0.0.1:3000/health
omochi-utils mcp
```

## 公開名とライフタイム

`--service-id`で検出用ID、`--name`で表示名を指定します。公開元プロセスの終了はサービスの終了です。アプリの状態や出力がHubのダッシュボード保存に含まれるわけではありません。

## MCP

エージェントがstdioサーバーを起動します。`tools/list`で利用可能なツールと入力スキーマを取得してから呼び出してください。Surfaceの作成・更新・取り下げを通じてエージェントの結果を画面へ公開します。MCP接続が終了すればSurfaceも取り下げられます。[接続例](../../guides/agents/)を参照してください。

## 引数を確認

```sh
omochi-utils --help
omochi-utils watch --help
omochi-utils mcp --help
```

モード別の全フラグとMCPツールの詳細なJSON例は[英語リファレンス](/en/docs/reference/utils/)に掲載しています。
