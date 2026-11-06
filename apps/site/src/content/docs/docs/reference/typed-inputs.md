---
title: "型付き入力"
description: "型付き入力 · omochi"
sidebar:
  order: 7
---

Actionは確定済みの値を受け取ります。入力部品はレンダラー内のLocalを編集し、Actionの実行時に入力式がその値を読み取ります。結び付けの構文は[ブラウザーが持つ入力](../../concepts/inputs/)、アプリ全体の例は[SDKガイド](../../guides/sdks/)を参照してください。

## スキーマと検証

TypeScriptのActionにはスキーマが必須で、ハンドラーの入力型をそこから推論します。Pythonはハンドラーの第1引数の型注釈から推論するか、`schema=`を明示できます。RustはSerdeでハンドラーの入力型へデシリアライズします。

`schema`は文字列・真偽値・数値・整数・literal・nullable・optional・配列・tuple・map・object・unionを検証します。Pythonのtuple/map/objectは`tuple_`・`map_`・`object_`で、型注釈からは`from_type`で作れます。objectは既定で未知のフィールドを拒否し、TypeScriptの`unknownKeys`、Pythonの`unknown_keys`に`strip`か`allow`を指定すると変更できます。独自変換には`custom`を使います。文字列から数値への暗黙変換はせず、Pythonの真偽値は整数として受け付けません。

入力部品の範囲指定だけでは受信値を検証できません。Pythonなら`schema.from_type(RunInput).refine(lambda v: 0 <= v.temperature <= 100, "out of range")`でサーバー側の制約を加えられます。

クライアント入力の検証に失敗すると、そのActionのハンドラーを実行せず、サービスは次のメッセージを受け付けます。直接の`invoke()`も失敗します。Python・TypeScriptでは不正な通信入力が`ProtocolError`、不正・循環した宣言値が`DeclarationError`になります。ハンドラー例外とTransportの失敗は呼び出し元へ伝わります。

## TypeScript

```ts
const run = app.action("run", schema.object({ temperature: schema.number() }), handler);
// Surface内:
const form = ui.local("form", { temperature: 50 });
ui.slider("Temperature", 0, 100, { bind: form.field("temperature").bind() });
ui.button("Run", run.call({ temperature: form.field("temperature").value() }));
```

フィールドを選んでから`.bind()`や`.value()`を呼びます。フォーム自身の`.value()`は値全体を参照します。必須フィールド・入れ子の構造・入力式の結果型はコンパイラーが検査します。

## PythonのAction

型注釈を持つハンドラーに`@app.action`を付けます。元の関数も保持する場合は`run = app.action(handler, name="run")`を使います。注釈のないハンドラーには明示スキーマが必要です。デコレーターはActionハンドルを返し、`.call()`は実行せずに宣言を作ります。`on_click=`には登録済みのActionハンドルを渡します。

`app.mount(main)`は同期関数`main(state)`を受け取り、`from omochi_ui import ui`で宣言できます。`@app.surface("main")`で`(ui, state)`を受け取る関数も登録できます。静的な型注釈には`App[State, UI]`と`ActionContext[State]`を使え、パッケージには`py.typed`を含みます。

`input=form`でフォーム全体、`input={"temperature": form.temperature}`でフィールドを組み合わせて渡します。Action宣言を別途作る場合は`run.call(form)`、または`run.call(run.compose({"temperature": form.temperature}))`を使います。同じボタンでAction callと別の`input=`を併用しないでください。`input=`を省略できるのは入力スキーマか型注釈が`None`の場合だけです。`InputSchema(Concrete, Expression)`と明示フィールド記述子も使えますが、入力式用のTypedDictは必須ではありません。

入力dataclassには`init=True`のフィールドだけを含めます。`field(init=False)`の計算フィールドはエンコードと入力式用TypedDictから除き、デコード時にコンストラクターか`__post_init__`が補います。計算フィールドを送ると未知の入力として拒否します。unionのメンバーや部分集合を返す式も使え、`str | None`は`Expr[str]`・`Expr[None]`・`Expr[str | None]`を受け付けます。

## Pythonのフォームと再利用可能なUI

Stateは通常のPythonオブジェクトで、アプリの接続クライアント間で共有します。Localの入力途中の値は各レンダラーが独立して持ちます。Action内はContext更新、バックグラウンド処理はRuntime更新を使い、描画ではStateを読み取ります。

dataclassの初期値からはスキーマを取得できます。`ui`経由で辞書やコンテナーのLocalを作る場合は`type=`を指定します。

```python
from typing import TypedDict

class FormInput(TypedDict):
    temperature: float

# Surface内:
form = ui.local("form", {"temperature": 50.0}, type=FormInput)
```

型指定は、省略された任意フィールド・union・空コンテナーの要素型も保持します。任意フィールドに入力部品を結び付けるには、初期値にそのフィールドを含めてください。存在しないキーは`None`へ変換しません。

`form.temperature`と`form["temperature"]`は、部品やActionへ接続するときに型を検証する参照です。`form["path"]`のように、メソッド名と重なるフィールドには角括弧を使います。`form.field("temperature", float)`は型を検査した`Local[float]`を返します。書き込み可能な型は完全一致が必要で、nullableの縮小やliteralの拡大はできません。`.value()`は読み取り用`Ref[T]`、`.bind()`は編集用のBindingを返します。

再利用する部品は関数にし、繰り返し使う箇所に安定したscopeを付けます。

```python
def temperature_control(temperature):
    with ui.column():
        ui.slider("Temperature", 0, 100, bind=temperature)


def main(state):
    for key, initial in [("low", 20.0), ("high", 80.0)]:
        with ui.scope(key):
            form = ui.local("form", RunInput(initial))
            temperature_control(form.temperature)
            ui.button("Run", on_click=run, input=form)
```

`scope()`は部品とLocalのキーに名前空間を付け、並べ替えても入力を維持します。レイアウト要素は追加せず、Surface IDとAction名も変更しません。columnは配置だけを変え、名前空間は作りません。

`ui`は同期描画中だけ使えます。別タスクや描画後の使用は拒否します。参照とキャッシュしたAction callを別のSurfaceや描画に持ち越すこともできません。カスタムカタログには`ui.catalog(CustomUI)`か、カタログ専用の明示ビルダーを使います。

入力には有限の数値・文字列・真偽値・null・Literal/Union・非再帰のdataclassとTypedDict・配列・固定長または同型tuple・文字列キーのmapを使えます。独自クラスや再帰型には明示スキーマが必要です。受信側スキーマだけでは独自値のフォーム用エンコーダーにはなりません。

## Rustの生成入力とSerde

入力型に`Serialize`・`Deserialize`・`ActionInput`をderiveします。`Request`から`Input<T>`のフィールドを持つ`RequestInput`を生成します。値や式には`.into()`、入れ子の生成入力・配列・tuple・option・文字列キーmapには`Input::fields(...)`を使います。`Request::field_name()`が型付きフィールド参照を作ります。

enumのペイロードは`local.variant(Request::variant_name())`で選択し、Localの初期値が別のvariantなら`None`を返します。タグ付きenumはシリアライズした識別子で照合するので、シリアライズ専用ペイロードも扱えます。untagged variantには`DeserializeOwned`が必要です。variantのフィールド記述子は`RequestVariantFields`という名前で生成し、variantの`#[serde(untagged)]`はコンテナーのtag/content経路を上書きします。

flattenしたフィールドの参照は`Local<T, FieldsOnly>`を返します。`form.field(Form::field_nested()).field(Nested::field_count())`のように各フィールドへ進んでから`.value()`か`.bind()`を呼びます。flattenしたオブジェクト全体には兄弟フィールドも含まれるため、全体の読み取り・結び付けはコンパイルエラーです。個々の式を`Input::fields(NestedInput { ... })`で組み合わせるか、別の`Local<Nested>`を使ってください。

内部タグ付きnewtypeのペイロードも、オブジェクトにタグを含むため同じ制約があります。flattenしたフィールドからnewtypeやtransparentの参照を続けても制約は維持します。独立したラッパーは全体を読めます。

入力式のシリアライザーは4種類のenum表現、structの各形式、ジェネリクス、rename/default/skip属性、flattenに対応します。flattenしたキーの重複と非有限数は拒否します。`skip_serializing_if`は確定値でのみ評価し、未確定の式では拒否します。述語は通常の引数型強制を扱い、生成ヘルパーはライフタイム引数を推論します。

独自シリアライズには`#[action_input(adapter = Adapter)]`と`ExpressionAdapter<T>`が必要です。シリアライズ後の`Wire`型、値・式のエンコード、フィールド参照の経路を定義し、フィールドか型全体に適用します。Bindingは`Wire`を使うため、文字列として保存する数値を数値部品へ結び付けることはできません。忠実に変換できない式はアダプターで拒否できます。

## オブジェクトの合成

キーが動的なブラウザー内オブジェクトと、明示フィールドを組み合わせる場合に使います。レンダラーが操作時に各要素を確定し、キーの重複を拒否します。`MERGE_OBJECTS_CAPABILITY`が必要なので、対応するレンダラーに接続するときだけ有効にしてください。設定は[Transport](../transports/)を参照してください。

TypeScriptは結果の型を推論します。

```ts
import { mergeObjects, MERGE_OBJECTS_CAPABILITY, StdioTransport } from "@omochi-ui/sdk";
// Surface内。runは{ title: string; count: number }を受け取る。
const counts = ui.local("counts", { count: 1 });
ui.button("Run", run.call(mergeObjects({ title: "x" }, counts.value())));
// 対応レンダラーにつなぐアプリの起動時:
await app.start(new StdioTransport([MERGE_OBJECTS_CAPABILITY]));
```

Pythonは各要素と結果のスキーマを使います。`Schema.input()`は値か`Expr[T]`を受け取り、`merge_objects()`は結果スキーマに対して型と必須フィールドを検証します。動的mapの結果には、対応するmapスキーマが必要です。

```python
from typing import TypedDict
from omochi_ui import merge_objects, schema, StdioTransport, MERGE_OBJECTS_CAPABILITY

class Counts(TypedDict):
    count: int

title = schema.object_({"title": schema.string()})
counts = schema.object_({"count": schema.integer()})
result = schema.object_({"title": schema.string(), "count": schema.integer()})
# Surface内。counts Localと、resultで登録したActionを使う。
counts_form = ui.local("counts", {"count": 1}, type=Counts)
input_expr = merge_objects(
    result,
    title.input({"title": "x"}),
    counts.input(counts_form.value()),
)
ui.button("Run", run.call(input_expr))
# 対応レンダラーにつなぐアプリの起動時:
await app.start(StdioTransport(capabilities={MERGE_OBJECTS_CAPABILITY}))
```

Rustは生成入力のフィールドを使います。

```rust
#[derive(serde::Serialize, serde::Deserialize, omochi_ui::ActionInput)]
struct RunInput {
    title: String,
    #[serde(flatten)]
    counts: std::collections::BTreeMap<String, i64>,
}
// Surface内のcountsはLocal<BTreeMap<String, i64>>。
let call = run.call(RunInputInput {
    title: "x".into(),
    counts: counts.value().into(),
})?;
// 起動時にStdioTransport::with_capabilities([MERGE_OBJECTS_CAPABILITY])を指定する。
```

確定したflatten値はSerdeの表現を使い、未確定のflatten入力にはcapabilityが必要です。独自シリアライズにはアダプターを使います。Rustの述語はブラウザーで実行できないため、未確定の`skip_serializing_if`フィールドは拒否します。
