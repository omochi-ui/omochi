---
title: "コマンドをツールにする"
description: "コマンドをツールにする · omochi"
sidebar:
  order: 3
---

ネイティブのutilsを使うと、SDKアプリを書かずに検出可能なSurfaceを公開できます。ビルド済みチェックアウトでHubを起動します。

```sh
bun run omochi-ui hub
```

別のターミナルでツールを起動し、ダッシュボードへ追加します。

```sh
bun run omochi-utils button --name "Build" -- bun run build
bun run omochi-utils watch --diff --interval 2s -- git status --short
bun run omochi-utils http http://127.0.0.1:3000/health
```

buttonは押したときにコマンドを実行します。watchは定期実行と差分表示、httpはヘルス表示です。Surfaceを使う間は公開元プロセスを動かしておきます。

`process`は起動・停止の操作、`tail`はファイルの追跡、`stream`は標準入力の表示に使います。引数と動作の詳細は[リファレンス](../../reference/utils/)へ。
