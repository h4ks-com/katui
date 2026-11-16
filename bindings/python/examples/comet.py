"""A comet you steer with the arrow keys, trailing sparks. A click moves it
there and space bursts."""

from katui import Event, game
from katui.game import V3, Keys, Light, Rng, Spark, Sparks, scaled, srgb

SPEED = 90.0
TAIL = srgb(255, 140, 60)
HEAD = srgb(255, 230, 190)


class Comet(game.Game):
    def __init__(self) -> None:
        self.at = (60.0, 40.0)
        self.sparks = Sparks()
        self.rng = Rng(7)

    def burst(self) -> None:
        for _ in range(40):
            velocity = self.rng.direction() * self.rng.range(20, 60)
            self.sparks.add(Spark(V3(*self.at, 0), velocity, TAIL, life=0.8, width=1.5))

    def press(self, key: str) -> None:
        if key == " ":
            self.burst()

    def mouse(self, event: Event, at: tuple[float, float]) -> None:
        if event.action == "press":
            self.at = at

    def step(self, keys: Keys, dt: float) -> None:
        x, y = self.at
        if keys.is_held("left"):
            x -= SPEED * dt
        if keys.is_held("right"):
            x += SPEED * dt
        if keys.is_held("up"):
            y -= SPEED * dt
        if keys.is_held("down"):
            y += SPEED * dt
        self.at = (x, y)
        self.sparks.add(Spark(V3(x, y, 0), self.rng.direction() * 8, TAIL, life=0.5))
        self.sparks.step(dt, (0, 0, 0))

    def render(self, light: Light) -> None:
        light.dim(0.8)
        self.sparks.draw(light, lambda at: (at.x, at.y))
        light.glow(self.at, 9, scaled(TAIL, 0.35))
        light.glow(self.at, 3, HEAD)
        light.text((4, 4), "arrows to steer, click to jump, space to burst", srgb(120, 120, 140))


def new_game() -> Comet:
    return Comet()


if __name__ == "__main__":
    game.play(new_game())
