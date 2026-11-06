---
title: "Serve local media"
description: "Serve local media · omochi"
sidebar:
  order: 6
---

Register a file or bytes and pass the returned URI to a media component:

```python
photo = app.resources.file("photo.png")
clip = app.resources.bytes(video_bytes, mime_type="video/mp4")
# Inside a Surface:
ui.image(photo)
ui.video(clip)
```

TypeScript uses `app.resources.file(path, mimeType?)` and `app.resources.bytes(data, mimeType)`. Rust uses `app.resources.file(path, Option<&str>)?` and `app.resources.bytes(&data, mime)?`. All return `omochi://resources/<id>` URIs. The browser does not need access to the app's filesystem.

## Registration and release

Registration creates an immutable copy. Register large files before starting the app or use a worker/thread. Keep resources registered while a Surface references them, including playback. Use `release(uri)` to prevent further reads when no longer needed. Register changed content again and display the new URI. Runtime shutdown closes the registry.

Default limits are 1 GiB per file and 4 GiB total. To change them, supply a `ResourceRegistry` with custom limits through the TypeScript App's third constructor argument, Python's `resources=`, or Rust's `app.resources` field. Files infer MIME type from their extension; bytes require an explicit MIME type. There is no codec conversion.

## Media support by client

Local Hubs and the development bridge deliver images, audio, and video, supporting GET, HEAD, and Range requests. Hosted owner/share browser connections display published images; audio/video are unavailable.

To publish a remote image, include its complete URI in Surface state, as in `ui.image(photo)`. URLs constructed later by a component or embedded only in Markdown destinations do not publish images. Clients have delivery and decode limits separate from SDK registration limits.

The [mobile and desktop app](../mobile/) has platform-specific support. See the [media samples](https://github.com/omochi-ui/omochi/tree/main/examples/media) and [custom transport delivery](../../reference/transports/#custom-resource-delivery).
