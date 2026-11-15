#ifndef KATUI_GO_CALLBACKS_H
#define KATUI_GO_CALLBACKS_H

#include "katui.h"

KatuiStatus katui_go_run(uintptr_t handle);
KatuiScreen *katui_go_screen_new(uintptr_t handle, uint32_t width, uint32_t height, uint16_t cell_width,
                                 uint16_t cell_height);
KatuiStatus katui_go_play(uintptr_t handle);
KatuiImage *katui_go_frame(uintptr_t handle, uint32_t width, uint32_t height, uint32_t steps,
                           const char *const *held, size_t held_count, float dt);
KatuiElement *katui_go_list(const KatuiListState *state, uintptr_t handle);
KatuiElement *katui_go_document(const char *source, bool markdown, uintptr_t handle, bool images,
                                uint64_t link_key, float font_size, KatuiColor color, KatuiColor link,
                                KatuiColor tint);
void katui_go_light_paint(KatuiLight *light, uintptr_t handle);
void katui_go_sparks_draw(const KatuiSparks *sparks, KatuiLight *light, uintptr_t handle);

#endif
