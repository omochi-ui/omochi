---
title: "Components & catalogs"
description: "Components & catalogs · omochi"
sidebar:
  order: 4
---

## Basic Catalog

The SDKs provide typed helpers for all 18 Basic Catalog A2UI v0.9 components. Use the language's `UI` builder to declare text, image/video/audio, rows/columns/lists, cards, tabs, modals, dividers, buttons, fields, checkboxes, choices, sliders, and date/time inputs.

`component()` accepts wire-level properties, with its component name selecting the generated property type. See the [generated catalog types](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/catalog.ts) for exact property names and required fields.

## Identity and layout

Give conditional or reorderable components explicit keys. Keys map to component IDs; `root` is reserved for the root. Declare layout using the SDK's containers instead of assuming arbitrary HTML/CSS is available in a surface. Browser inputs should bind to a [Local](../../concepts/inputs/).

## Custom catalogs

```sh
bun run omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

Select any subset of outputs. `--check` verifies generated files in CI. Generation reads only local schemas. Generated bindings require a compatible renderer; they do not install implementations into the bundled Basic Catalog renderer.
