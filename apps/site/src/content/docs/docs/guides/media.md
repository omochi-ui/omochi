---
title: "ローカルメディアを表示する"
description: "ローカルメディアを表示する · omochi"
sidebar:
  order: 6
---

ファイルまたはバイト列の不変コピーを登録し、URIをメディア部品へ渡します。

```python
photo = app.resources.file("photo.png")
# Surface内:
ui.image(photo)
```

TypeScriptは`app.resources.file(path, mimeType?)`・`app.resources.bytes(data, mimeType)`、Rustは`app.resources.file(path, Option<&str>)?`・`app.resources.bytes(&data, mime)?`です。いずれも`omochi://resources/<id>`を返します。

ブラウザがアプリのファイルシステムを読める必要はありません。開発ブリッジとローカル・リモートHubに対応します。登録は上限付きの同期I/Oなので、大きなファイルは起動前に登録するか別スレッドで扱います。

再生中を含め、Surfaceが参照する間は登録を維持してください。`release(uri)`は以後の読み取りを止めます。内容が変わったら再登録が必要です。既定の上限は1ファイル1 GiB、合計4 GiB。ファイルはMIME型を推定し、バイト列は明示が必要です。コーデック変換はしません。

[Python・TypeScriptの例](https://github.com/omochi-ui/omochi/tree/main/examples/media)とRust SDKのmedia exampleを参照できます。独自Transportはリソースフレームを保持し、要求元接続へ応答する必要があります。[通信仕様](https://github.com/omochi-ui/omochi/blob/main/spec/resources.md)も確認してください。
