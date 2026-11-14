# katui guides

These guides walk you through katui one kind of program at a time. Each one
starts from a program you can run and builds on it. Start with the first
one, then pick whichever matches what you want to make.

1. [Getting started](getting-started.md): install katui, run a first app,
   and learn what happens when it starts.
2. [Pixel UI apps](ui-apps.md): views, messages, widgets, layout, keyboard
   input and background work.
3. [Games](games.md): a fixed-step game loop, drawing with light, and held
   keys.
4. [Video](video.md): play a file or a stream inside your app.
5. [HTML and markdown](html-and-markdown.md): render documents as part of
   your view.
6. [Images in ratatui apps](ratatui.md): keep your ratatui widgets and add
   pixels next to them.
7. [How katui works](architecture.md): the layers, and what happens
   between your `view` and the terminal.

## API reference

The API reference lists every type and function, generated from the code:
<https://matheusfillipe.github.io/katui/katui/>.

To build it yourself, run `just api-docs` in a clone and open
`target/doc/katui/index.html`.
