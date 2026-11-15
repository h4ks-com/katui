"""The Python code in the docs keeps working: every block compiles, and a
block that runs an app or a game type-checks under strict mypy and runs
headless here."""

import re
from pathlib import Path

import katui
import katui.game
import pytest
from conftest import BINDINGS, REPO
from katui.testing import Screen, frame
from mypy import api as mypy

DOCS = [REPO / "docs" / "python.md", BINDINGS / "README.md"]


def blocks(path: Path) -> list[str]:
    return re.findall(r"^```python\n(.*?)^```", path.read_text(), re.M | re.S)


CASES = [
    pytest.param(code, id=f"{path.name}:{index}")
    for path in DOCS
    for index, code in enumerate(blocks(path))
]


def runs(code: str) -> bool:
    return "run(" in code or "play(" in code


def test_the_docs_have_python_examples() -> None:
    assert all(blocks(path) for path in DOCS)


def test_the_blocks_that_run_type_check(tmp_path: Path) -> None:
    files = []
    for index, code in enumerate(code for path in DOCS for code in blocks(path) if runs(code)):
        file = tmp_path / f"block_{index}.py"
        file.write_text(code)
        files.append(str(file))
    config = str(BINDINGS / "pyproject.toml")
    report, errors, status = mypy.run(["--config-file", config, "--no-incremental", *files])
    assert status == 0, report + errors


@pytest.mark.parametrize("code", CASES)
def test_a_python_block_compiles_and_runs(code: str, monkeypatch: pytest.MonkeyPatch) -> None:
    compiled = compile(code, "<docs>", "exec")
    if not runs(code):
        return
    monkeypatch.setattr(katui, "run", lambda app: Screen(app, 600, 300).render())
    monkeypatch.setattr(katui.game, "play", lambda game: frame(game, steps=3, held=["right"]))
    exec(compiled, {"__name__": "__main__"})
