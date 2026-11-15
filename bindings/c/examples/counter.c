/* A click counter, the smallest katui program. */

#include <stdio.h>

#include "katui.h"

enum { ADD = 1 };

typedef struct {
  int count;
} Counter;

static KatuiElement *counter_view(void *user) {
  const Counter *counter = user;
  char text[64];
  snprintf(text, sizeof text, "Clicked %d times", counter->count);
  KatuiElement *panel = katui_panel(KATUI_LIST(katui_label(text), katui_primary_button("Add one", ADD)));
  return katui_element_align(panel, KATUI_ALIGN_CENTER);
}

static KatuiControl counter_update(void *user, const KatuiMessage *message) {
  Counter *counter = user;
  if (message->key == ADD) {
    counter->count++;
  }
  return KATUI_CONTROL_REDRAW;
}

static KatuiApp counter_app(Counter *counter) {
  return (KatuiApp){.user = counter, .view = counter_view, .update = counter_update};
}

#ifndef KATUI_NO_MAIN
int main(void) {
  Counter counter = {0};
  KatuiApp app = counter_app(&counter);
  if (katui_run(&app) != KATUI_STATUS_OK) {
    fprintf(stderr, "katui: %s\n", katui_last_error());
    return 1;
  }
  return 0;
}
#endif
