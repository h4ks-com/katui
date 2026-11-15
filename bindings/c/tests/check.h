/* What every C test shares: a check that stops the test, and screenshots. */

#ifndef KATUI_CHECK_H
#define KATUI_CHECK_H

#include <stdio.h>
#include <stdlib.h>

#include "katui.h"

#define CHECK(condition)                                                    \
  do {                                                                      \
    if (!(condition)) {                                                     \
      const char *error = katui_last_error();                               \
      fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__,      \
              #condition);                                                  \
      fprintf(stderr, "  last katui error: %s\n", error ? error : "none");  \
      exit(1);                                                              \
    }                                                                       \
  } while (0)

/* The pixels whose red channel is above 40. */
static inline size_t lit(const KatuiImage *image) {
  size_t length = 0;
  const uint8_t *pixels = katui_image_rgba(image, &length);
  size_t count = 0;
  for (size_t at = 0; at < length; at += 4) {
    count += pixels[at] > 40;
  }
  return count;
}

/* Save `image` as target/c-shots/<name>.png for a person to look at, then free it. */
static inline void shot(KatuiImage *image, const char *name) {
  char path[256];
  snprintf(path, sizeof path, "target/c-shots/%s.png", name);
  CHECK(image != NULL);
  CHECK(katui_image_save(image, path) == KATUI_STATUS_OK);
  katui_image_free(image);
}

#endif
