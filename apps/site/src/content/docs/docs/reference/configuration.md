---
title: "設定と保存先"
description: "設定と保存先 · omochi"
sidebar:
  order: 5
---

| 設定 | 既定値・動作 |
| --- | --- |
| Hub HTTP | `127.0.0.1:5555` |
| 開発ブリッジ | ポート`5555` |
| Viteプレビュー | ポート`5173` |
| `OMOCHI_SERVICE` | 同梱exampleが検出可能なUnixソケットを選択 |
| 実行時名前空間 | `$XDG_RUNTIME_DIR/omochi`、所有者のみアクセス可能 |
| ネイティブID・信頼 | `$XDG_CONFIG_HOME/omochi/rust-v1`、未設定なら`$HOME/.config/omochi/rust-v1` |
| リモート接続 | 明示的に有効にするまで無効 |
| `--allow-origin` | HubのHTTP・WebSocketの許可Originを追加 |
| オンラインの既定リレー | releaseは`https://relay.omochi.fyi`（共有トークン組み込み）、debugはn0 |
| `--relay` | 既定のリレー一覧を置き換え。本番リレーを明示指定した場合も自動認証 |
| `--offline` | 公開検索・リレーを停止し、直接経路とLAN mDNSを維持 |

## 所有権と安全性

実行時ディレクトリの所有者と0700のモードがローカルソケットを保護します。マニフェストは信頼せず、サービスID・ソケット・アイコンを検証して使います。共有書き込み可能な場所に実行時ディレクトリを置かないでください。

開発サーバーに認証・TLSはありません。HubはHTTPとWebSocket両方のOriginを確認します。外部公開が必要でなければループバックを維持してください。

ネイティブのIDと信頼は旧状態と別管理です。旧信頼をコピーせず再登録してください。ダッシュボードの配置保存はアプリ状態のバックアップではありません。マシンを接続する前に[リモート設定](../../guides/remote/)を確認します。

## 接続先と保存場所

`--config-dir`または`OMOCHI_CONFIG_DIR`でネイティブ状態の場所を変更できます。`--runtime-dir`または`OMOCHI_RUNTIME_ROOT`はSDKレジストリの場所を選びます。`XDG_RUNTIME_DIR`が未設定または空なら、既定の実行時ルートは`<システム一時ディレクトリ>/omochi-<uid>/omochi`です。安全でない実行時ディレクトリは拒否します。

`hub --remote`はirohのオーナー・共有接続を有効にします。`gateway --remote`は信頼済みHubからアプリへの接続を有効にします。両者の認可は別です。`--web-url HTTPS_BASE`の既定値は`https://app.omochi.fyi/`で、指定には`--remote`が必要です。Hubのリモート待受と`--offline`は併用できません。

ホストされたブラウザーのidentityと登録済み接続はIndexedDBに保存し、Web Locksでひとつのタブがidentityを所有します。公開共有の再読み込み用bearer情報はタブのsessionStorageに保持します。オーナー・登録型共有の招待secretは保存しません。FlutterはOSの保護ストレージに保存し、明示保存した公開共有ではbearer情報も保存します。詳細は[接続ガイド](../../guides/remote/)へ。
