# katui: agent guide

Graphics-first terminal UI engine in Rust: cell and pixel planes over the
kitty graphics protocol, ratatui-compatible, portable fallbacks. The module
map, trait seams and test strategy live in [ARCHITECTURE.md](ARCHITECTURE.md). Read it before
writing code; it is the contract this guide enforces.

## Layout

```
crates/katui-proto/     L0: protocol codecs (kitty, rgp), capability probes (pure, no I/O)
crates/katui-scene/     L2: cell plane, placements, RGBA surfaces with tile damage (pure state)
crates/katui-present/   L1: presenters: cells, blocks, kitty placements, full-frame surfaces (pure)
crates/katui-ratatui/   ratatui Backend adapter, image decode/fit, the gallery demo
crates/katui-rt/        the impure layer: /dev/tty transport, frame clock, session guard,
                        the engine demos (dvd, snake, plasma, life)
crates/katui-layout/    L4: constraint splitting of cell rectangles (pure)
crates/katui-content/   L6: documents to scene cells, markdown today (pure)
scripts/                repo tooling (the commit-message gate)
.opencode/              repo commands and skills (/rev review fan-out)
```

New crates grow bottom-up with the phases in ARCHITECTURE.md. A crate lands
only with its tier-1 test suite.

## Commands (the justfile is the single source of truth)

- `just check`: clippy with CI's denials; run after every change
- `just ci`: the gate; must pass before any commit
- `just fix`: clippy --fix + fmt, the mechanical repairs
- `just test`: unit tests
- `just deny` / `just typos` / `just dupes` / `just machete`: the extra CI
  gates (licenses and advisories, spelling, copy-paste duplication, unused
  dependencies)
- `just metrics`: per-function complexity report via rust-code-analysis
  (`cargo install rust-code-analysis-cli` once). Report tier only; the
  gate tier stays clippy's cognitive ceiling of 20
- `just hooks`: install the prek hooks, once per clone
- CI (`.github/workflows/ci.yml`) runs these same targets through just.
  Never duplicate a check in CI that is not behind a just target, and never
  run a raw tool when a just target exists for it.

## The loop

1. Keep the change small; one concern per commit.
2. `just check` after every change, `just ci` before committing.
3. Review every non-trivial change with `/rev` (`.opencode/skill/review/`):
   it runs the tooling with autofixes, fans out three specialist reviewers
   and reports findings. Fix Critical and Important findings first.
4. Commit with `git commit`. The prek hooks run fmt, clippy and the
   commit-message gate. If a hook fails, fix the cause. Never commit with
   `--no-verify`.
5. Commit messages: one conventional line, `<type>: <lowercase imperative
   summary>` with types feat, fix, perf, refactor, test, docs, chore, ci,
   build, revert; e.g. `feat: chunk kitty payloads at 4096 bytes`.

## Rules

- Work stays inside this repository. Never modify, branch, build in, or
  run tests in another repository on this machine, even a scratch branch,
  unless the user asks for exactly that in the current session.

- Code is done only when `just ci` passes. Run it, don't assume.
- No `unwrap`, `panic!` or `todo!` in non-test code; clippy denies them.
  `expect` needs a reason. `unsafe` is denied workspace-wide; the only
  future exception is syscall glue in the runtime crate, and every such
  block carries a `// SAFETY:` comment.
- Errors: one typed `thiserror` enum per crate. No `Result<_, String>`
  seams.
- Cognitive complexity over 20 fails CI (clippy.toml): the function grew a
  second job, split it.
- Layering, from ARCHITECTURE.md: no layer above present emits bytes;
  nothing below scene knows what a widget is; the core crates perform zero
  I/O and everything flows through the Sink, Source and Transport traits.
- Unit-testable means no terminal. Encoders get golden-byte tests, parsers
  get fuzzing and recorded-reply fixtures, the scene gets diff invariants.
  A module lands only with tests and an artifact a human can look at:
  bytes, a scene dump or a PNG.
- New dependencies: declare once in `[workspace.dependencies]`, justify in
  the commit, meet the adoption bar in ARCHITECTURE.md. Copyleft stays out
  of the build graph; cargo-deny enforces the license list.
- Comments explain why, never what; default to zero. No change-history
  commentary.
- Prose in docs, commits and comments: declarative, terse, no em dashes.
- Nothing personal in the repo: no real names, handles, emails, local
  machine paths or assistant attribution in code, docs, fixtures, commit
  messages or CI. Test data uses generic mock names ("acme", "reviewer").
- `research/` is git-excluded local scratch. Tracked files never reference
  it or any local path; everything a cold reader needs lives in tracked
  docs or a public URL. The links from ARCHITECTURE.md into `research/` are
  temporary and disappear as those notes are distilled into tracked docs.
- Do not weaken a gate to make it pass. Fix the code, or change the rule in
  a commit of its own with the argument.

## Secrets

The repo holds none. Keep it that way: no tokens in code, docs, CI or
fixtures.
