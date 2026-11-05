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

## Media support by client

The development bridge and local HTTP/WS Hub preserve resource GET, HEAD and
Range reads. Hosted owner/share connections deliver published images through
native Hub validation; remote audio/video shows an unavailable state. Include
an image's canonical complete URI string in authoritative Surface state.
Constructed URLs or embedded Markdown destinations alone do not publish an image
to remote shares.

Flutter media support varies by platform; see the
[app guide](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md).
SDK registration limits are separate from delivery and image-decode budgets.
