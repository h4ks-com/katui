/* A comet you steer with the arrow keys, trailing sparks. A click moves it
   there and space bursts. */

#include <stdio.h>
#include <string.h>

#include "katui.h"

#define SPEED 90.0f

typedef struct {
  KatuiPoint at;
  KatuiSparks *sparks;
  KatuiRng *rng;
} Comet;

static KatuiRgb tail(void) { return katui_srgb(255, 140, 60); }

static void comet_burst(Comet *comet) {
  for (int spark = 0; spark < 40; spark++) {
    KatuiV3 velocity = katui_v3_mul(katui_rng_direction(comet->rng), katui_rng_range(comet->rng, 20, 60));
    katui_sparks_add(comet->sparks, (KatuiSpark){.at = katui_v3(comet->at.x, comet->at.y, 0),
                                                 .velocity = velocity,
                                                 .color = tail(),
                                                 .life = 0.8f,
                                                 .width = 1.5f});
  }
}

static void comet_press(void *user, const char *key) {
  if (strcmp(key, " ") == 0) {
    comet_burst(user);
  }
}

static void comet_mouse(void *user, const KatuiEvent *event, KatuiPoint at) {
  Comet *comet = user;
  if (katui_event_action(event) == KATUI_MOUSE_PRESS) {
    comet->at = at;
  }
}

static float axis(const KatuiKeys *keys, const char *less, const char *more) {
  return (float)katui_keys_is_held(keys, more) - (float)katui_keys_is_held(keys, less);
}

static void comet_step(void *user, const KatuiKeys *keys, float dt) {
  Comet *comet = user;
  comet->at.x += SPEED * dt * axis(keys, "left", "right");
  comet->at.y += SPEED * dt * axis(keys, "up", "down");
  KatuiV3 drift = katui_v3_mul(katui_rng_direction(comet->rng), 8);
  katui_sparks_add(comet->sparks, (KatuiSpark){.at = katui_v3(comet->at.x, comet->at.y, 0),
                                               .velocity = drift,
                                               .color = tail(),
                                               .life = 0.5f,
                                               .width = 1});
  katui_sparks_step(comet->sparks, dt, katui_v3(0, 0, 0));
}

static bool flat(void *user, KatuiV3 at, KatuiPoint *on_screen) {
  (void)user;
  *on_screen = (KatuiPoint){at.x, at.y};
  return true;
}

static void comet_render(void *user, KatuiLight *light) {
  Comet *comet = user;
  katui_light_dim(light, 0.8f);
  katui_sparks_draw(comet->sparks, light, flat, NULL);
  katui_light_glow(light, comet->at, 9, katui_scaled(tail(), 0.35f));
  katui_light_glow(light, comet->at, 3, katui_srgb(255, 230, 190));
  katui_light_text(light, (KatuiPoint){4, 4}, "arrows to steer, click to jump, space to burst", 1, katui_srgb(120, 120, 140));
}

static void comet_open(Comet *comet) {
  *comet = (Comet){.at = {60, 40}, .sparks = katui_sparks_new(), .rng = katui_rng_new(7)};
}

static void comet_close(Comet *comet) {
  katui_sparks_free(comet->sparks);
  katui_rng_free(comet->rng);
}

static KatuiGame comet_game(Comet *comet) {
  return (KatuiGame){
      .user = comet, .press = comet_press, .mouse = comet_mouse, .step = comet_step, .render = comet_render};
}

#ifndef KATUI_NO_MAIN
int main(void) {
  Comet comet;
  comet_open(&comet);
  KatuiGame game = comet_game(&comet);
  KatuiStatus status = katui_play(&game);
  comet_close(&comet);
  if (status != KATUI_STATUS_OK) {
    fprintf(stderr, "katui: %s\n", katui_last_error());
    return 1;
  }
  return 0;
}
#endif
