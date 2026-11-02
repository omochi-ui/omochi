---
title: "最初のUI"
description: "最初のUI · omochi"
sidebar:
  order: 2
---

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
