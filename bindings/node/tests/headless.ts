// The package with `run` and `play` drawing off-terminal, for the code in
// the docs.

import { type AppLike, frame, type GameLike, Screen } from '@matheusfillipe/katui'

export * from '@matheusfillipe/katui'

export function run(app: AppLike): void {
  new Screen(app, 600, 300).render()
}

export function play(game: GameLike): void {
  frame(game, { steps: 3, held: ['right'] })
}
