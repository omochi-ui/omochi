---
title: "ブラウザが持つ入力"
description: "ブラウザが持つ入力 · omochi"
sidebar:
  order: 2
---

## 入力途中の値はレンダラーに

Surface内で`ui.local("form", initialValue)`を宣言し、入力部品をそのLocalに結び付けます。`form.bind("/temperature")`はJSON Pointerで入力先を指定し、`form.value("/temperature")`はActionに渡す式を作ります。

操作時にレンダラーが式を確定済みJSONに変換します。アプリのRuntimeは入力途中の値を保存しません。

## 再描画とリセット

Localの初期値は最初の一度だけ適用され、サーバーが再描画しても入力は保たれます。明示的に戻す場合はTypeScriptの`resetKey`、Pythonの`reset_key`、Rustの`local_with`を使います。アプリの状態更新だけでは入力を上書きしません。

条件付きで現れる入力にも安定したキーを付けます。フォーム全体の例は[SDKガイド](../../guides/sdks/)を参照してください。
