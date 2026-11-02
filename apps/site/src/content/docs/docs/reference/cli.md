---
title: "CLIリファレンス"
description: "CLIリファレンス · omochi"
sidebar:
  order: 2
---

すべてのコマンドはネイティブの`omochi-ui`で実行します。ソースから使う場合は、ルートで`bun run omochi-ui`を先頭に付けます。

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

## pair・status・trust

[マシン間接続](../../guides/remote/)に登録・確認・失効の手順があります。`--offline`で公開検索とリレーを止め、`--relay`で専用リレーを選びます。`status`は接続試行と診断を表示します。

## カタログ生成

```sh
omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

出力言語は一部だけ指定できます。`--check`は既存生成物が古いと失敗します。生成だけではブラウザにカスタム部品の実装は追加されません。

## 正確なオプションを調べる

```sh
omochi-ui --help
omochi-ui dev --help
omochi-ui help hub
```

[設定と保存先](../configuration/)と[トラブル対応](../../understand/troubleshooting/)も参照してください。
