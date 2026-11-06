---
title: "カタログ生成"
description: "カタログ生成 · omochi"
sidebar:
  order: 8
---

```sh
mkdir -p src
omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

出力先のディレクトリを作成してから実行します。出力言語は一部だけ指定できます。`--check`は既存生成物が古いと失敗します。生成だけではブラウザにカスタム部品の実装は追加されません。

| オプション | 意味 |
| --- | --- |
| `--catalog FILE` | 必須のカタログJSON |
| `--name NAME` | 型名。既定はカタログのtitle、なければ`GeneratedCatalog` |
| `--typescript FILE` | TypeScriptの出力先 |
| `--python FILE` | Pythonの出力先 |
| `--rust FILE` | Rustの出力先 |
| `--check` | 書き込まず検査し、生成物が存在しないか古ければ失敗 |

最低ひとつの出力言語を指定してください。スキーマをネットワークから取得することはありません。

## 名前とプロパティ

コンポーネント名は生成する型名と通信上の名前にそのまま使うため、`[A-Za-z_][A-Za-z0-9_]*`に一致する必要があります。TypeScriptでは各位置に`$`も使えます。Pythonではクラス内の参照に名前マングリングが働くため、先頭が`__`の名前には対応していません。Rustでは生成する`ComponentProps`がSDKのtraitと重複するため、`Component`を使えません。型名には`Props`を付けるため、`class`などの予約語は使えます。制約は指定した出力言語にだけ適用し、未対応の名前があればどの出力も変更せずにエラーになります。

Pythonは元のプロパティ名を維持し、`class`のような予約語や`data-label`のようなクラスのフィールドにできないキーには関数形式の`TypedDict`を使います。これらのキーは辞書で指定でき、必須・任意の区別も保たれます。

TypeScriptとPythonではコンポーネントのプロパティ名`key`をビルダーのノードキー引数に予約しています。`allOf`やローカル参照を含め、`properties`または`required`に`key`がある場合、TypeScriptまたはPython出力の指定はどの出力も変更せずにエラーになります。これらの言語向けにはプロパティ名を変更してください。Rustではノードキーをプロパティとは別の引数で渡すため使用できます。入れ子のオブジェクト内のキーは3言語とも使用できます。

Rustのフィールドはsnake caseになりますが、JSON出力ではカタログの元のプロパティ名を略語やアンダースコアも含めて維持します。Rustの予約語には`r#type`のようなraw identifierを使い、`self`・`super`・`crate`には末尾に`_`を付けます。Rustの識別子に変換できない名前や、変換後に同じフィールド名になるプロパティがある場合は、どの出力も書き込まずにエラーになります。

## enum

`enum`は文字列・真偽値・数値・nullを扱え、配列やunion内でもJSONの型を保ちます。空のenumや、オブジェクト・配列を値に持つenumは出力前にエラーになります。TypeScriptはリテラルのunion、Pythonは`True`・`False`・`None`を使った`Literal`を生成し、小数の値は`float`に広げます。Rustは文字列プロパティの名前付きenumを維持し、同じ型の非文字列enumには`bool`・`serde_json::Number`・`()`、型が混在するenumには`Value`を使います。ネストした文字列enumは`String`になります。広げた型はJSONの値を保ちますが、宣言されたenumの値だけに制限するものではありません。Rustの文字列variant名が変換後に重複する場合は数値の接尾辞を付けます。

## スキーマの合成

ローカル`$ref`と兄弟キーワードは[JSON Schema 2020-12の参照規則](https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.3.1)に従い、参照先と兄弟の両方の制約を適用します。たとえば`enum: ["a", "b"]`への参照に`enum: ["b", "c"]`を添えると、TypeScriptでは`"b"`だけを受け付ける交差型になります。`description`などの注釈だけでは型を変えません。`required`・`properties`・`items`だけの制約には、参照先が保証するobjectまたはarray型を補います。ローカルにプロパティ定義がない必須フィールドはTypeScriptで`unknown`とし、交差型によって参照先の型も保持します。相対参照はそれぞれの記述元ファイルを基準に解決します。PythonとRustには下記の`allOf`近似を適用します。

プロパティや配列要素のスキーマでは、`$ref`がなくても隣接する制約を合わせます。たとえば文字列または数値を受け付ける`anyOf`に`enum: ["allowed"]`を添えると、TypeScriptでは`"allowed"`だけを受け付けます。`allOf`に隣接するオブジェクトのプロパティや必須フィールドも交差型に残ります。PythonとRustには`allOf`と同じ近似を適用します。生成する型はJSON Schemaのすべての制約を検証するものではありません。

コンポーネントの型には、コンポーネント自身と入れ子の`allOf`の各層からプロパティと必須フィールドを収集し、共通mixinも反映します。同名のプロパティはスキーマを`allOf`として合成するため、TypeScriptでは層の順序によらずすべての制約が必要になります。同一のスキーマは重複を除き、異なるスキーマの合成にはPythonでは下記の近似、Rustでは`Value`を使います。必須フィールドは各層の指定を合わせます。条件分岐の中にあるフィールドは、無条件のコンポーネントプロパティとしては追加しません。

コンポーネントレベルの`anyOf`と`oneOf`には対応していません。入れ子の`allOf`や解決したローカル参照に含まれる場合も、どの出力も変更せずにコンポーネント名とunionのキーワードを示すエラーになります。プロパティのスキーマ内では、配列要素や入れ子のオブジェクトプロパティも含めてunionを利用できます。

Pythonには型の交差演算子がないため、プロパティの`allOf`で各層が同じPython型になる場合はその型にまとめます。オブジェクト同士なら`dict[str, Any]`、単一の層ならその型を使い、異なる型の組み合わせや空の`allOf`は`Any`に広げます。他言語と同様に`if`または`then`を持つ層は省略し、残る層がなければPythonでは`Any`を使います。この近似は配列やunion内でも適用され、JSONの値は保ちますが、スキーマの全制約を検証するものではありません。空のスキーマ`{}`は制約を加えず、TypeScriptでは`unknown`、Pythonでは`Any`、Rustでは`Value`を使います。真偽値スキーマなど未対応の層は、どの出力も書き込まずにエラーになります。
