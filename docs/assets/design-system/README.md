# デザイン見本のアセット

[編集元の design-system.svg](../design-system.svg) は、アイコンとロゴを次のファイルから `<use href="design-system/icons/surface.svg#surface">` のように相対参照する。形状の編集は各ファイルで行い、位置・大きさ・アイコンの色は参照元で指定する。

| ファイル | 内容 | 元の形状 |
| --- | --- | --- |
| [surface.svg](icons/surface.svg) | Surface | 提供キットの `icons/surface.svg` |
| [gateway.svg](icons/gateway.svg) | Gateway | 提供キットの `icons/gateway.svg` |
| [hub.svg](icons/hub.svg) | Hub | 提供キットの `icons/hub.svg` |
| [check.svg](icons/check.svg) | 接続済み | 見本図用の状態記号 |
| [clock.svg](icons/clock.svg) | 接続中 | 見本図用の状態記号 |
| [offline.svg](icons/offline.svg) | 未接続 | 見本図用の状態記号 |
| [omochi-mark.svg](logos/omochi-mark.svg) | Omochi ロゴ | 提供キットの `logos/omochi-mark.svg` を指定パレットへ配色変更 |

各 SVG のルート要素の `id` が参照先になる。名前や `id` を変更する場合は、見本図の `href` も更新する。機能アイコンは `currentColor` を使うため、同じファイルを明るい背景と濃色ナビゲーションで共用できる。ロゴは図形に含まれる2色を自身のファイルで指定する。

## プレビュー

外部参照する SVG は、同一オリジンから文書として開く。リポジトリルートで次を実行し、[ローカルプレビュー](http://127.0.0.1:8765/design-system.svg)を開く。

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/assets
```

Markdown や HTML の `<img>` で表示する SVG では外部リソースの読み込みが制限されるため、[仕様書](../../design-system.md)には[生成した PNG](../design-system.png)を使用する。SVG のブラウザプレビューは上記の HTTP URL で直接開くか、同一オリジンの `<object>` に文書として埋め込む。[MDN の SVG 画像の制限](https://developer.mozilla.org/en-US/docs/Web/SVG/Guides/SVG_as_an_image)を参照。

## PNG の再生成

SVG 本体や個別アセットを更新したら、リポジトリルートで librsvg の `rsvg-convert` を使って PNG を再生成する。PNG は直接編集しない。

```sh
rsvg-convert docs/assets/design-system.svg -o docs/assets/design-system.png
```

アイコンの欠け、濃色面での色、文字との重なりを確認する。SVG を別の場所へ移す場合は `design-system/` 以下のアセットも相対位置を保って配置する。
