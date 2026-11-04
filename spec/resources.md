# Resources protocol v1

`omochi://resources/<id>` names immutable media registered by the application owning
its surface. IDs are 16 random bytes, lowercase hex (32 characters). They are opaque,
not file paths or cross-service addresses. Applications never embed node IDs or ports.

A2UI remains v0.9. Resource control frames are separate JSON objects on the existing
stdio, Unix JSONL, or WebSocket connection. Gateway protocol **3** is required; upgrade
hub and gateway together. Old SDKs without a resource announcement still serve A2UI,
but resource HTTP requests return 501. No announcement or resource bytes enter A2UI
snapshots or broadcast to other browsers.

The SDK announces `{"omochi":"resources/1","kind":"hello"}` on each connection.
A request has `omochi`, `kind: "request"`, `id`, `op`, and `resource`. Request IDs
are ASCII letters/digits/underscore/hyphen, 1–80 characters. `op: "stat"` needs
no other fields; `op: "read"` requires a nonnegative safe integer `offset` and
an integer `length` from 1 through 24576. Reads must fit entirely inside the resource.

Responses echo `id`, with `kind: "response"` and numeric `status`. Success is 200.
A stat response adds `size`, `mime`, and `etag` (the quoted resource ID). A read
response adds canonical padded base64 `data`. Errors: 400 invalid request, 404
unknown/released resource, 416 out-of-range read, 500 storage error, 501 unsupported,
503 unavailable/busy, 504 timeout. There is no push stream: consumers pull chunks.

Hub requests additionally carry `service` to the gateway. Each upstream connection
has its own broker, fresh request IDs, at most 8 pending requests, and a 15-second
per-request timeout. Gateway strips the service field before forwarding. Disconnect
rejects outstanding requests; late responses are discarded. HTTP backpressure stops
additional reads; abort cancels local pending work and stops subsequent reads. A
forwarded read already in flight may finish (at most 24 KiB); it is never broadcast.

TypeScript and Python SDK receivers, and the Rust Unix server transport, keep serving
resource requests while an action handler runs. Their pending application-message
queues are limited to 256 messages and 4 MiB of compact UTF-8 JSON, whichever is
reached first. The running action and resource RPC frames do not count toward these
limits. Exceeding either limit clears
the pending actions and latches a receive error, surfaced when the current handler
returns. Server transports immediately disconnect clients and close their listener;
resource pumps stop consuming and close their source iterator. Custom callback
integrations must release their connection in the source iterator's cleanup. This
fails the stream instead of pausing resource reads behind an overloaded action queue.
Normal shutdown also discards pending actions without needing space for a sentinel.

Browser HTTP routes are `/resources/<nodeId>~<serviceId>/<id>` at a hub and
`/resources/_/<id>` at a dev bridge or direct SDK server. GET and HEAD support media
MIME types, Content-Length, ETag, Accept-Ranges, single byte ranges (including suffix
ranges), If-Range, 206 and 416. Multiple ranges are ignored and return the full file.
There is no HTTP cache (`no-store`). The server streams bounded chunks, not whole files.
The browser handles codecs; omochi does not transcode or implement live streaming.

Media routes apply Origin/Fetch Metadata checks. Remote transport uses the same pinned
mTLS and revocation as A2UI. SVG is sandboxed with `default-src 'none'`; all resources
use `nosniff`. Requests can only read an application's registered IDs.

Registries copy data into private temporary storage at registration. They default to
1 GiB per resource and 4 GiB total. Registration rejects unknown MIME types unless
an explicit image/audio/video MIME type is supplied. Release stops new reads; a chunk
already read can finish, but subsequent reads fail. Close removes owned storage. The
registry never reuses IDs, even for the same content. Abrupt process termination can
leave temporary files; normal shutdown closes the registry.

`fixtures/resources.json` defines shared range/base64/error conformance cases. The
existing A2UI IR and its fixtures are unchanged.
