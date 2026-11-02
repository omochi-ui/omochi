---
title: "ドキュメント"
description: "ドキュメント · omochi"
sidebar:
  order: 1
---

OmochiはPython・TypeScript・RustのプログラムにブラウザUIを付けるツールです。状態・処理・ビューをアプリの言語で宣言すると、共通のレンダラーがA2UI v0.9の更新を画面に表示します。

## 目的から探す

- **はじめて使う**：[インストール](./start/installation/)から[最初のUI](./start/first-ui/)へ。
- **アプリを作る**：[状態・Action・Surface](./concepts/runtime/)を理解し、[SDK](./guides/sdks/)を選びます。
- **既存のツールを使う**：[コマンドやログ](./guides/utils/)を公開するか、[MCPエージェント](./guides/agents/)を接続します。
- **複数アプリやマシンを使う**：[ダッシュボード](./guides/dashboards/)に集め、[リモート接続](./guides/remote/)を設定します。

## SDKとネイティブツール

SDKはアプリ内で動作します。ネイティブCLIは開発ブリッジ・Gateway・Hubを実行します。SDK単独の起動はプロトコルを出力するだけで、ブラウザを開きません。

このDocsは現在のリポジトリを対象とします。ネイティブサービスとSDKは別々にリリースされます。ソースからの利用と配布版の手順を区別して案内します。
