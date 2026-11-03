---
title: "マシンを接続する"
description: "マシンを接続する · omochi"
sidebar:
  order: 5
---

リモート接続は明示的に有効にします。両方のマシンにネイティブツールを導入し、アプリのある側で実行します。

```sh
omochi-ui gateway --remote --pair --name workstation
```

Hub側:

```sh
omochi-ui pair --name workstation
# 招待を標準入力へ貼り付け、Ctrl+D。
omochi-ui hub
```

招待は180秒で失効し、ひとつのHubだけを登録できます。秘密情報として扱ってください。登録後は完全なiroh EndpointIdを使って再接続します。検出は相手の場所を探すだけで、認可を与えません。

releaseビルドは本番リレー`https://relay.omochi.fyi`を使い、組み込みの共有トークンで認証します。開発用のdebugビルドはn0の公開リレーを使います。オンライン時のアドレス検索はどちらもn0を使います。`--relay`でリレーを変更でき、debugビルドでも`--relay https://relay.omochi.fyi`で本番リレーを試せます。共有トークンは本番リレーにだけ送信します。

`--offline`は公開検索とリレーを無効にしますが、直接接続とLANのmDNSは維持します。接続できないGatewayが他の相手やローカルサービスを妨げることはありません。

## 確認と取り消し

```sh
omochi-ui status
omochi-ui trust list
omochi-ui trust forget NAME_OR_ENDPOINT_ID
```

取り消しは、稼働中のプロセスが認可更新と接続破棄を確認するまで待ちます。信頼レコードの削除後でも確認に失敗すればエラーです。無視して成功と見なさないでください。旧方式の信頼は移行しません。ネイティブ版への破壊的更新後は旧サービスを止め、再ペアリングとダッシュボードの再作成が必要です。
