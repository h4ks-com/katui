# katui

Graphics-first terminal UI engine in Rust: cell and pixel planes over the
kitty graphics protocol, ratatui-compatible, portable fallbacks. Status:
skeleton. Read [ARCHITECTURE.md](ARCHITECTURE.md) and [AGENTS.md](AGENTS.md)
first.

## Terminals

Graphics need a terminal that speaks the kitty graphics protocol, such as
kitty. Other terminals get a quarter-block fallback.

Inside tmux, every image is anchored to one Unicode placeholder cell: plain
text that tmux moves, clears and hides with the rest of the pane, so
resizes, splits and window switches stay clean. tmux needs passthrough on
and true color for the outer terminal. A minimal config:

```
set -s escape-time 0
set -g allow-passthrough on
set -g focus-events on
set -g mouse on
set -as terminal-features ',xterm*:RGB,xterm*:sync'
```

To try the demos without touching your own sessions, save that as
`katui.conf` and start a separate tmux server with only that config:

```
tmux -L katui -f katui.conf new -s demos
```

## Develop

```
just check   # clippy, the fast gate; run after every change
just ci      # the full local gate; must pass before any commit
just hooks   # install the prek hooks, once per clone
just --list  # everything else
```
