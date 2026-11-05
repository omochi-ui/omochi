---
title: "マシンを接続する"
description: "マシンを接続する · omochi"
sidebar:
  order: 5
---

リモート接続には、GatewayがアプリへのアクセスをHubに許可する境界と、ネイティブHubがオーナー・共有クライアントを許可する境界があります。検出やリレーの入場認証だけでは、どちらの権限も得られません。

## GatewayをネイティブHubに接続する

両方のマシンにネイティブツールを導入し、アプリのある側で実行します。

```sh
omochi-ui gateway --remote --pair --name workstation
```

Hub側:

```sh
omochi-ui pair --name workstation
# 招待を標準入力へ貼り付け、Ctrl+D。
omochi-ui hub
```

Gatewayは`omochi://invite/<payload>`形式の招待、`https://app.omochi.fyi/#g:<payload>`形式のHTTPSリンクとQRを表示します。`pair`はどちらのリンクも受け付けます。同じパディングなしbase64url JSONと秘密を含み、旧hex形式や`#connect:`形式には対応しません。標準入力を使うと秘密がシェル履歴に残りません。

招待は180秒で失効し、最初に登録した認証済みHubひとつを許可します。同じidentityなら有効期間内に再試行できます。権限は、そのGatewayが公開するすべてのサービス・操作・リソースに及びます。登録後は完全なiroh EndpointIdで再接続します。別のHubには別の招待を発行してください。

## ホストされたブラウザーから接続する

Hubのマシンで実行します。

```sh
omochi-ui hub --remote
```

ローカル画面`http://127.0.0.1:5555/`の接続セクションから **オーナーブラウザを登録** し、対象ブラウザーでリンクを開いて明示的に承認します。リンクは3分で失効しますが、登録後の権限は取り消すまで有効です。ホストされたブラウザーはこのネイティブHubに接続するため、Hubを起動したままにします。独自の静的クライアントには`--web-url HTTPS_BASE`を使います。

オーナーは全ダッシュボード、ビュー・接続元の変更、アプリ操作、共有管理、Gateway登録を行えます。オーナー権限の発行・取り消しはHubのローカル画面だけで行います。ひとつのボードだけを公開する場合は[ダッシュボード共有](../dashboards/#保存したダッシュボードを共有する)を使います。

ブラウザーからGatewayを追加するには、アプリ側で`gateway --remote --pair`を実行し、表示されたHTTPSリンクまたはQRを開きます。接続先オーナーHubを選んで **このハブに追加** を押します。Gatewayに登録するのはブラウザー自身ではなく、そのネイティブHubのidentityです。共有ビューアーはGatewayを登録できません。

登録処理は、開始したオーナー権限とブラウザーidentityに紐付きます。切断した場合は同じ権限で再接続し、状態を確認します。保存待ちなら、招待を再使用せずHub側の信頼情報の保存だけを再試行できます。招待画面を閉じても、Hubですでに開始した処理は中止されません。

## Flutterアプリ

[モバイル・デスクトップアプリ](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md)では、Gateway招待を **この端末に追加** すると端末内のRust Hubを登録できます。外部Hubのオーナー・共有リンクも開くか貼り付けられ、その接続を選ぶ間は端末内Hubを休止します。ブラウザーとは別のidentityなので、アプリには新しい登録招待を発行してください。

招待の貼り付けと`omochi://`による受け渡しに対応します。HTTPSリンクからアプリを直接起動する設定は現在無効で、`mobile.omochi.fyi`は案内ページだけを配信します。環境構築と検証範囲はアプリのガイドを参照してください。

## ネットワーク設定

releaseビルドは本番リレー`https://relay.omochi.fyi`を使い、組み込みの共有トークンで入場認証します。debugビルドはn0の公開リレーを使います。オンライン時のアドレス検索はどちらもn0です。`--relay`で既定のリレーを置き換えられ、debugでも`--relay https://relay.omochi.fyi`で本番リレーを試せます。共有トークンは本番リレーの正確なURLにだけ送ります。ブラウザーはHubリンク内のリレー情報を使います。

`--offline`は公開検索とリレーを無効にしますが、直接接続とLANのmDNSは維持します。接続できないGatewayが他の相手やローカルサービスを妨げることはありません。`hub --remote`と`--offline`は併用できません。HTTP/WSのループバックとOrigin検証を維持してください。ホストされたクライアントのiroh接続にHTTPサーバーの外部公開は不要です。

## 確認と取り消し

```sh
omochi-ui status
omochi-ui trust list
omochi-ui trust forget NAME_OR_ENDPOINT_ID
```

Gatewayの信頼情報と、Hubのオーナー・共有権限は別です。Hubのアプリへのアクセスを失効させるにはGateway側で`trust forget`を使い、オーナー・共有権限はHubで管理します。クライアント側の接続記録の削除だけでは、サーバー側の権限は取り消せません。

取り消しは、稼働中のプロセスが認可更新と接続破棄を確認するまで待ちます。記録の削除後でも確認に失敗すればエラーです。送達済みのデータや実行済みの操作は取り消せません。旧方式の信頼は移行しないため、ネイティブ版への破壊的更新後は再ペアリングが必要です。

詳細は[共有と回復のガイド](https://github.com/omochi-ui/omochi/blob/main/docs/dashboard-sharing.md)と[現行仕様の索引](https://github.com/omochi-ui/omochi/blob/main/spec/README.md)を参照してください。
