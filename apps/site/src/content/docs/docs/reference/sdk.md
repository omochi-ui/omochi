---
title: "SDK APIリファレンス"
description: "SDK APIリファレンス · omochi"
sidebar:
  order: 1
---

## 公開エントリーポイント

| 概念 | Python | TypeScript | Rust |
| --- | --- | --- | --- |
| App | `App(state)` | `new App(state)` | `App::new(state)` |
| Action登録 | `@app.action(name)` | `app.action(name, handler)` | `app.action(name, handler)?` |
| ビュー登録 | `@app.surface(name)` | `app.surface(name, view)` | `app.surface(name, view)?` |
| stdio起動 | `await app.start()` | `await app.start()` | `app.start().await` |
| Transport指定 | `await app.start(transport)` | `await app.start(transport)` | `app.start_with(transport).await` |
| 状態更新 | `await ctx.update(mutator)` | `await ctx.update(mutator)` | `ctx.update(mutator).await?` |

## Runtimeと宣言

`App.build()`はTransportを選ばずRuntimeを作ります。Runtimeには`update`・`handle`とAction呼び出しがあります。`handle`はJSONの`getSurface`観測と`invoke`コマンドを受け取ります。キーが部品のIDになり、ルートは常に`root`です。

`Action.call(input)`は部品に渡すAction宣言を作ります。`Local.bind(pointer)`は入力の結び付け、`Local.value(pointer)`は送信する入力式を作ります。Actionに到達する前に入力は確定済みJSONになります。

## Transport

Python・TypeScriptは`StdioTransport`・`CallbackTransport`・`WebSocketTransport`を公開します。TypeScriptの`WebSocketServerTransport`と`UnixSocketServerTransport`は`@omochi-ui/sdk/node`、Pythonの`UnixSocketServerTransport`はパッケージ直下です。Rustは`Transport` trait、`StdioTransport`・`CallbackTransport`・`UnixSocketServerTransport`を持ちます。

## エラー

Python・TypeScriptでは宣言不正が`DeclarationError`、プロトコル不正が`ProtocolError`、機能の非対応が`CapabilityError`です。Rustは`Error`と`Result`を使います。Context更新とTransportの失敗は呼び出し元へ伝わり、完了済みの更新は巻き戻りません。

## 型付きシグネチャ

- [TypeScriptの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/index.ts)
- [Pythonの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/python/src/omochi_ui/__init__.py)
- [Rustの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/rust/src/lib.rs)

[実行例](../../guides/sdks/)・[コンポーネント](../components/)・[メディア](../../guides/media/)も参照してください。
