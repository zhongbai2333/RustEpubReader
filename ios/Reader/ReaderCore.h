#ifndef READER_CORE_H
#define READER_CORE_H
#include <stdint.h>
#include <stddef.h>
// request: borrowed UTF-8 JSON; result: owned UTF-8 JSON, freed exactly once.
char *reader_ios_request(const char *request);
void reader_ios_free(char *result);
// Diagnostic binary transport. Copy read-only bytes before freeing the handle.
typedef struct ReaderImageBuffer ReaderImageBuffer;
ReaderImageBuffer *reader_ios_image_read(const char *request);
const uint8_t *reader_ios_image_data(const ReaderImageBuffer *buffer);
size_t reader_ios_image_length(const ReaderImageBuffer *buffer);
void reader_ios_image_free(ReaderImageBuffer *buffer);
uint64_t reader_ios_epoch(void);
void reader_ios_cancel_sync(void);
#endif

void reader_ios_stop_nearby(void);
