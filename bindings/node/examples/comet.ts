// A comet you steer with the arrow keys, trailing sparks. Space bursts.

import { fileURLToPath } from 'node:url'
import {
  Game,
  type Keys,
  type Light,
  type Point,
  play,
  Rng,
  Spark,
  Sparks,
  scaled,
  srgb,
  V3,
} from '@mattfillipe/katui'

const SPEED = 90
const TAIL = srgb(255, 140, 60)
const HEAD = srgb(255, 230, 190)

export class Comet extends Game {
  at: Point = [60, 40]
  sparks = new Sparks()
  rng = new Rng(7)

  burst() {
    const [x, y] = this.at
    for (let i = 0; i < 40; i++) {
      const velocity = this.rng.direction().mul(this.rng.range(20, 60))
      this.sparks.add(new Spark(new V3(x, y, 0), velocity, TAIL, 0.8, 1.5))
    }
  }

  press(key: string) {
    if (key === ' ') this.burst()
  }

  step(keys: Keys, dt: number) {
    let [x, y] = this.at
    if (keys.isHeld('left')) x -= SPEED * dt
    if (keys.isHeld('right')) x += SPEED * dt
    if (keys.isHeld('up')) y -= SPEED * dt
    if (keys.isHeld('down')) y += SPEED * dt
    this.at = [x, y]
    this.sparks.add(new Spark(new V3(x, y, 0), this.rng.direction().mul(8), TAIL, 0.5))
    this.sparks.step(dt, [0, 0, 0])
  }

  render(light: Light) {
    light.dim(0.8)
    this.sparks.draw(light, (at) => [at.x, at.y])
    light.glow(this.at, 9, scaled(TAIL, 0.35))
    light.glow(this.at, 3, HEAD)
    light.text([4, 4], 'arrows to steer, space to burst', srgb(120, 120, 140))
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  play(new Comet())
}
