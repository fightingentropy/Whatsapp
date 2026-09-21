#ifndef WHATSAPP_CORE_BRIDGE_H
#define WHATSAPP_CORE_BRIDGE_H
#include <stdint.h>
// Version 1 JSON boundary. Calls are serialized on the engine queue.
uint64_t wa_start(const char *root, void (*wake)(void));
int32_t wa_command(uint64_t handle, const char *input);
char *wa_poll(uint64_t handle);
void wa_free_string(char *value);
void wa_stop(uint64_t handle);
#endif
