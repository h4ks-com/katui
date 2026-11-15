package main

import (
	"testing"

	katui "github.com/matheusfillipe/katui/bindings/go"
	"github.com/matheusfillipe/katui/bindings/go/internal/shots"
)

func TestHelpLinksSwitchPages(t *testing.T) {
	app := &help{current: "start"}
	screen, err := katui.NewScreen(app, 700, 400)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	picture, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("help")); err != nil {
		t.Fatal(err)
	}
	for y := float32(40); y < 120 && app.current == "start"; y += 4 {
		for x := float32(0); x < 700 && app.current == "start"; x += 8 {
			if err := screen.Click(x, y); err != nil {
				t.Fatal(err)
			}
		}
	}
	if app.current == "start" {
		t.Fatal("no link on the start page switched pages")
	}
	keys, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := keys.Save(shots.Path("help-" + app.current)); err != nil {
		t.Fatal(err)
	}
}

func TestScreenshot(t *testing.T) {
	screen, err := katui.NewScreen(&help{current: "start"}, 900, 560)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	picture, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("example-help")); err != nil {
		t.Fatal(err)
	}
}
