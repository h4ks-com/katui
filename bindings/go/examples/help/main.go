// A help screen written in Markdown, its links switching pages.
package main

import (
	"log"

	katui "github.com/matheusfillipe/katui/bindings/go"
)

var pages = map[string]string{
	"start": `# Help

Press **Ctrl+C** to quit. Read [the keys](keys) or [the credits](credits).

- Arrows move
- Enter opens
`,
	"keys":    "# Keys\n\n- **Up** and **Down** move\n- **Enter** opens\n\n[Back](start)",
	"credits": "# Credits\n\nDrawn by katui in real pixels.\n\n[Back](start)",
}

type help struct {
	current string
}

func (h *help) open(name string) {
	if _, ok := pages[name]; ok {
		h.current = name
	} else {
		h.current = "start"
	}
}

func (h *help) View() *katui.Element {
	return katui.Page(
		katui.Markdown(pages[h.current], katui.Document{OnLink: h.open}),
		katui.Hint("You are on the "+h.current+" page"),
	)
}

func (h *help) Update(message any) katui.Control {
	return katui.Redraw
}

func main() {
	if err := katui.Run(&help{current: "start"}); err != nil {
		log.Fatal(err)
	}
}
