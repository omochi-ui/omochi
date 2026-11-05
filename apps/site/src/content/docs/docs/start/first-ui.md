---
title: "最初のUI"
description: "最初のUI · omochi"
sidebar:
  order: 2
---

## Hubを開く

任意のディレクトリで、ターミナル1に次を実行します。Node.js 22以上とLinuxまたはmacOSが必要です。別の導入方法は[インストール](../installation/)を参照してください。

```sh
npx @omochi-ui/cli hub
```

**http://127.0.0.1:5555/** を開きます。HubはローカルGatewayも自動起動するので、これだけでアプリを検出できます。

## 最初のSurfaceを追加する

ターミナル2で、現在時刻を表示するツールを起動します。

```sh
npx @omochi-ui/utils watch --interval 2s -- date
```

ブラウザでダッシュボードを作成し、**Add view**からこのツールのSurfaceを追加して保存します。2秒ごとに時刻が更新されます。SDKアプリを書く前に、Hubとutilsだけで表示まで確認できます。終了時は各ターミナルでCtrl+Cを押します。

## 自分のSDKアプリを接続する

utilsの代わりに自分のアプリを起動し、同じ**Add view**から追加します。ローカル検出には`UnixSocketServerTransport`での公開が必要です。`app.start()`の既定は標準入出力で、Hubには自動で現れません。[SDKガイド](../../guides/sdks/)と[接続方法](../../guides/dashboards/)を参照してください。

以下はOmochi本体の開発や同梱サンプルを試すための、ソースチェックアウト向けの手順です。先に上のHubを停止しておきます。

## 同梱の作業場を開く

[インストール](../installation/)後、リポジトリのルートで実行します。

```sh
bun run quickstart
```

ビルド後、一時的なダッシュボードが開きます。**Preview server**パネルからViteを起動すると、HTTPヘルス表示が正常になります。レイアウトを残すには**Save as dashboard**を選びます。Ctrl+CでHubと、この構成が起動したプロセスを停止します。

## ひとつのプログラムを表示する

ルートディレクトリでターミナルを2つ使います。ターミナル1:

```sh
bun run --filter @omochi-ui/dev-server dev
```

ターミナル2:

```sh
bun run omochi-ui dev -- uv run --project examples/python examples/python/app.py
```

**http://127.0.0.1:5173/**を開き、温度を変更して**Run**を押します。ブラウザが入力値を確定し、PythonのActionが状態を更新すると、結果がSurfaceに表示されます。

プレビュー画面とブリッジは別のサーバーです。ブリッジの既定ポートは5555、画面は5173です。quickstartのHubも5555を使うので、先に停止してください。

## 他の言語でも試す

ターミナル1を動かしたまま、ターミナル2を次のいずれかに置き換えます。

```sh
# TypeScript
bun run omochi-ui dev -- bun run --cwd examples/typescript start
# Rust（アプリはひとつずつ起動）
bun run omochi-ui dev -- cargo run --manifest-path examples/rust/Cargo.toml
```

次は[ランタイムの基本](../../concepts/runtime/)と[SDKガイド](../../guides/sdks/)へ進みます。
