# katui-game examples

These are games and animations built on katui-game, which steps a game at a fixed rate and draws its light buffer as real pixels. The `support/` folder holds a helper the tests share.

Every demo here quits with q, Esc or Ctrl+C.

### cube

A lit cube spinning in software 3D, with a glow around its edges.

Run it with `just demo-cube`. Hold the arrow keys to steer its spin.

<img src="screenshots/cube.webp" alt="A red, green and white cube glowing against a dark sky" width="720">

### dvd

The bouncing logo over a twinkling starfield. It changes color and speeds up each time it hits a corner.

Run it with `just demo-dvd`.

<img src="screenshots/dvd.webp" alt="A red KATUI! logo with a motion trail over a purple starfield" width="720">

### life

Conway's game of life with cells that glow by age and leave fading ash. A board that stalls seeds itself again.

Run it with `just demo-life`.

<img src="screenshots/life.webp" alt="Two colonies of glowing red and white cells on a dark nebula" width="720">

### lumenshaft

A falling-block game in a 3D shaft, with pieces of three to five cells and sparks when a row clears.

Run it with `just demo-lumenshaft`. Press Enter to start, Left and Right to move, Up to turn, hold Down to fall faster, Space to drop and p to pause.

<img src="screenshots/lumenshaft.webp" alt="A glowing 3D shaft with stacked colored blocks, a falling piece and the score beside it" width="720">

### plasma

An interference plasma that cycles from deep blue through orange to pink.

Run it with `just demo-plasma`.

<img src="screenshots/plasma.webp" alt="Soft waves of orange and purple across the screen" width="720">

### shardrift

An asteroids-style game: fly a ship through a nebula and shoot crystal rocks into smaller shards.

Run it with `just demo-shardrift`. Press Enter to start, Left and Right or a and d to turn, Up and Down or w and s to fly, Space or f to fire and p to pause.

<img src="screenshots/shardrift.webp" alt="A ship firing at purple and teal crystal rocks in a starry nebula" width="720">

### snake

A snake that plays itself on a bordered grid.

Run it with `just demo-snake`.

<img src="screenshots/snake.webp" alt="A long green snake heading for a red apple on a dark grid" width="720">

### video

A video player that decodes a file or stream URL with ffmpeg and plays its sound with ffplay. Without an argument it plays ffmpeg's beeping test pattern, so it needs ffmpeg installed either way.

Run it with `just demo-video`, or `just demo-video clip.mp4` for a file. Press Space to pause, Left and Right to seek five seconds and m to mute.

<img src="screenshots/video.webp" alt="The ffmpeg color bar test pattern, paused, with a progress bar along the bottom" width="720">
