package main

import (
	"testing"

	katui "github.com/matheusfillipe/katui/bindings/go"
	"github.com/matheusfillipe/katui/bindings/go/internal/shots"
)

func TestTheCounterCountsClicks(t *testing.T) {
	app := &counter{}
	screen, err := katui.NewScreen(app, 480, 240)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	picture, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("counter")); err != nil {
		t.Fatal(err)
	}
	for range 2 {
		if err := screen.Click(240, 72); err != nil {
			t.Fatal(err)
		}
		if _, err := screen.Render(); err != nil {
			t.Fatal(err)
		}
	}
	if app.count != 2 {
		t.Fatalf("counted %d clicks, want 2", app.count)
	}
}

func TestScreenshot(t *testing.T) {
	screen, err := katui.NewScreen(&counter{count: 3}, 900, 560)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	picture, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("example-counter")); err != nil {
		t.Fatal(err)
	}
}
