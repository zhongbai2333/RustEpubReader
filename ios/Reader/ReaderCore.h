#ifndef READER_CORE_H
#define READER_CORE_H
#include <stdint.h>
// request: borrowed UTF-8 JSON; result: owned UTF-8 JSON, freed exactly once.
char *reader_ios_request(const char *request);
void reader_ios_free(char *result);
uint64_t reader_ios_epoch(void);
void reader_ios_cancel_sync(void);
#endif

void reader_ios_stop_nearby(void);
