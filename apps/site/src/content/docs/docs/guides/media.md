---
title: "ローカルメディアを表示する"
description: "ローカルメディアを表示する · omochi"
sidebar:
  order: 6
---

ファイルまたはバイト列を登録し、返されたURIをメディア部品へ渡します。

```python
photo = app.resources.file("photo.png")
clip = app.resources.bytes(video_bytes, mime_type="video/mp4")
# Surface内:
ui.image(photo)
ui.video(clip)
```

TypeScriptは`app.resources.file(path, mimeType?)`・`app.resources.bytes(data, mimeType)`、Rustは`app.resources.file(path, Option<&str>)?`・`app.resources.bytes(&data, mime)?`です。いずれも`omochi://resources/<id>`のURIを返します。ブラウザーがアプリのファイルシステムへアクセスする必要はありません。

## 登録と解放

登録すると内容の不変コピーを作ります。大きなファイルはアプリの起動前に登録するか、別スレッドで扱ってください。Surfaceが参照する間は再生中も登録を維持し、不要になったら`release(uri)`で以後の読み取りを止めます。内容が変わった場合は再登録して新しいURIを表示します。Runtimeの終了時にはレジストリも閉じます。

既定の上限は1ファイル1 GiB、合計4 GiBです。変更するにはTypeScript Appの第3引数、Pythonの`resources=`、Rustの`app.resources`へ、上限を指定した`ResourceRegistry`を渡します。ファイルは拡張子からMIME型を推定し、バイト列では指定が必要です。コーデック変換はしません。

## クライアントごとのメディア対応

ローカルHubと開発ブリッジは画像・音声・動画の配信に対応し、GET・HEAD・Range要求を扱います。ホストされたブラウザーのオーナー・共有接続では公開画像を表示でき、音声・動画は未対応です。

リモートへ画像を公開するには、`ui.image(photo)`のように完全な画像URIをSurfaceの状態に含めます。部品が後から組み立てるURLやMarkdown内の宛先だけでは公開されません。SDKの登録上限とは別に、クライアントにも配信・デコードの上限があります。

[モバイル・デスクトップアプリ](../mobile/)の対応にはプラットフォーム差があります。[メディアのサンプル](https://github.com/omochi-ui/omochi/tree/main/examples/media)と[独自Transportの配信方法](../../reference/transports/#独自のリソース配信)も参照してください。
