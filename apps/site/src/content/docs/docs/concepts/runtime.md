---
title: "状態・Action・Surface"
description: "状態・Action・Surface · omochi"
sidebar:
  order: 1
---

## アプリの状態

`App`が状態と宣言を持ち、`App.build()`が通信方式に依存しないRuntimeを作ります。稼働中の状態は、Action内では`context.update()`、独立した処理では`runtime.update()`を通して更新します。

## Surface

Surfaceは状態を表示する名前付きビューです。描画関数がコンポーネントを宣言し、差分からA2UIメッセージを生成します。ルートのIDは常に`root`です。条件分岐や並べ替えを伴う要素には明示的なキーを付け、同一性を維持します。

## Actionと進捗

Actionは確定済みの型付き入力と更新用Contextを受け取ります。レンダラーのBindingにはアクセスしません。Contextの更新をawaitするたびに全Surfaceを描画し、差分の送信完了後に戻ります。送信完了はブラウザの描画完了を意味しません。

Actionは直列です。途中経過の公開によって並列実行にはなりません。Action中に`runtime.update()`をawaitすると自身の完了待ちになるため、Contextを使ってください。更新はハンドラーの終了前にawaitします。エラー時も送信済みの更新は巻き戻りません。

## 通信との境界

既定の起動は標準入出力のJSONLです。開発ブリッジ、直接WebSocket、検出可能なローカルIPCなどで画面につなぎます。Transportはひとつのアプリを扱い、Hubのセッション経路はA2UIの外側で管理します。
