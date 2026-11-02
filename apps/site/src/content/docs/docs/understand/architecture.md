---
title: "アーキテクチャ"
description: "アーキテクチャ · omochi"
sidebar:
  order: 1
---

OmochiのサーバーはRust、SDKはPython・TypeScript・Rust、共通のブラウザレンダラーとダッシュボードはTypeScript/Reactです。npmのCLIはネイティブ実行ファイルの起動用で、別実装のサーバーではありません。

```text
SDKアプリ → Unix JSONL → Gateway → Hub → HTTP/WebSocket → ブラウザ
                            ↑
                リモートGatewayは認証済みirohストリーム
```

## 役割

| 要素 | 責務 |
| --- | --- |
| SDK Runtime | 状態更新、Action、宣言、A2UI v0.9差分 |
| Discovery | ローカルマニフェストの検証とライフタイム監視 |
| Gateway | SDK接続の共有、スナップショット再送、操作・リソースの中継 |
| Hub | Gateway集約、ダッシュボード、HTTPとWebSocket |
| Renderer | 部品の描画、入力途中の値、操作送信 |

## 境界

GatewayはサービスごとにひとつのSDK接続を共有します。HubはSurface IDに名前空間を付け、操作と入力メタデータを対象に絞って戻します。A2UIメッセージにセッション用の独自ラッパーは加えません。

リモートは明示登録したEndpointIdと役割で認可します。セッションの世代で古い操作を拒否し、再接続時は新しいスナップショットから開始します。失効時は全参加プロセスが認可を更新し接続を破棄するまで待ちます。

ネイティブ版にはビルド済み画面が埋め込まれています。開発プレビューは別サーバーです。通信の詳細は[Gateway v4仕様](https://github.com/omochi-ui/omochi/blob/main/spec/gateway-v4.md)、実装構成は[開発者向けアーキテクチャ](https://github.com/omochi-ui/omochi/blob/main/docs/architecture.md)を参照してください。
