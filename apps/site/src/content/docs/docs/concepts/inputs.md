---
title: "ブラウザが持つ入力"
description: "ブラウザが持つ入力 · omochi"
sidebar:
  order: 2
---

## 入力途中の値はレンダラーに

Surface内で`ui.local("form", initialValue)`を宣言し、入力部品で編集するフィールドを選びます。数値の`temperature`を持つフォームの場合は次のように参照します。

- TypeScript: 部品の結び付けには`form.field("temperature").bind()`、Actionに渡す入力式には`form.field("temperature").value()`を使います。
- Python: `RunInput`のdataclassなど型付きの初期値を使い、部品の`bind=`には`form.temperature`を渡します。Actionには`input=form`でフォーム全体を渡すか、`input={"temperature": form.temperature}`で入力を組み立てます。明示的な型付き参照には`form.field("temperature", float)`に続けて`.bind()`や`.value()`を使います。
- Rust: `RunInput`に`ActionInput`をderiveし、部品の結び付けには`form.field(RunInput::field_temperature()).bind()?`、生成された`RunInputInput`の入力式には`form.field(RunInput::field_temperature()).value()`を使います。

フィールドを選ばなければ`bind()`と`value()`はLocalの値全体が対象です。Action登録は[SDKリファレンス](../../reference/sdk/)、フォーム全体の例は[SDKガイド](../../guides/sdks/)を参照してください。

操作時にレンダラーが式を確定済みJSONに変換します。アプリのRuntimeは入力途中の値を保存しません。

## 再描画とリセット

Localの初期値は最初の一度だけ適用され、サーバーが再描画しても入力は保たれます。明示的に戻す場合はTypeScriptの`resetKey`、Pythonの`reset_key`、Rustの`local_with`を使います。アプリの状態更新だけでは入力を上書きしません。

条件付きで現れる入力にも安定したキーを付けます。フォーム全体の例は[SDKガイド](../../guides/sdks/)を参照してください。
