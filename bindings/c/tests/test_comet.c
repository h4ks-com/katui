/* The comet glows, follows the held keys and bursts into sparks. */

#define KATUI_NO_MAIN
#include "../examples/comet.c"
#include "check.h"

int main(void) {
  Comet comet;
  comet_open(&comet);
  KatuiGame game = comet_game(&comet);

  KatuiImage *still = katui_frame(&game, 200, 120, 1, NULL, 0, 1.0f / 60);
  CHECK(still != NULL);
  CHECK(lit(still) > 0);
  katui_image_free(still);

  comet_press(&comet, " ");
  CHECK(katui_sparks_len(comet.sparks) > 40);
  const char *held[] = {"right"};
  KatuiImage *moved = katui_frame(&game, 200, 120, 20, held, 1, 1.0f / 60);
  CHECK(comet.at.x > 60);
  CHECK(comet.at.y == 40);
  shot(moved, "comet");

  const char *unknown[] = {"no such key"};
  CHECK(katui_frame(&game, 200, 120, 1, unknown, 1, 1.0f / 60) == NULL);
  CHECK(katui_last_error() != NULL);

  comet_close(&comet);
  return 0;
}
