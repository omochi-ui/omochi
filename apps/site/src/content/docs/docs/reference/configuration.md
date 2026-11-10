---
title: "設定と保存先"
description: "設定と保存先 · omochi"
sidebar:
  order: 5
---

## ポートと保存先

| 設定 | 既定値 |
| --- | --- |
| Hub HTTP/WebSocket | `127.0.0.1:5555` |
| 標準入出力の開発ブリッジ | ポート`5555` |
| ソースから起動するViteプレビュー | ポート`5173` |
| 実行時ディレクトリ | `$XDG_RUNTIME_DIR/omochi/v2` |
| ネイティブのID・信頼・ダッシュボード | `$XDG_CONFIG_HOME/omochi/rust-v1`、未設定なら`$HOME/.config/omochi/rust-v1` |

`--config-dir`か`OMOCHI_CONFIG_DIR`でnative状態の保存先を変えます。`--runtime-dir`か`OMOCHI_RUNTIME_DIR`は実行時ディレクトリ全体を指定します。SDK Transportの明示オプションは同じ環境変数より優先します。XDG未設定のUnixでは`<システム一時ディレクトリ>/omochi-<uid>/omochi/v2`を使います。Windowsの既定はLocalAppData KnownFolderの`Omochi\Runtime\v2`、native状態は`Omochi\rust-v1`です。

SDKアプリ・Gateway・Hubは同じディレクトリを使ってください。`up`は子プロセスへ`OMOCHI_RUNTIME_DIR`を渡し、パス末尾の制約はありません。`OMOCHI_SERVICE`は`up`が渡すサービスIDで、同梱exampleではローカル検出の選択に使います。

Unixでは自分のユーザー所有・モード0700を維持し、Windowsでは現在のユーザーだけを許可する保護ACLを使います。共有書き込み可能な場所は拒否します。ダッシュボードのファイルは配置を保存するもので、アプリの実行状態は含みません。v2への更新手順は[Transport](../transports/)を参照してください。

## ネットワーク

| オプション | 動作 |
| --- | --- |
| `hub --remote` | リモートのオーナー・共有クライアントを有効にする |
| `gateway --remote` | 登録済みのリモートHubからアプリへのアクセスを有効にする |
| `--web-url HTTPS_BASE` | ホストされたクライアントのベースURL。既定は`https://app.omochi.fyi/`。`--remote`が必要 |
| `--relay URL` | 既定のリレー一覧を置き換える。複数指定可 |
| `--offline` | 公開検索・リレーを停止し、直接接続とLAN mDNSを維持する |
| `--allow-origin ORIGIN` | Hub HTTP/WebSocketに完全一致するOriginを追加する |

配布するreleaseビルドは`https://relay.omochi.fyi`、ソースのdebugビルドはn0の公開リレーを使います。Gateway・ペアリング・Hubのリレー設定を揃えてください。リモート待受を有効にしたHubは`--offline`を使えません。

`--web-url`は認証情報・クエリ・フラグメントを含まない2048文字以下の絶対HTTPS URLです。独自のクライアントをホストする場合に指定します。

HTTP/WebSocketの待受に認証・TLSはありません。ループバックを維持してください。リモートクライアントのためにHTTPポートを外部公開する必要はありません。権限の管理は[接続ガイド](../../guides/remote/)を参照してください。

## ブラウザーとアプリの保存

オーナー接続はブラウザーに保存します。ブラウザーIDを使えるタブは同時にひとつです。保存データを削除した場合は再登録が必要です。共有リンクはそのタブの再読み込み後も使えますが、タブを閉じた後や保存機能が使えない場合は元のリンクを開き直してください。

モバイル・デスクトップアプリはOSの保護ストレージに接続を保存します。共有リンクを保存するとアクセス用の情報も保存し、一時閲覧では保存しません。ブラウザーやアプリで接続を削除しても、Hubのアクセス権は取り消されません。
