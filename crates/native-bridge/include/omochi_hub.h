#ifndef OMOCHI_HUB_H
#define OMOCHI_HUB_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

/* All calls are nonblocking. No caller pointer is retained. JSON is UTF-8.
 * The host must continue polling/replying to storage until close settles.
 * Storage callbacks must never reenter an administrative Hub command.
 * A write callback returns committed/conflict/failed/unknown, never a guess.
 */
uint32_t omochi_hub_abi_version(void);
uint64_t omochi_hub_create(void); /* 0 = instance capacity reached */
/* 0 = invalid/rejected/at capacity; actions are never buffered for retry.
 * Commands: start, state, pair, forget, retry, reload, suspend, resume,
 * subscribe, next (optional applied sequence ack), action, image, close.
 * Input <= 4 MiB. Eight outstanding commands, 8 MiB input credit; close has
 * reserved credit. Polling a result releases its input/command credit.
 */
uint64_t omochi_hub_submit(uint64_t hub, const uint8_t *json, size_t length);
/* Returns an allocated JSON event or NULL. Always free each non-NULL pointer.
 * kind = result (id, ok, value/error), storage (id, op, fields), or terminal.
 * Storage events have priority over render/results to avoid save deadlock.
 */
char *omochi_hub_poll(uint64_t hub);
void omochi_hub_string_free(char *value);
bool omochi_hub_storage_reply(uint64_t hub, uint64_t request,
                             const uint8_t *json, size_t length);
void omochi_hub_cancel(uint64_t hub, uint64_t request);
/* Image results contain a blob ID; bytes never pass through JSON/base64.
 * Reads copy <= 64 KiB and recheck handle lifetime. -1 = invalid/expired.
 * Two blobs maximum; free even if decoding/copying is cancelled or fails.
 */
intptr_t omochi_hub_blob_read(uint64_t hub, uint64_t blob, size_t offset,
                            uint8_t *destination, size_t length);
void omochi_hub_blob_free(uint64_t hub, uint64_t blob);
/* Call only after successful close. False means work/callbacks still settling;
 * keep polling and retry release. Never interpret false as shutdown success.
 */
bool omochi_hub_release(uint64_t hub);

#ifdef __cplusplus
}
#endif
#endif
