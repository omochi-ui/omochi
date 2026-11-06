# Design sample assets

The editable [design-system.svg](../design-system.svg) references icons and logos
from the following files with relative links such as
`<use href="design-system/icons/surface.svg#surface">`. Edit shapes in the
individual files; set position, size, and icon color in the referring document.

| File | Contents | Shape source |
| --- | --- | --- |
| [surface.svg](icons/surface.svg) | Surface | Supplied kit's `icons/surface.svg` |
| [gateway.svg](icons/gateway.svg) | Gateway | Supplied kit's `icons/gateway.svg` |
| [hub.svg](icons/hub.svg) | Hub | Supplied kit's `icons/hub.svg` |
| [check.svg](icons/check.svg) | Connected | Status symbol for the sample |
| [clock.svg](icons/clock.svg) | Connecting | Status symbol for the sample |
| [offline.svg](icons/offline.svg) | Disconnected | Status symbol for the sample |
| [omochi-mark.svg](logos/omochi-mark.svg) | Omochi logo | Supplied kit's `logos/omochi-mark.svg`, recolored to the specified palette |

Each SVG's root `id` is the reference target. Update sample `href` values when
renaming files or IDs. Functional icons use `currentColor`, allowing the same
file on light backgrounds and dark navigation. The logo defines its two shape
colors in its own file.

## Preview

Open SVGs with external references as documents from the same origin. Run this
from the repository root and open the
[local preview](http://127.0.0.1:8765/design-system.svg):

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/assets
```

SVGs displayed through Markdown or HTML `<img>` restrict external resource
loading, so the [specification](../../design-system.md) uses the
[exported PNG](../design-system.png). Open the SVG directly through the HTTP URL
above or embed it as a document in a same-origin `<object>`. See
[MDN's SVG image restrictions](https://developer.mozilla.org/en-US/docs/Web/SVG/Guides/SVG_as_an_image).

## Regenerating the PNG

After editing the main SVG or an individual asset, regenerate the PNG from the
repository root with librsvg's `rsvg-convert`. Do not edit the PNG directly.

```sh
rsvg-convert docs/assets/design-system.svg -o docs/assets/design-system.png
```

Check for clipped icons, colors on dark surfaces, and text overlap. When moving
the SVG, preserve the relative positions of the assets under `design-system/`.
