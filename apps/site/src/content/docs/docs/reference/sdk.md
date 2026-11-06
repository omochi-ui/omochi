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
| Action登録 | `@app.action`または`@app.action(name)` | `app.action(name, schema, handler)` | `app.action::<Input, _, _>(name, handler)?` |
| ビュー登録 | `app.mount(view)`または`@app.surface(name)` | `app.surface(name, view)` | `app.surface(name, view)?` |
| stdio起動 | `await app.start()` | `await app.start()` | `app.start().await` |
| Transport指定 | `await app.start(transport)` | `await app.start(transport)` | `app.start_with(transport).await` |
| 状態更新 | `await ctx.update(mutator)` | `await ctx.update(mutator)` | `ctx.update(mutator).await?` |

Pythonはハンドラーの第1引数の型注釈から入力スキーマを推論します。明示する場合は`schema=`を使います。状態更新には`async with ctx.update() as state:`も使えます。TypeScriptはスキーマ引数が必須です。Rustはハンドラーの入力型へデシリアライズし、型付きの入力式とフィールド参照には`ActionInput`をderiveします。

## Runtimeと宣言

`App.build()`はTransportを選ばずRuntimeを作ります。Runtimeには`update`・`handle`とAction呼び出しがあります。`handle`はJSONの`getSurface`観測と`invoke`コマンドを受け取ります。キーが部品のIDになり、ルートは常に`root`です。

`Action.call(input)`は部品に渡すAction宣言を作ります。Rustでは`action.call(input)?`です。入力部品への結び付けやブラウザ上の値の参照には、先にフィールドを選びます。

| 言語 | 入力部品への結び付け | Actionに渡す入力式 |
| --- | --- | --- |
| Python | `form.field("temperature", float).bind()` | `form.field("temperature", float).value()` |
| TypeScript | `form.field("temperature").bind()` | `form.field("temperature").value()` |
| Rust | `form.field(RunInput::field_temperature()).bind()?` | `form.field(RunInput::field_temperature()).value()` |

ここで`form`は数値の`temperature`フィールドを持つLocalで、Rustの`RunInput`は`ActionInput`をderiveした型です。Pythonでは`form.temperature`を部品の`bind=`やActionの`input=`に直接渡すこともできます。`bind()`と`value()`は現在のLocalを参照します。フィールドを選ばなければ値全体が対象です。Actionに到達する前に入力は確定済みJSONになります。[ブラウザが持つ入力](../../concepts/inputs/)も参照してください。

## 進捗と更新の失敗

長い処理を続ける前に、Contextの各更新をawaitします。

```python
@app.action
async def work(input: RunInput, context: ActionContext[State]):
    async with context.update() as state:
        state.status = "Working"
    await do_work(input)
    async with context.update() as state:
        state.status = "Done"
```

更新ごとに全Surfaceを描画し、Transportの送信完了を待ちます。同時に見せたい変更はひとつの更新にまとめます。`invoke()`と`handle()`は、送信済みのものも含め、生成したメッセージを順番に返します。完了時の再送はせず、Transport未接続でも同じリストを返します。

Pythonの更新ブロックとコールバック更新は同じロックを使います。正常終了時にrevisionを1回増やして送信を待ちます。例外・キャンセル時も変更は残りますが、revisionの更新と描画は行いません。送信失敗時も変更と送信済みメッセージは残ります。同一タスク内の入れ子は`DeclarationError`、別タスクは直列化します。Action完了後の更新は拒否します。進捗を表示するには、長い処理を更新ブロックの間に置いてください。

## Transport

公開アダプター・検出・直接WebSocketサーバー・終了処理は[Transportリファレンス](../transports/)を参照してください。

## エラー

Python・TypeScriptでは宣言不正が`DeclarationError`、プロトコル不正が`ProtocolError`、機能の非対応が`CapabilityError`です。Rustは`Error`と`Result`を使います。Context更新とTransportの失敗は呼び出し元へ伝わり、完了済みの更新は巻き戻りません。

## 型付きシグネチャ

- [TypeScriptの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/index.ts)
- [Pythonの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/python/src/omochi_ui/__init__.py)
- [Rustの公開API](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/rust/src/lib.rs)

[実行例](../../guides/sdks/)・[コンポーネント](../components/)・[メディア](../../guides/media/)も参照してください。

スキーマ・フォーム・Serdeの詳細は[型付き入力](../typed-inputs/)を参照してください。
