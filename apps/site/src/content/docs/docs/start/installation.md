---
title: "インストール"
description: "インストール · omochi"
sidebar:
  order: 1
---

## Hubを起動する

SDKやutilsで作ったUIを表示するには、アプリと同じPCでHubを起動します。LinuxまたはmacOSとNode.js 22以上があれば、任意のディレクトリで実行できます。

```sh
npx @omochi-ui/cli hub
```

**http://127.0.0.1:5555/** を開きます。画面はCLIに同梱され、ローカルGatewayも自動起動します。Hubを使う間はターミナルを動かしておきます。

同じPCのアプリを使うだけなら、Gatewayを別に起動する必要はありません。別のPCのアプリを接続するときは[リモート接続](../../guides/remote/)を参照してください。

## インストールして使う

繰り返し使う場合はCLIをインストールできます。

```sh
npm install -g @omochi-ui/cli
omochi-ui hub
```

Rust環境では、最新のstable Rust/Cargoでインストールできます。

```sh
cargo install omochi-cli --locked
omochi-ui hub
```

Cargoパッケージ名は`omochi-cli`、実行コマンドは`omochi-ui`です。どちらの配布方法でもHub・Gateway・ダッシュボードが含まれます。SDKの`omochi-ui`クレートとは別です。

SDKやutilsをインストールしてもHubは起動しません。SDKアプリはその言語のSDKを使い、Hubは上のコマンドで起動します。utilsは別ターミナルで`npx @omochi-ui/utils ...`として使えます。常設する場合は`npm install -g @omochi-ui/utils`、または`cargo install omochi-utils --locked`でインストールします。

npm版はビルド済みバイナリを取得するのでRustは不要です。optional dependenciesを有効にしてください。対応環境はLinux（glibc）・macOSのx64/arm64で、ネイティブサービスはWindows未対応です。利用できるオプションは`hub --help`で確認してください。

次は[最初のUI](../first-ui/)でSurfaceを表示します。

## ネイティブ配布アーカイブ

[GitHub Releases](https://github.com/omochi-ui/omochi/releases)からOS・CPUに合うアーカイブと`.sha256`を取得し、チェックサムを確認して展開します。

```sh
./install.sh  # 既定の配置先は $HOME/.local/bin
omochi-ui hub
```

アーカイブには`omochi-ui`と`omochi-utils`、ビルド済みダッシュボードが含まれます。バイナリの実行にBun・Node.js・Python・Rustは不要ですが、起動するアプリには各言語の実行環境が必要です。Linux版はglibc向けで、macOS版は署名・公証されていません。

Omochi本体の開発や同梱サンプルの実行は[Contributing](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md)を参照してください。
