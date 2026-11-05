---
title: "アーキテクチャ"
description: "アーキテクチャ · omochi"
sidebar:
  order: 1
---

OmochiのサーバーはRust、SDKはPython・TypeScript・Rust、共通のブラウザレンダラーとダッシュボードはTypeScript/Reactです。npmのCLIはネイティブ実行ファイルの起動用で、別実装のサーバーではありません。

```text
SDKアプリ → Unix JSONL → Gateway → ネイティブHub
                                      ├─ HTTP/WS/SSE → ローカルブラウザー
                                      └─ iroh → ホストされたブラウザー / Flutter

Gateway → 認証済みiroh → Flutterアプリ内のRust Hub
```

## 役割

| 要素 | 責務 |
| --- | --- |
| SDK Runtime | 状態更新、Action、宣言、A2UI v0.9差分 |
| Discovery | ローカルマニフェストの検証とライフタイム監視 |
| Gateway | SDK接続の共有、スナップショット再送、操作・リソースの中継 |
| Hub | Gateway集約、ダッシュボード、ローカルHTTP/WS/SSEとリモートのオーナー・共有接続 |
| Downstream core | WASM・ネイティブクライアント共通のオーナー・共有契約と適用・ACK状態 |
| Native bridge | Flutterの端末内Hub・外部Hub接続を提供するC ABI v2 |
| Hub transport | ホストされたブラウザーからネイティブHubへ接続するWASM iroh transport |
| Renderer | 部品の描画、入力途中の値、操作送信 |

## 境界

GatewayはサービスごとにひとつのSDK接続を共有します。HubはSurface IDに名前空間を付け、操作と入力メタデータを対象に絞って戻します。A2UIメッセージにセッション用の独自ラッパーは加えません。

リモートは明示登録したEndpointIdと役割で認可します。セッションの世代で古い操作を拒否し、再接続時は新しいスナップショットから開始します。失効時は全参加プロセスが認可を更新し接続を破棄するまで待ちます。

ネイティブ版にはビルド済み画面が埋め込まれています。開発プレビューは別サーバーです。通信の詳細は[Gateway v4仕様](https://github.com/omochi-ui/omochi/blob/main/spec/gateway-v4.md)、実装構成は[開発者向けアーキテクチャ](https://github.com/omochi-ui/omochi/blob/main/docs/architecture.md)を参照してください。

## クライアントと権限

ホストされたダッシュボードは、稼働中のネイティブHubへオーナーまたは共有権限で接続します。ボードと認可はHubが持ち、静的Webホストは画面のファイルを配信します。オーナーは全ボード・共有の管理とGateway登録を行えます。共有は保存済みボードひとつを対象にし、リンクごとに操作・レイアウト編集を個別に許可できます。Gatewayの信頼とHubの権限は、登録・取り消しを別に管理します。

FlutterアプリはGatewayに直接接続する共通Rust Hubを内蔵し、外部Hubのオーナー・共有クライアントにもなれます。外部接続を選ぶ間は端末内Hubを休止し、identityはそれぞれ別の保護ストレージに保存します。ブラウザー内でGatewayを集約する方式は`experiments/browser-dashboard`の明示実行する実験として残しています。

ローカルブラウザーのリソース経路はRange・HEADに対応します。ホストされたオーナー・共有クライアントの画像は、ネイティブHubの検証方針を通して取得します。リモートの音声・動画アダプターは未対応です。[接続ガイド](../../guides/remote/)と[ダッシュボード共有](../../guides/dashboards/#保存したダッシュボードを共有する)を参照してください。
