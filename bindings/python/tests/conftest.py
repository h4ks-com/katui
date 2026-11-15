import sys
from pathlib import Path

BINDINGS = Path(__file__).parents[1]
REPO = BINDINGS.parents[1]
SHOTS = REPO / "target" / "python-shots"

sys.path.insert(0, str(BINDINGS / "examples"))


def shot(name: str) -> Path:
    """Where a test saves its screenshot, for a person to look at."""
    SHOTS.mkdir(parents=True, exist_ok=True)
    return SHOTS / f"{name}.png"
