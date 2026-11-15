"""The Python code in the docs keeps working: every block compiles, and a
block that runs an app or a game runs headless here."""

import re
from pathlib import Path

import katui
import katui.game
import pytest
from conftest import BINDINGS, REPO
from katui.testing import Screen, frame

DOCS = [REPO / "docs" / "python.md", BINDINGS / "README.md"]


def blocks(path: Path) -> list[str]:
    return re.findall(r"^```python\n(.*?)^```", path.read_text(), re.M | re.S)


CASES = [
    pytest.param(code, id=f"{path.name}:{index}")
    for path in DOCS
    for index, code in enumerate(blocks(path))
]


def test_the_docs_have_python_examples() -> None:
    assert all(blocks(path) for path in DOCS)


@pytest.mark.parametrize("code", CASES)
def test_a_python_block_compiles_and_runs(code: str, monkeypatch: pytest.MonkeyPatch) -> None:
    compiled = compile(code, "<docs>", "exec")
    if "run(" not in code and "play(" not in code:
        return
    monkeypatch.setattr(katui, "run", lambda app: Screen(app, 600, 300).render())
    monkeypatch.setattr(katui.game, "play", lambda game: frame(game, steps=3, held=["right"]))
    exec(compiled, {"__name__": "__main__"})
