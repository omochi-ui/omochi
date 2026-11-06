---
title: "CLIリファレンス"
description: "CLIリファレンス · omochi"
sidebar:
  order: 2
---

例では[インストール済み](../../start/installation/)の`omochi-ui`を使います。`npx @omochi-ui/cli COMMAND`でも実行できます。

| コマンド | 用途 |
| --- | --- |
| `up [FILE]` | プロジェクトのサービスとダッシュボードのプリセットを起動 |
| `dev -- COMMAND [ARGS...]` | 標準入出力のアプリをレンダラーへ中継 |
| `hub` | ダッシュボードの表示・管理 |
| `gateway` | このマシンで実行中のアプリをHubへ公開 |
| `pair --name NAME` | Gatewayの招待を登録 |
| `status` | Gatewayの接続状態を確認 |
| `id [--hub]` | GatewayまたはHubのIDを表示 |
| `trust list` | 登録済みの相手を一覧 |
| `trust forget NAME_OR_ENDPOINT_ID` | 相手の権限を取り消す |
| `generate catalog` | 型付きカタログのバインディングを生成 |

`--help`・`-h`・`help COMMAND`はサービスを起動せず使い方を表示します。不正な引数では終了コード2を返します。

## up

TOMLに宣言したサービス・Hub・ダッシュボードのプリセットを起動します。

```sh
omochi-ui up --open
omochi-ui up --port 5557 ./omochi.toml
```

設定ファイルの既定は`./omochi.toml`です。コマンドはシェルを介さず、環境変数を引き継いで実行します。相対的な作業ディレクトリは設定ファイルが基準です。子サービスには`OMOCHI_SERVICE`を渡すので、アプリ側で検出用Transportを選ぶ処理が必要です。utilityの省略記法は`button`・`watch`・`process`・`tail`・`http`に対応します。標準入力を使う`stream`、エージェントが起動する`mcp`は別途実行してください。

```toml
version = 1
id = "local-development"

[services.health]
utility = ["http", "http://127.0.0.1:3000/health"]

[services.app]
command = ["python", "app.py"]
cwd = "./backend"

[dashboard]
name = "Local development"

[[dashboard.rows]]
panels = ["health", "app"]
height = "medium"

[dashboard.panels.health]
title = "Health"
service = "health"

[dashboard.panels.app]
title = "Application"
service = "app"
```

行は12列を均等に分割します。高さは`small`・`medium`・`large`、Surfaceの既定は`main`です。プリセットは読み取り専用で、**Save as dashboard**で編集可能なコピーを保存します。ひとつのサービスが終了しても他は継続します。Ctrl+Cで`up`が起動したサービスとGatewayを停止し、先に動いていたプロセスは継続します。

| オプション | 意味 | 既定値 |
| --- | --- | --- |
| `--host HOST` | Hubの待受アドレス | `127.0.0.1` |
| `--port PORT` | Hubのポート。`0`で空きポートを選択 | `5555` |
| `--allow-origin ORIGIN` | 完全一致するブラウザーOriginを追加。複数指定可 | ループバックOrigin |
| `--open` | ブラウザーでプリセットを開く | 無効 |

## dev

`omochi-ui dev -- COMMAND [ARGS...]`はアプリを起動し、標準入出力のJSONLをWebSocketへ中継します。表示用レンダラーは別途必要です。ソースからViteを使う場合は[標準入出力のプレビュー手順](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md#preview-a-stdio-application)を参照してください。

`--`以降は`--help`も含めアプリへ渡します。ログは標準エラーへ出してください。`--host`の既定は`127.0.0.1`、`--port`は`5555`で、`0`なら空きポートを選べます。表示された`?ws=` URLをレンダラーで使います。ブリッジに認証・TLSはないのでループバックを維持してください。

## hub

```sh
omochi-ui hub
```

**http://127.0.0.1:5555/** を開き、実行中の[SDKアプリ](../../guides/sdks/)か[utils](../../guides/utils/)を追加します。画面は同梱され、必要なローカルGatewayも自動起動します。既存のGatewayは再利用しますが、そのプロセスの管理は引き継ぎません。

| オプション | 意味 | 既定値 |
| --- | --- | --- |
| `--host HOST` | HTTP/WebSocketの待受アドレス | `127.0.0.1` |
| `--port PORT` | HTTP/WebSocketのポート。`0`で空きポートを選択 | `5555` |
| `--no-auto-gateway` | ローカルGatewayがない場合も自動起動しない | 自動起動あり |
| `--allow-origin ORIGIN` | 完全一致するOriginを追加。複数指定可 | ループバックOrigin |
| `--remote` | リモートのオーナー・共有クライアントを受け付ける。`--offline`とは併用不可 | 無効 |
| `--web-url HTTPS_BASE` | ホストされたクライアントのURL。`--remote`が必要 | `https://app.omochi.fyi/` |
| `--offline` | 公開検索・リレーを停止し、直接接続とLAN mDNSを維持 | 無効 |
| `--relay URL` | リレーURLを置き換え。複数指定可 | [設定](../configuration/)を参照 |
| `--assets DIR` | ダッシュボードのファイルを置き換え | 同梱画面 |

登録済みGatewayにはバックグラウンドで個別に再接続します。全体の管理には[オーナー接続](../../guides/remote/)、ボードひとつへのアクセスには[ダッシュボード共有](../../guides/dashboards/#保存したダッシュボードを共有する)を使います。HTTP/WebSocketはループバックを維持し、リモートクライアントのためにポートを外部公開する必要はありません。

## gateway

```sh
omochi-ui gateway
omochi-ui gateway --remote --pair --name workstation
```

同じユーザーで実行中のアプリを公開します。アプリは別途起動し、表示にはHubを使います。

| オプション | 意味 | 既定値 |
| --- | --- | --- |
| `--remote` | 登録済みのリモートHubを受け付ける | 無効 |
| `--pair` | 3分間有効な招待を発行。`--remote`が必要 | 無効 |
| `--web-url HTTPS_BASE` | 招待用クライアントのURL。`--remote`が必要 | `https://app.omochi.fyi/` |
| `--name NAME` | 表示名 | `gateway` |
| `--offline` | 公開検索・リレーを使わず直接接続とLAN mDNSを利用 | 無効 |
| `--relay URL` | リレーURLを置き換え。複数指定可 | [設定](../configuration/)を参照 |

`--pair`はネイティブ用・HTTPSの招待リンクとQRを表示します。どちらのリンクも`pair`で登録できます。非公開の経路で共有してください。ブラウザーとアプリからの操作は[接続ガイド](../../guides/remote/)を参照してください。

## pair

Hubのあるマシンで実行します。

```sh
omochi-ui pair --name workstation
# 招待を標準入力へ貼り付け、Ctrl+D。
omochi-ui hub
```

表示されたネイティブ用・HTTPSの招待を標準入力か位置引数で渡せます。標準入力ならシェル履歴に残りません。招待は3分間有効で、ひとつのHubを登録します。同じIDなら中断した登録を再試行できます。

`--name`はローカルでのGatewayの別名、`--hub-name`の既定は`hub`です。`--offline`はLAN内だけの登録、`--relay URL`は独自リレーを選びます。`gateway`・`pair`・`hub`のネットワーク設定を合わせてください。

## status

`omochi-ui status`は登録済みGatewayのEndpointId・別名・接続状態・最後のエラーをJSONで表示します。未接続の相手も再接続中は一覧に残ります。既定では`127.0.0.1:5555`の稼働中Hubへ問い合わせ、`--host`・`--port`で変更できます。空の配列なら、追跡中のリモート接続試行はありません。

## idとtrust

```sh
omochi-ui id
omochi-ui id --hub
omochi-ui trust list
omochi-ui trust forget workstation
```

`id`は保存済みのGateway ID、`--hub`はHub IDを表示します。IDは64文字の小文字16進数のEndpointIdです。名前はローカルな別名です。

`trust forget`にはEndpointIdか一意な名前を渡します。登録を削除し、稼働中のローカルプロセスが該当接続を閉じるまで待ちます。削除が保存済みでも確認に失敗すればエラーになるので、[トラブルシューティング](../../understand/troubleshooting/#アクセス権の取り消しがエラーになる)を参照してください。削除した相手を再登録する場合はペアリングし直します。

## カタログ生成

```sh
omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

出力先ディレクトリを先に作り、必要な言語を指定します。`--check`は書き込まず、生成物が存在しないか古ければ失敗します。全オプションとスキーマ対応は[カタログ生成](../catalog-generation/)を参照してください。

保存先は[設定](../configuration/)、よくあるエラーは[トラブルシューティング](../../understand/troubleshooting/)を参照してください。
