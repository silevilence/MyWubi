#ifndef MYWUBI_V1_H
#define MYWUBI_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* UTF-8, byte length, no terminator required. Do not mutate data. */
typedef struct { const uint8_t *data; uint64_t len; uint64_t allocation; } MyWubiBuffer;
uint32_t mywubi_v1_version(void);
/* Caller supplies valid readable input and aligned writable output.
 * Transport status: 0 success, 2 invalid argument, 8 internal failure.
 * JSON error codes are separate; always inspect ok. Max input: 16 MiB. */
uint32_t mywubi_v1_call(const uint8_t *input, uint64_t len, MyWubiBuffer *output);
/* After release all copies of data become invalid. Repeat release returns 3. */
uint32_t mywubi_v1_buffer_free(uint64_t allocation);
#ifdef __cplusplus
}
#endif
#endif
