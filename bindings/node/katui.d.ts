import type {
  AppLike,
  Child,
  Control,
  Element,
  Event,
  GameLike,
  Image,
  Keys,
  Light,
  Span,
  Theme,
  Waker,
} from './index.js'

export * from './index.js'

/** Children side by side, laid out with flexbox. */
export declare function row(...children: Child[]): Element
/** Children one above the other, laid out with flexbox. */
export declare function column(...children: Child[]): Element
/** Children drawn over each other. */
export declare function stack(...children: Child[]): Element
/** A raised box around its children, drawn with the theme. */
export declare function panel(...children: Child[]): Element
/** The page background with its children laid out in a column. */
export declare function page(...children: Child[]): Element
export declare function text(text: string): Element
export declare function rich(spans: Span[]): Element
export declare function image(image: Image): Element
export declare function spacer(): Element

/** What `App.update` does: calls a function message and returns what it returns. */
export declare function defaultUpdate(message: unknown): Control | undefined | void

/**
 * A pixel UI program: state that shows itself as a view and changes on
 * messages. Extend it and pass an instance to `run`.
 */
export declare abstract class App implements AppLike {
  /** Keep `waker` to send messages later, like `waker.every(0.25, tick)`. */
  start(waker: Waker): void
  /** The view of the current state. */
  abstract view(): Element
  /** Apply `message`. A function message is called. Nothing redraws. */
  update(message: unknown): Control | undefined | void
  /** React to a key or a paste. Nothing waits for the next event. */
  input(event: Event): Control | undefined | void
  /** The theme for cells of `cell` pixels, asked before every frame. */
  theme(cell: [number, number]): Theme | undefined | void
}

/** A game `play` can run. Extend it and fill in `step` and `render`. */
export declare class Game implements GameLike {
  /** `key` went down. */
  press(key: string): void
  /** `key` repeated while held. */
  repeat(key: string): void
  /** The screen became the size of `light`, so baked art needs redoing. */
  resize(light: Light): void
  /** Advance the world by `dt` seconds, a fixed step. */
  step(keys: Keys, dt: number): void
  /** Draw the world into `light`. */
  render(light: Light): void
  /** Whether the game is over, so `play` ends after this frame. */
  finished(): boolean
}
