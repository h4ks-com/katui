# katui

Graphics-first terminal UI engine in Rust: cell and pixel planes over the
kitty graphics protocol, ratatui-compatible, portable fallbacks. Status:
skeleton. Read [ARCHITECTURE.md](ARCHITECTURE.md) and [AGENTS.md](AGENTS.md)
first.

## Develop

```
just check   # clippy, the fast gate; run after every change
just ci      # the full local gate; must pass before any commit
just hooks   # install the prek hooks, once per clone
just --list  # everything else
```
