---
title: "ダッシュボードを構成する"
description: "ダッシュボードを構成する · omochi"
sidebar:
  order: 2
---

## 検出可能なアプリを公開

ビルド済みチェックアウトで実行します。

```sh
OMOCHI_SERVICE=python-example uv run --project examples/python examples/python/app.py
```

別のターミナル:

```sh
bun run omochi-ui hub
```

http://127.0.0.1:5555/ でダッシュボードを作り、Surfaceを追加して保存します。同梱のアプリは`OMOCHI_SERVICE`があるとUnixソケットで公開します。Hubは必要ならローカルGatewayを起動しますが、検出対象のアプリ自体は起動しません。

## upでプロセスを構成

`omochi-ui up --open [FILE]`はプリセットに宣言したサービスとHubを起動します。quickstartの設定は`examples/quickstart/omochi.toml`です。最初のレイアウトは保存するまで一時的です。Ctrl+Cで`up`が所有するプロセスを停止します。

## 配置とライフタイム

Surfaceは個別に配置できるビューです。ダッシュボードの保存は配置の保存であり、アプリの実行状態の保存ではありません。サービス停止中は利用できず、アプリの再起動でSurfaceが戻ります。`hub`と`dev`の既定ポートはともに5555なので、片方を止めるかポートを変えます。

詳細は[CLI](../../reference/cli/)と[設定](../../reference/configuration/)へ。
