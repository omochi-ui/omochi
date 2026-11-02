---
title: "コンポーネントとカタログ"
description: "コンポーネントとカタログ · omochi"
sidebar:
  order: 4
---

## Basic Catalog

SDKはA2UI v0.9のBasic Catalogの18部品に型付きヘルパーを提供します。各言語の`UI` builderでテキスト・画像・動画・音声、行・列・リスト、カード・タブ・モーダル・区切り、ボタン・入力・チェック・選択・スライダー・日時入力を宣言します。

`component()`ではワイヤー形式のプロパティを直接指定でき、部品名に対応する生成済み型が選ばれます。必須項目と正確なプロパティ名は[生成カタログ型](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/catalog.ts)を参照してください。

## IDとレイアウト

条件分岐や並べ替えには明示キーを使います。キーは部品IDになり、`root`はルートに予約されています。Surfaceは任意のHTML/CSSではなくSDKのコンテナで構成します。入力は[Local](../../concepts/inputs/)に結び付けてください。

## カスタムカタログ

```sh
bun run omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

必要な言語だけ出力できます。CIでは`--check`で生成物の更新漏れを確認します。スキーマはローカルのみです。生成したバインディングを使うには対応レンダラーが必要で、生成だけでは同梱画面に部品は追加されません。
