import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const BINDINGS = fileURLToPath(new URL('..', import.meta.url))
export const REPO = join(BINDINGS, '..', '..')
export const SHOTS = join(REPO, 'target', 'node-shots')

/** Where a test saves its screenshot, for a person to look at. */
export function shot(name: string): string {
  mkdirSync(SHOTS, { recursive: true })
  return join(SHOTS, `${name}.png`)
}
