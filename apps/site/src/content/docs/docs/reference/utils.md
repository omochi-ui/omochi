---
title: "utilsリファレンス"
description: "utilsリファレンス · omochi"
sidebar:
  order: 3
---

例では[インストール済み](../../start/installation/)の`omochi-utils`を使います。別途`omochi-ui hub`を起動し、**Add view**からツールのSurfaceを追加してください。

## コマンドをボタンにする

```sh
omochi-utils button --service-id build --name "Build" -- bun run build
# 別のターミナル:
omochi-ui hub
```

Hubで`Build · main`を追加します。ボタンを押すとコマンドを実行し、標準出力・標準エラー、実行状態、終了コードを表示します。複数のブラウザーから使っても同時実行はひとつです。完了後や起動失敗後も再実行できます。

```sh
omochi-utils button --name "Tests" --cwd /path/to/project -- bun run test
omochi-utils button --name "Pipeline" -- sh -c 'printf hello | wc -c'
```

`--`以降はシェルを介さず、そのまま引数として渡します。環境変数と作業ディレクトリを引き継ぎ、`--cwd`で変更できます。標準入力は閉じます。出力は最新64 KiBを保持し、超過を表示します。実行ごとにリセットし、Markdownとして解釈せずコードとして表示します。Ctrl+CかSIGTERMで子プロセスグループを停止し、1秒以内にTERMからKILLへ進み、サービスを取り下げます。

## コマンドを定期実行する

```sh
omochi-utils watch --interval 2s -- git status --short
omochi-utils watch --diff --on-change --history 10 -- bun run test
omochi-utils watch --timeout 30s --cwd /path/to/project -- command args
```

`watch`はすぐ実行し、コマンド完了後に指定間隔だけ待って再実行します。実行は重なりません。画面から一時停止・即時再開・1回だけの実行ができます。`--timeout`は応答しないプロセスグループを停止します。`--diff`は現在の出力と前回からの行差分、`--on-change`は出力と終了結果が同じ場合の完了更新を抑制します。`--history N`は1〜100件の短い実行記録を残し、過去の全出力は保持しません。

## 長く動くプロセスを操作する

```sh
omochi-utils process --name "Dev server" -- bun run dev
omochi-utils process --restart on-failure --restart-delay 2s -- worker
```

**Start**で起動し、**Stop**・**Restart**で操作します。最新64 KiBのログを再起動後も保持し、終了理由を記録します。`--restart`は`never`（既定）・`on-failure`・`always`です。手動停止やCLI終了では自動再起動しません。

## ログファイルと標準入力を追う

```sh
omochi-utils tail --lines 200 ./logs/api.log
journalctl -f | omochi-utils stream --name "System log"
```

`tail`はファイルがなければ待ち、切り詰めやローテーションにも`tail -F`のように名前を追跡します。UTF-8として読み、書き込みません。`stream`は標準入力をUTF-8で表示します。EOF後も最終表示を残し、SIGINTかSIGTERMまで待ちます。

## HTTPを監視する

```sh
omochi-utils http http://127.0.0.1:3000/health
omochi-utils http --interval 30s --timeout 5s \
  --expect-status 200 --expect-body '"ready":true' https://example.test/health
```

重ならないGET要求を送り、`watch`と同じ一時停止・再開・即時実行を提供します。ステータス・応答時間・最終URL・Content-Type・サイズ・連続失敗数・正常性の変化を表示します。正常ステータスの既定は200〜299です。テキストは64 KiBまで表示し、JSONは整形します。バイナリー本文は表示しません。独自メソッド・ヘッダー・認証・リクエスト本文が必要なら`watch -- curl ...`を使います。

## MCP

エージェントに標準入出力のMCPサーバーを設定します。PATH上の`omochi-utils`か、インストール先の絶対パスを指定してください。

```json
{
  "mcpServers": {
    "omochi": {
      "command": "omochi-utils",
      "args": ["mcp", "--name", "Agent"]
    }
  }
}
```

ローカル検出ではエージェント・Gateway・Hubを同じOSユーザー・実行時ディレクトリで動かします。導入は[MCPガイド](../../guides/agents/)を参照してください。

| ツール | 引数 | 結果 |
| --- | --- | --- |
| `publish_messages` | `messages`: 空でないA2UI v0.9メッセージ配列 | `accepted`・`serviceId`・`surfaceIds` |
| `list_surfaces` | なし | `serviceId`・`surfaceIds` |
| `read_events` | `after`（既定0）、`timeoutMs`（既定0、最大30000） | `events`・`nextCursor`・`dropped`・`closed` |

`publish_messages`は`createSurface`・`updateComponents`・`updateDataModel`・`deleteSurface`を受け付けます。バッチ全体を現在の状態に対して検証してから送信し、エラーなら配列内の位置を返して公開状態を維持します。同時呼び出しは直列化します。IDは各Surface内で一意にしてください。通信形式とSurfaceのライフサイクルを検証し、カタログ固有の部品検証はレンダラーが行います。

次を`publish_messages`へ渡すと操作可能なフォームを公開できます。

```json
{
  "messages": [
    {
      "version": "v0.9",
      "createSurface": {
        "surfaceId": "main",
        "catalogId": "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json",
        "sendDataModel": true
      }
    },
    {
      "version": "v0.9",
      "updateComponents": {
        "surfaceId": "main",
        "components": [
          { "id": "root", "component": "Column", "children": ["title", "request", "submit"] },
          { "id": "title", "component": "Text", "text": "What should we build?" },
          { "id": "request", "component": "TextField", "label": "Request", "value": { "path": "/request" } },
          { "id": "submit", "component": "Button", "child": "label", "action": { "event": { "name": "submit", "context": { "request": { "path": "/request" } } } } },
          { "id": "label", "component": "Text", "text": "Send" }
        ]
      }
    },
    { "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/", "value": { "request": "" } } }
  ]
}
```

`read_events`へ`{ "after": 0, "timeoutMs": 30000 }`を渡します。イベントには`sequence`・`receivedAt`（ISO時刻）・元のA2UIクライアント`message`が含まれ、`action.context`とクライアントのデータモデル情報を読めます。この例の送信テキストは`message.action.context.request`です。続けて次のバッチで表示を更新できます。

```json
{
  "messages": [{
    "version": "v0.9",
    "updateComponents": {
      "surfaceId": "main",
      "components": [{ "id": "title", "component": "Text", "text": "Working…" }]
    }
  }]
}
```

次の読み取りでは`nextCursor`を`after`へ渡します。読み取りでイベントを消費しないので、同じカーソルで再試行できます。最新1000件を保持し、期限切れになった件数を`dropped`で返します。タイムアウトは空配列です。待機中の読み取りはMCPのキャンセルと終了通知に対応します。ボタンだけでは待機中のエージェントは起動せず、`read_events`が必要です。これらのツール自身はシェルコマンドやコールバックを実行しません。

取り下げは`{"version":"v0.9","deleteSurface":{"surfaceId":"main"}}`を公開します。永続保存はせず、標準入力のEOF・SIGINT・SIGTERMでサーバーを終了してサービスを取り下げます。後から接続したGatewayには現在のSurfaceを送ります。

## 共通オプションとライフタイム

全サブコマンドで`--service-id ID`と`--name NAME`を使えます。IDの既定は一意な`utils-<uuid>`で、配置を保存する場合は固定IDを指定してください。同じIDで稼働中のプロセスがあれば、置き換えず起動に失敗します。時間は`ms`・`s`・`m`の接尾辞に対応します。各プロセスは独立したSurfaceとイベントバッファーを持ちます。サービスを公開した後、Hubの起動とビューの追加を行ってください。

全体か各サブコマンドの`--help`で使い方を確認できます。実行時ディレクトリは自分のユーザー所有・非公開にしてください。[設定](../configuration/)も参照してください。
