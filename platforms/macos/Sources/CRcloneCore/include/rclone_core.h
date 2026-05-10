#ifndef RCLONE_BROWSER_CORE_H
#define RCLONE_BROWSER_CORE_H

#ifdef __cplusplus
extern "C" {
#endif

char *rb_call(const char *request_json);
void rb_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif
