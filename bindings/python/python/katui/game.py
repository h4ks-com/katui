"""Arcade games: draw into a `Light` buffer, read the held `Keys`, and let
`play` run the fixed-step loop."""

from katui._katui import (
    BASE_IMAGE,
    MULTIPLEXED_SHRINK,
    V3,
    Camera,
    Keys,
    Light,
    Rng,
    Spark,
    Sparks,
    lerp3,
    play,
    scaled,
    srgb,
)


class Game:
    """A game `play` can run. Subclass it and fill in `step` and `render`."""

    def press(self, key: str) -> None:
        """`key` went down."""

    def repeat(self, key: str) -> None:
        """`key` auto-repeated while held."""

    def resize(self, light: Light) -> None:
        """The screen became the size of `light`, so baked art needs redoing."""

    def step(self, keys: Keys, dt: float) -> None:
        """Advance the world by `dt` seconds, a fixed step."""

    def render(self, light: Light) -> None:
        """Draw the world into `light`."""

    def finished(self) -> bool:
        """Whether the game is over, so `play` ends after this frame."""
        return False


__all__ = [
    "BASE_IMAGE",
    "MULTIPLEXED_SHRINK",
    "V3",
    "Camera",
    "Game",
    "Keys",
    "Light",
    "Rng",
    "Spark",
    "Sparks",
    "lerp3",
    "play",
    "scaled",
    "srgb",
]
