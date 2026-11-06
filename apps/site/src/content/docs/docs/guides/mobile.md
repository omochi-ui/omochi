---
title: "モバイル・デスクトップアプリ"
description: "モバイル・デスクトップアプリ · omochi"
sidebar:
  order: 7
---

OmochiアプリはAndroid・iOS・macOS・Windows・LinuxでSurfaceを表示・操作します。プラットフォーム別のビルド・入手方法と対応状況は[アプリのREADME](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.ja.md)を参照してください。

## Gatewayへ接続する

アプリのあるコンピューターで`omochi-ui gateway --remote --pair`を実行します。表示されたHTTPS招待ページを端末で開き、**Open in Omochi app**を選びます。アプリ内で接続先と期限を確認し、**Add to this device**で登録します。Connectionsへ招待を貼り付けることもできます。

AppsからサービスのSurfaceを選びます。アプリの内蔵Hubが接続するため、別のHubは不要です。登録先のGatewayと元のアプリは動かしておいてください。

## 外部Hubへ接続する

ツールバーのHub接続から、[オーナー招待](../remote/#ホストされたブラウザーから接続する)または[共有リンク](../dashboards/#保存したダッシュボードを共有する)を開くか貼り付けます。表示する接続はひとつで、外部Hubの利用中は内蔵Hubを休止します。

オーナー招待はアプリ用に新しく発行してください。ブラウザーで使った招待は再利用できません。共有リンクでは、リンクで許可された操作・レイアウト編集ができます。

共有リンクは一時的に開くか、端末へ保存できます。保存すると、次回もそのリンクの権限で接続できます。接続を削除してもHubの権限は取り消されないので、失効させるには接続先Hubで取り消してください。

## 接続とメディアの制約

HTTPSリンクからのアプリ直接起動は現在利用できません。招待の貼り付けか`omochi://`リンクによる受け渡しを使います。

内蔵Hubの上限は2 Gateway・8サービスです。外部Hubへの接続では公開画像を表示できますが、リモートの音声・動画は未対応です。LinuxのVideo再生も未対応です。対応範囲の詳細は[アプリのREADME](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.ja.md)を参照してください。
