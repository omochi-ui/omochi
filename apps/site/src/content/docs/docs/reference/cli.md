---
title: "CLIリファレンス"
description: "CLIリファレンス · omochi"
sidebar:
  order: 2
---

LinuxまたはmacOSとNode.js 22以上があれば、任意のディレクトリで実行できます。

```sh
npx @omochi-ui/cli hub
npx @omochi-ui/cli --help
```

`npm install -g @omochi-ui/cli`または`cargo install omochi-cli --locked`でインストールすると、以下の例のように`omochi-ui`を直接使えます。ダッシュボード画面は同梱され、`hub`がローカルGatewayも自動起動します。対応環境とソースビルドは[インストール](../../start/installation/)を参照してください。ビルド済みチェックアウトでは`bun run omochi-ui`を使います。

## コマンド一覧

| コマンド | 用途 | 注意点 |
| --- | --- | --- |
| `up --open [FILE]` | プリセットのサービス・Hub・一時ダッシュボードを起動 | 設定ファイルを指定可能 |
| `dev -- COMMAND [ARGS...]` | アプリと標準入出力ブリッジを起動 | プレビュー画面は別途起動 |
| `hub` | ダッシュボードサーバー | 必要ならローカルGatewayを起動 |
| `gateway` | 起動中のアプリをHubへ中継 | アプリ自体は起動しない |
| `pair --name NAME` | 標準入力の招待を登録 | 通常通信とは別の登録操作 |
| `status` | Gateway接続状態の確認 | 稼働中Hubへ問い合わせ |
| `id` | 自分のEndpointIdを表示 | 初回はIDを作成 |
| `trust list` | 登録済みの相手を一覧 | 名前とEndpointIdを確認 |
| `trust forget NAME_OR_ENDPOINT_ID` | 相手の権限を取り消す | 稼働プロセスの確認失敗はエラー |
| `generate catalog` | カタログから型付きバインディングを生成 | ローカルスキーマのみ利用 |

## up

```sh
omochi-ui up --open examples/quickstart/omochi.toml
```

プリセットが指定するプロセスとHubを起動します。配置は保存するまで一時的です。Ctrl+Cで所有するプロセスを停止します。

## dev

```sh
# ターミナル1: チェックアウトのルート
bun run --filter @omochi-ui/dev-server dev
# ターミナル2
bun run omochi-ui dev -- uv run --project examples/python examples/python/app.py
```

既定のブリッジは5555、プレビューは5173です。配布アーカイブに開発用Vite画面は含まれません。

## hubとgateway

```sh
omochi-ui hub
omochi-ui gateway --remote --pair --name workstation
```

Hubは既定で`127.0.0.1:5555`にバインドします。ポート変更は`--port`、Origin許可の追加は`--allow-origin`で行います。外部公開が必要でなければ既定のループバックを維持してください。Gatewayのリモート待受は明示的な`--remote`が必要です。

ブラウザ用ダッシュボードへの招待には、`gateway --remote --pair`を使います。Web URLの既定値は`https://app.omochi.fyi/`です。`--web-url`には、認証情報・クエリ・フラグメントを含まない2048文字以下の絶対HTTPS URLを指定します。`--pair`で新しい招待を発行したときだけ、ネイティブ用招待とブラウザ用リンクを表示します。独自のホストを使う場合は`--web-url`で上書きできます。

両方の招待には同じ一時的な秘密が含まれるため、非公開の経路で共有してください。登録したブラウザは、そのGatewayが公開するすべてのサービス・アクション・リソースにアクセスできます。ブラウザ側の接続記録を削除してもGateway側の信頼は失効しません。取り消すにはGatewayで`trust forget <browser-endpoint-id>`を実行します。

ネイティブHubのダッシュボードを共有するには、`hub --remote`で起動し、ローカル画面の **Share dashboard** から招待または読み取り専用リンクを作成します。Web URLの既定値は`https://app.omochi.fyi/`で、`--web-url`で上書きできます。`--web-url`を指定する場合は`--remote`が必要です。`--remote`は`--offline`とは併用できません。受信者のブラウザはこのHubへ接続し、Gatewayとは直接ペアリングしません。

release版Hubは既定で`https://relay.omochi.fyi`、debug版はn0リレーを使います。ブラウザは共有リンクに含まれるリレーへ接続し、本番リレーの正確なURLにだけ共通の入場トークンを付けます。ダッシュボードへのアクセス権限はHubが別途確認します。

## pair・status・trust

[マシン間接続](../../guides/remote/)に登録・確認・失効の手順があります。`--offline`で公開検索とリレーを止め、`--relay`で専用リレーを選びます。`status`は接続試行と診断を表示します。

## カタログ生成

```sh
omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

出力言語は一部だけ指定できます。`--check`は既存生成物が古いと失敗します。生成だけではブラウザにカスタム部品の実装は追加されません。

`enum`は文字列・真偽値・数値・nullを扱え、配列やunion内でもJSONの型を保ちます。空のenumや、オブジェクト・配列を値に持つenumは出力前にエラーになります。TypeScriptはリテラルのunion、Pythonは`True`・`False`・`None`を使った`Literal`を生成し、小数の値は`float`に広げます。Rustは文字列プロパティの名前付きenumを維持し、同じ型の非文字列enumには`bool`・`serde_json::Number`・`()`、型が混在するenumには`Value`を使います。ネストした文字列enumは`String`になります。広げた型はJSONの値を保ちますが、宣言されたenumの値だけに制限するものではありません。Rustの文字列variant名が変換後に重複する場合は数値の接尾辞を付けます。

Rustのフィールドはsnake caseになりますが、JSON出力ではカタログの元のプロパティ名を略語やアンダースコアも含めて維持します。Rustの予約語には`r#type`のようなraw identifierを使い、`self`・`super`・`crate`には末尾に`_`を付けます。Rustの識別子に変換できない名前や、変換後に同じフィールド名になるプロパティがある場合は、どの出力も書き込まずにエラーになります。

## 正確なオプションを調べる

```sh
omochi-ui --help
omochi-ui dev --help
omochi-ui help hub
```

[設定と保存先](../configuration/)と[トラブル対応](../../understand/troubleshooting/)も参照してください。
