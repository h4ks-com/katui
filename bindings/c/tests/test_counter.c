/* The counter renders headless and counts its clicks. */

#define KATUI_NO_MAIN
#include "../examples/counter.c"
#include "check.h"

int main(void) {
  Counter counter = {0};
  KatuiApp app = counter_app(&counter);
  KatuiScreen *screen = katui_screen_new(&app, 480, 240, 10, 20);
  CHECK(screen != NULL);
  shot(katui_screen_render(screen), "counter");
  CHECK(katui_screen_click(screen, 240, 72) == KATUI_STATUS_OK);
  CHECK(katui_screen_click(screen, 240, 72) == KATUI_STATUS_OK);
  CHECK(counter.count == 2);
  CHECK(katui_screen_click(screen, 5, 5) == KATUI_STATUS_OK);
  CHECK(counter.count == 2);
  shot(katui_screen_render(screen), "counter-clicked");
  katui_screen_free(screen);
  return 0;
}
