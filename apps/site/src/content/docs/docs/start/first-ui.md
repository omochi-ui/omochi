---
title: "最初のUI"
description: "最初のUI · omochi"
sidebar:
  order: 2
---

## Hubを開く

LinuxまたはmacOSとNode.js 22以上を用意し、ターミナル1で実行します。別の導入方法は[インストール](../installation/)を参照してください。

```sh
npx @omochi-ui/cli hub
```

**http://127.0.0.1:5555/** を開きます。

## 時刻を表示する

ターミナル2で、現在時刻を表示するツールを起動します。

```sh
npx @omochi-ui/utils watch --interval 2s -- date
```

ブラウザーでダッシュボードを作成し、**Add view**からこのツールのSurfaceを追加して保存します。2秒ごとに時刻が更新されます。Surfaceは、アプリやツールが公開する名前付きのビューです。

利用中はHubとツールの両方を動かしておきます。終了するときは各ターミナルでCtrl+Cを押します。ダッシュボードの配置は保存されますが、ツールの実行状態は保存されません。

## 次に進む

- 自分のプログラムにUIを付ける：[SDKを使う](../../guides/sdks/)
- コマンドやログを表示する：[コマンドをツールにする](../../guides/utils/)
- ビューの配置や共有をする：[ダッシュボードを構成する](../../guides/dashboards/)
