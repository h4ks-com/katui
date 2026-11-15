/* katui_run and katui_play for the docs test: one frame drawn off-terminal. */

#include "katui.h"

KatuiStatus katui_headless_run(const KatuiApp *app) {
  KatuiScreen *screen = katui_screen_new(app, 600, 300, 10, 20);
  KatuiImage *picture = katui_screen_render(screen);
  KatuiStatus status = picture != NULL ? KATUI_STATUS_OK : KATUI_STATUS_CALLBACK;
  katui_image_free(picture);
  katui_screen_free(screen);
  return status;
}

KatuiStatus katui_headless_play(const KatuiGame *game) {
  const char *held[] = {"right"};
  KatuiImage *picture = katui_frame(game, 320, 180, 3, held, 1, 1.0f / 60);
  KatuiStatus status = picture != NULL ? KATUI_STATUS_OK : KATUI_STATUS_CALLBACK;
  katui_image_free(picture);
  return status;
}
