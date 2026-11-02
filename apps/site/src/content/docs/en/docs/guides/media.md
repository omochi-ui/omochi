---
title: "Serve local media"
description: "Serve local media · omochi"
sidebar:
  order: 6
---

Register an immutable copy of a file or bytes and pass its URI to a media component:

```python
photo = app.resources.file("photo.png")
# Inside a surface:
ui.image(photo)
```

TypeScript uses `app.resources.file(path, mimeType?)` and `app.resources.bytes(data, mimeType)`. Rust uses `app.resources.file(path, Option<&str>)?` and `app.resources.bytes(&data, mime)?`. All return `omochi://resources/<id>` URIs.

The browser does not need access to the application's filesystem. This works over the development bridge and local/remote hubs. Registration uses bounded synchronous I/O: register large files before starting, or use a worker/thread.

Keep resources registered while a surface may reference them, including playback. `release(uri)` prevents further reads; changed content needs a new registration. Default limits are 1 GiB per file and 4 GiB total. MIME type is inferred for files and required for bytes. There is no codec conversion.

[Media examples](https://github.com/omochi-ui/omochi/tree/main/examples/media) show Python and TypeScript; the Rust SDK also includes a media example. Custom transports must preserve resource frames and reply to the originating connection. See the [wire contract](https://github.com/omochi-ui/omochi/blob/main/spec/resources.md).
