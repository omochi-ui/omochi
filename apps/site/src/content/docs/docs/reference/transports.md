---
title: "Transport"
description: "Transport · omochi"
sidebar:
  order: 6
---

SDKアプリとクライアントの接続方法を選びます。

| Transport | 用途 | 対応 |
| --- | --- | --- |
| `LocalServerTransport` | Hubが検出できるアプリを公開 | Python・Node.js/Bun・Rust |
| `StdioTransport` | ブリッジや独自クライアントへ標準入出力のJSONLを渡す | 全SDK。`app.start()`の既定 |
| `WebSocketTransport` | 呼び出し元が持つWebSocket相当の接続を使う | Python・TypeScript |
| `WebSocketServerTransport` | ブラウザーを直接接続するサーバー | Node.js/Bun |
| `CallbackTransport` | 別のメッセージ経路に組み込む | 全SDK |

Pythonは`omochi_ui`、TypeScriptのサーバーTransportは`@omochi-ui/sdk/node`、その他は`@omochi-ui/sdk`から読み込みます。Rustは`omochi_ui`から`Transport` traitと各アダプターを読み込みます。

## ローカルサービスの検出

実行可能な例は[SDKガイド](../../guides/sdks/)を参照してください。クライアントとサービスは同じOSユーザー・実行時ディレクトリを使います。全SDKが共通のnativeアダプターを使い、Linux/macOSではUnixドメインソケット、Windows 11ではnamed pipeで接続します。アクセスは現在のユーザーだけに制限します。

`OMOCHI_RUNTIME_DIR`を揃えるか、各Transportの実行時ディレクトリを指定してください。既定はUnixでは`$XDG_RUNTIME_DIR/omochi/v2`、XDG未設定なら`<システム一時ディレクトリ>/omochi-<uid>/omochi/v2`、WindowsではLocalAppData KnownFolderの`Omochi\Runtime\v2`です。

サービスIDは`[a-z0-9._-]{1,64}`に一致し、`.`・`..`以外にします。大文字は拒否します。同じIDでの同時公開は拒否されます。保存した配置を再利用する場合はIDを固定してください。保存先は符号化せず`svc-<id>~`とします。

Node.js/Bunではoptional dependenciesを有効にしてください。Pythonのplatform wheelにはアダプターを同梱し、sdistからのビルドにはCargoが必要です。配布済みアダプターはLinux glibc/musl・macOS・Windowsの各x64/ARM64に対応します。

v2への更新ではpublisher・Gateway・Hubをまとめて停止してください。`LocalServerTransport`と以下のオプション名へ変更し、旧runtimeを明示していた場合は新しい空ディレクトリを指定します。ソケットパスの取得は型付き`endpoint`へ移行してください。native identityとtrustは維持します。manifestと所有権の詳細は[実行時の仕様](https://github.com/omochi-ui/omochi/blob/main/spec/local-service-ownership.md)を参照してください。

| SDK | 実行時ディレクトリの指定 | capabilityの指定 |
| --- | --- | --- |
| Python | `LocalServerTransport`の`runtime_dir=` | `capabilities=` |
| TypeScript | Transportオプションの`runtimeDir` | `capabilities` |
| Rust | `LocalServerOptions::with_runtime_dir(...)` | `.with_capabilities(...)` |

## 標準入出力と独自クライアント

Python・TypeScriptの`await app.start()`、Rustの`app.start().await`は標準入出力を使います。標準出力はJSONL用なので、ログは標準エラーへ出します。表示にはブリッジかクライアントが必要です。ソースからViteプレビューを使う手順は[Contributing](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md#preview-a-stdio-application)を参照してください。

Transportを明示する場合はPython・TypeScriptで`await app.start(transport)`、Rustで`app.start_with(transport).await`を使います。Transportはひとつのアプリを扱います。レンダラーのcapabilityは最初のSurfaceを描画する前に設定し、クライアントが実装しているものだけを有効にしてください。

Pythonの標準入出力とローカルTransportは、JSONLの1行につきUTF-8で4 MiBまで受け付けます。LFは上限に含まず、直前のCRは含みます。超過すると標準入出力の受信は`ValueError`で終了し、ローカルTransportでは該当接続だけを閉じます。LFのない最終行も上限内なら受け付けます。

## 直接WebSocketサーバー

Node.js/Bunでは標準入出力ブリッジを使わず、ブラウザーを直接接続できます。

```ts
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const transport = new WebSocketServerTransport({ port: 5555 });
await app.start(transport);
```

既定の待受は`127.0.0.1`の`/ws`です。接続する全クライアントでアプリ状態を共有し、後から参加したクライアントには現在のスナップショットを送ります。`host`・`path`で待受を変え、`port: 0`で空きポートを選べます。`transport.url`を読む前に`await transport.ready`を待ち、終了には`await transport.close()`を使います。認証・TLSはないので、通常はループバックを維持してください。

WebSocketとHTTPリソース要求には同じOrigin検証を適用します。`localhost`・`127.0.0.1`・`[::1]`のHTTP(S) Originだけを許可し、ポートは任意です。外部・不正・空文字・`null`のOriginは接続前にHTTP 403で拒否します。ブラウザー以外のクライアントはOriginを省略できますが、`Sec-Fetch-Site: cross-site`はOriginの有無にかかわらず拒否します。`host`を変えてもこの制約は変わりません。外部ブラウザーにはHubのOrigin設定を使ってください。

## 終了処理

Python・TypeScriptの`runtime.shutdown()`はTransportからの新しいメッセージの受け付けとバックグラウンド処理を停止し、ResourceRegistryを閉じます。組み込みのサーバーTransportはリスナーも閉じ、ローカルサービスの公開情報を削除します。後片付けの完了はPythonの`runtime.wait_closed()`、TypeScriptの`runtime.waitClosed()`で待てます。すでに実行中のActionが完了してから`start()`が戻る場合があります。

TypeScriptの`shutdown()`は初回も停止を要求した時点で解決済みのPromiseを返すため、`await runtime.shutdown()`だけでは後片付けの完了を待ちません。`spawn(async signal => { ... })`と`spawn(promise)`のどちらで登録したタスクも、停止を要求してから自分の後片付けを続けられます。タスクの外から`await runtime.shutdown(); await runtime.waitClosed();`を呼ぶと、登録された全タスクの終了とResourceRegistryのクローズを待てます。`waitClosed()`はタスク自身の終了も待つため、登録タスク内では待たないでください。後片付けのエラーは`waitClosed()`と`start()`に伝わります。ActionやTransportが先に失敗していた場合、`start()`は先に発生したエラーを維持します。

独自TransportはPythonの`async stop_receiving()`、TypeScriptの`stopReceiving()`でRuntime終了時の後片付けに参加できます。このフックで受信を止め、Transport自身が所有するリソースを解放します。Callback・WebSocketアダプターは受信ループを止めますが、接続自体の所有者は呼び出し元です。独自Transportの`close()`をRuntimeが自動で呼ぶことはありません。


## 独自のリソース配信

独自TransportはA2UIスナップショットとリソースフレームを分けて保持し、要求元接続へ応答してください。組み込みTransportはこれに対応します。ブラウザーレンダラーの`useA2uiSession(wsUrl, resourceBaseUrl?)`はHTTPリソースのベースURLを指定でき、未指定ならWebSocketと同じオリジンを使います。

アダプターを実装する場合は[リソース通信仕様](https://github.com/omochi-ui/omochi/blob/main/spec/resources.md)を参照してください。
