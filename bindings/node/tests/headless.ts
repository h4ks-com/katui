// The package with `run` and `play` drawing off-terminal, for the code in
// the docs.

import { type AppLike, frame, type GameLike, Screen } from '@mattfillipe/katui'

export * from '@mattfillipe/katui'

export function run(app: AppLike): void {
  new Screen(app, 600, 300).render()
}

export function play(game: GameLike): void {
  frame(game, { steps: 3, held: ['right'] })
}
