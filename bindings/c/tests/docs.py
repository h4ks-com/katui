"""The C code in the docs keeps working: every block compiles, and a block
with a `main` links and runs, with katui_run and katui_play drawing one
frame off-terminal. Run it with `just c-check`, which passes the library
and the system libraries to link as arguments."""

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
DOCS = [REPO / "docs/c.md", REPO / "bindings/c/README.md"]
BUILD = REPO / "target/c-docs"
CFLAGS = ["-std=c11", "-Wall", "-Wextra", "-Werror", f"-I{REPO / 'bindings/c/include'}"]
HEADLESS = ["-Dkatui_run=katui_headless_run", "-Dkatui_play=katui_headless_play"]


def blocks(path: Path) -> list[str]:
    return re.findall(r"^```c\n(.*?)^```", path.read_text(), re.M | re.S)


def check(name: str, code: str, link: list[str]) -> str | None:
    source = BUILD / f"{name}.c"
    source.write_text(code)
    if "int main(" not in code:
        command = ["cc", *CFLAGS, "-c", str(source), "-o", str(BUILD / f"{name}.o")]
    else:
        program = str(BUILD / name)
        headless = str(REPO / "bindings/c/tests/headless.c")
        command = ["cc", *CFLAGS, *HEADLESS, str(source), headless, *link, "-o", program]
    built = subprocess.run(command, capture_output=True, text=True)
    if built.returncode != 0:
        return built.stderr
    if "int main(" not in code:
        return None
    ran = subprocess.run([program], capture_output=True, text=True, timeout=120)
    return ran.stderr or f"exit {ran.returncode}" if ran.returncode != 0 else None


def main() -> int:
    BUILD.mkdir(parents=True, exist_ok=True)
    failures = 0
    for path in DOCS:
        found = blocks(path)
        if not found:
            print(f"{path.name} has no C blocks", file=sys.stderr)
            failures += 1
        for index, code in enumerate(found):
            name = f"{path.parent.name}-{path.stem}-{index}"
            error = check(name, code, sys.argv[1:])
            if error is not None:
                print(f"{path.relative_to(REPO)} block {index} fails:\n{error}", file=sys.stderr)
                failures += 1
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
