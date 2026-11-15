// A click counter, the smallest katui program.
package main

import (
	"fmt"
	"log"

	katui "github.com/h4ks-com/katui/bindings/go"
)

type counter struct {
	count int
}

func (c *counter) View() *katui.Element {
	return katui.Panel(
		katui.Label(fmt.Sprintf("Clicked %d times", c.count)),
		katui.PrimaryButton("Add one", c.add),
	).Align(katui.Center)
}

func (c *counter) Update(message any) katui.Control {
	return katui.Redraw
}

func (c *counter) add() {
	c.count++
}

func main() {
	if err := katui.Run(&counter{}); err != nil {
		log.Fatal(err)
	}
}
