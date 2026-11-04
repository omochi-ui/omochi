---
title: "コマンドをツールにする"
description: "コマンドをツールにする · omochi"
sidebar:
  order: 3
---

ネイティブのutilsを使うと、SDKアプリを書かずに検出可能なSurfaceを公開できます。LinuxまたはmacOSとNode.js 22以上があれば、任意のディレクトリでHubを起動できます。

```sh
npx @omochi-ui/cli hub
```

HubがローカルGatewayも自動起動します。**http://127.0.0.1:5555/**を開き、別のターミナルでツールを起動して、**Add view**からダッシュボードへ追加します。[インストール](../../start/installation/)すると`omochi-ui`と`omochi-utils`を直接実行できます。

```sh
npx @omochi-ui/utils button --name "Build" -- bun run build
npx @omochi-ui/utils watch --diff --interval 2s -- git status --short
npx @omochi-ui/utils http http://127.0.0.1:3000/health
```

buttonは押したときにコマンドを実行します。watchは定期実行と差分表示、httpはヘルス表示です。Surfaceを使う間は公開元プロセスを動かしておきます。

`process`は起動・停止の操作、`tail`はファイルの追跡、`stream`は標準入力の表示に使います。引数と動作の詳細は[リファレンス](../../reference/utils/)へ。
