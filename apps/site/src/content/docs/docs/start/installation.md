---
title: "インストール"
description: "インストール · omochi"
sidebar:
  order: 1
---

## 現在のソースからビルド

このガイドではソースチェックアウトを再現可能な導入方法として使います。LinuxまたはmacOS、Bun 1.4.3以上、最新のstable Rust/Cargoを用意してください。Pythonの例にはPython 3.11以上とuvが必要です。

```sh
git clone https://github.com/omochi-ui/omochi.git
cd omochi
bun install --frozen-lockfile
bun run build
```

SDK・ブラウザ側パッケージとネイティブCLIをビルドします。この手順にNode.jsは不要です。Unixソケットによる検出にはUnix系環境が必要で、ネイティブサービスは現在Windowsに対応していません。

## ネイティブ配布アーカイブ

[GitHub Releases](https://github.com/omochi-ui/omochi/releases)からOS・CPUに合うアーカイブと`.sha256`を取得し、チェックサムを確認して展開します。

```sh
./install.sh  # 既定の配置先は $HOME/.local/bin
omochi-ui hub
```

アーカイブには`omochi-ui`と`omochi-utils`、ビルド済みダッシュボードが含まれます。バイナリの実行にBun・Node.js・Python・Rustは不要ですが、起動するアプリには各言語の実行環境が必要です。Linux版はglibc向けで、macOS版は署名・公証されていません。

## レジストリ配布

npm・Cargo向けのネイティブ配布処理は実装されていますが、公開済みバージョンはリリースノートとレジストリで確認してください。ソース内の`packages/npm`は非公開の起動スタブで、配布物として直接公開・導入するものではありません。npm版の起動にはNode.js 22以上が必要です。

次は[最初のUI](../first-ui/)を表示します。
