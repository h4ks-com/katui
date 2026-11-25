import type {
  Align,
  Color,
  Column,
  Control,
  Event,
  FrameOptions,
  GameLike,
  Image,
  Keys,
  Length,
  Light,
  ListState,
  Point,
  Scroll,
  ScrollState,
  Span,
  TextArea,
  TextField,
  Theme,
  Toasts,
  TreeNode,
  TreeState,
} from './index.js'

export * from './index.js'

declare const messages: unique symbol

/**
 * One node of a view that sends messages of type `M`. `M` exists for the
 * type checker only: a panel holding a button that sends `'save'` is an
 * `Element<'save'>`, and an app whose messages are `'save' | 'quit'` cannot
 * show a button that sends anything else. Methods change the element and
 * return it, so calls chain; a handler adds its message type to `M`.
 */
export interface Element<M = unknown> {
  readonly [messages]?: M
  layer(key: number): Element<M>
  scroll(offset: number): Element<M>
  onClick<const N>(message: N): Element<M | N>
  onSpanClick<const N>(span: number, message: N): Element<M | N>
  onHover<const N>(message: N): Element<M | N>
  onLeave<const N>(message: N): Element<M | N>
  /** `message(scroll)` makes the message for each new position. */
  onScroll<N>(message: (scroll: Scroll) => N): Element<M | N>
  /**
   * `message(x, y)` makes the message for the press and each move until
   * the release, `x` and `y` as fractions of the element's box.
   */
  onPress<N>(message: (x: number, y: number) => N): Element<M | N>
  onGripPress<N>(message: (x: number, y: number) => N): Element<M | N>
  grip(): Element<M>
  popup(x: number, y: number): Element<M>
  opaque(): Element<M>
  scrollbar(color: Color): Element<M>
  id(id: number): Element<M>
  /**
   * Scroll so the descendant with `id` starts at the top; `onScroll`
   * receives the new position. Takes an `id` number or an `anchorId`.
   */
  reveal(id: number | bigint): Element<M>
  padding(all: number): Element<M>
  paddingXy(x: number, y: number): Element<M>
  gap(gap: number): Element<M>
  width(width: Length): Element<M>
  height(height: Length): Element<M>
  size(width: Length, height: Length): Element<M>
  grow(grow: number): Element<M>
  align(to: Align): Element<M>
  justify(to: Align): Element<M>
  background(color: Color): Element<M>
  radius(radius: number): Element<M>
  border(width: number, color: Color): Element<M>
  color(color: Color): Element<M>
  fontSize(size: number): Element<M>
  weight(weight: number): Element<M>
}

/** What a container takes: elements, arrays of them, and gaps left out. */
export type Child<M = unknown> = Element<M> | null | undefined | false | readonly Child<M>[]

/** The messages the elements in `C` send, through nested arrays. */
export type MessageOf<C> =
  C extends Element<infer M> ? M : C extends readonly (infer I)[] ? MessageOf<I> : never

export declare const Element: {
  row<const C extends readonly Child[]>(children: C): Element<MessageOf<C>>
  column<const C extends readonly Child[]>(children: C): Element<MessageOf<C>>
  stack<const C extends readonly Child[]>(children: C): Element<MessageOf<C>>
  text(text: string): Element<never>
  rich(spans: Span[]): Element<never>
  image(image: Image): Element<never>
  spacer(): Element<never>
  readonly prototype: Element<unknown>
}

/** Children side by side, laid out with flexbox. */
export declare function row<const C extends readonly Child[]>(...children: C): Element<MessageOf<C>>
/** Children one above the other, laid out with flexbox. */
export declare function column<const C extends readonly Child[]>(
  ...children: C
): Element<MessageOf<C>>
/** Children drawn over each other. */
export declare function stack<const C extends readonly Child[]>(
  ...children: C
): Element<MessageOf<C>>
/** A raised box around its children, drawn with the theme. */
export declare function panel<const C extends readonly Child[]>(
  ...children: C
): Element<MessageOf<C>>
/** The page background with its children laid out in a column. */
export declare function page<const C extends readonly Child[]>(
  ...children: C
): Element<MessageOf<C>>
export declare function text(text: string): Element<never>
export declare function rich(spans: Span[]): Element<never>
export declare function image(image: Image): Element<never>
export declare function spacer(): Element<never>

export declare function label(text: string): Element<never>
export declare function title(text: string): Element<never>
export declare function heading(text: string): Element<never>
export declare function hint(text: string): Element<never>
export declare function progress(fraction: number): Element<never>
export declare function spinner(elapsed: number): Element<never>
export declare function toasts(toasts: Toasts): Element<never>
export declare function keyHints(hints: Array<[string, string]>): Element<never>
export declare function textField(
  field: TextField,
  placeholder?: string | null,
  focused?: boolean | null,
): Element<never>
export declare function textArea(
  area: TextArea,
  placeholder?: string | null,
  focused?: boolean | null,
): Element<never>
export declare function button<const M>(text: string, message: M): Element<M>
export declare function focusableButton<const M>(
  text: string,
  message: M,
  focused: boolean,
): Element<M>
export declare function primaryButton<const M>(text: string, message: M): Element<M>
export declare function checkbox<const M>(text: string, checked: boolean, message: M): Element<M>
export declare function toggleSwitch<const M>(text: string, on: boolean, message: M): Element<M>
export declare function slider<M>(value: number, change: (value: number) => M): Element<M>
export declare function tabs<M>(
  titles: string[],
  selected: number,
  choose: (index: number) => M,
): Element<M>
export declare function radio<M>(
  options: string[],
  selected: number,
  choose: (index: number) => M,
): Element<M>
export declare function listView<M>(
  state: ListState,
  row: (index: number, selected: boolean) => Element<M>,
): Element<M>
export declare function table<M>(
  columns: Column[],
  rows: string[][],
  state: ListState,
  pick: (index: number) => M,
): Element<M>
export declare function tree<M>(
  roots: TreeNode[],
  state: TreeState,
  toggle: (id: number) => M,
  select: (id: number) => M,
): Element<M>
export declare function scrollView<const C extends readonly Child[], M>(
  state: ScrollState,
  content: C,
  moved: (scroll: Scroll) => M,
): Element<MessageOf<C> | M>
export declare function split<A, B, M>(
  first: Element<A>,
  second: Element<B>,
  fraction: number,
  resize: (fraction: number) => M,
  orientation?: 'horizontal' | 'vertical',
): Element<A | B | M>
export declare function select<const T, M>(
  options: string[],
  selected: number,
  open: boolean,
  toggle: T,
  choose: (index: number) => M,
): Element<T | M>
export declare function menu<const T, M>(
  title: string,
  items: string[],
  open: boolean,
  toggle: T,
  choose: (index: number) => M,
): Element<T | M>
export declare function tooltip<M, const S, const H>(
  anchor: Element<M>,
  text: string,
  shown: boolean,
  show: S,
  hide: H,
): Element<M | S | H>
export declare function dialog<B, const C extends readonly Child[], const D>(
  title: string,
  body: Element<B>,
  actions: C,
  dismiss: D,
): Element<B | MessageOf<C> | D>

/** How `html` and `markdown` handle links, images and text. */
export interface DocumentOptions<M = string> {
  /** Makes the message a link click sends; without it the click sends the href. */
  onLink?: (href: string) => M
  /** The image for each `src`, as an object or a function. */
  images?: Record<string, Image | undefined> | ((src: string) => Image | null | undefined)
  fontSize?: number
  color?: Color
  link?: Color
  tint?: Color
}

/** An HTML document; a link click sends `onLink(href)`, or the href itself. */
export declare function html<M = string>(source: string, options?: DocumentOptions<M>): Element<M>
/** A Markdown document; a link click sends `onLink(href)`, or the href itself. */
export declare function markdown<M = string>(
  source: string,
  options?: DocumentOptions<M>,
): Element<M>

/** Sends messages of type `M` to a running app. */
export interface Waker<M = unknown> {
  /** Hand `message` to the app's `update`. `false` once the app has quit. */
  send(message: M): boolean
  /** Hand `message` to `update` every `seconds` until the app quits. */
  every(seconds: number, message: M): void
}

/** Anything `run` and `Screen` accept as an app with messages of type `M`. */
export interface AppLike<M = unknown> {
  view(): Element<M>
  update?(message: M): Control | undefined | void
  input?(event: Event): Control | undefined | void
  theme?(cell: [number, number]): Theme | undefined | void
  start?(waker: Waker<M>): void
}

/** Run `app` on the terminal until it quits. */
export declare function run<M>(app: AppLike<M>): void

/** An app drawn into pixels, with clicks and keys sent by hand. */
export declare class Screen<M = unknown> {
  /** `app` on a screen of `width` by `height` pixels in cells of `cell`. */
  constructor(app: AppLike<M>, width?: number, height?: number, cell?: [number, number])
  /** Draw the app's view and return the picture. */
  render(): Image
  /** Click at `x`, `y` pixels and deliver what the click sends. */
  click(x: number, y: number): void
  /** Press at `x`, `y` and keep holding, for a drag. */
  press(x: number, y: number): void
  /** Turn the wheel once at `x`, `y`, down or up. */
  wheel(x: number, y: number, down?: boolean | null): void
  /**
   * Move the pointer to `x`, `y`: the element it left gets its leave
   * message, the one it entered its hover message.
   */
  hover(x: number, y: number): void
  /** Move a held press to `x`, `y`. */
  drag(x: number, y: number): void
  release(): void
  /** Hand `event` to the app's `input`. */
  send(event: Event): void
  /** Type `text`, one key press per character. */
  typeText(text: string): void
  /** Whether the app asked to quit. */
  get quit(): boolean
}

/** Which of several controls of type `T` has the keyboard; Tab and Shift+Tab move it. */
export declare class Focus<T> {
  /** Focus over `controls`, the first one focused. */
  constructor(controls: readonly T[])
  current(): T
  is(control: T): boolean
  set(control: T): boolean
  next(): void
  previous(): void
  handle(event: Event): boolean
}

/**
 * The message of an app that acts in its callbacks: a callback the default
 * `update` calls, whose result decides what happens next, or nothing, when
 * a value callback like a slider's already did the work. Such apps extend
 * `App<Action>`.
 */
export type Action = (() => Control | undefined | void) | undefined | void

/** What `App.update` does: calls a function message and returns what it returns. */
export declare function defaultUpdate(message: unknown): Control | undefined | void

/**
 * A pixel UI program whose clicks, keys and timers send messages of type
 * `M`: state that shows itself as a view and changes on messages. Extend
 * it and pass an instance to `run`.
 */
export declare abstract class App<M = unknown> implements AppLike<M> {
  /** Keep `waker` to send messages later, like `waker.every(0.25, tick)`. */
  start(waker: Waker<M>): void
  /** The view of the current state. */
  abstract view(): Element<M>
  /** Apply `message`. A function message is called. Nothing redraws. */
  update(message: M): Control | undefined | void
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
  /** The mouse pressed, released, dragged, moved or turned its wheel at `at`, in the light buffer's pixels. */
  mouse(event: Event, at: Point): void
  /** The screen became the size of `light`, so baked art needs redoing. */
  resize(light: Light): void
  /** Advance the world by `dt` seconds, a fixed step. */
  step(keys: Keys, dt: number): void
  /** Draw the world into `light`. */
  render(light: Light): void
  /** Whether the next frame differs from the last one drawn. While it is false, `play` draws nothing and steps ten times a second. */
  changed(): boolean
  /** Whether the game is over, so `play` ends after this frame. */
  finished(): boolean
}

/** Draw `game` off the terminal and return the picture, for tests and screenshots. */
export declare function frame(game: GameLike, options?: FrameOptions): Image
