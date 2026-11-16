package main

import (
	"testing"

	katui "github.com/h4ks-com/katui/bindings/go"
	"github.com/h4ks-com/katui/bindings/go/internal/shots"
)

func lit(picture *katui.Image) int {
	count := 0
	pixels := picture.RGBA()
	for at := 0; at < len(pixels); at += 4 {
		if pixels[at] > 40 {
			count++
		}
	}
	return count
}

func TestTheCometGlowsFollowsTheKeysAndBursts(t *testing.T) {
	game := newComet()
	still, err := katui.Frame(game, katui.FrameOptions{Width: 200, Height: 120})
	if err != nil {
		t.Fatal(err)
	}
	if lit(still) == 0 {
		t.Fatal("the comet glows")
	}
	game.Press(" ")
	if game.sparks.Len() <= 40 {
		t.Fatalf("a burst adds 40 sparks, now %d", game.sparks.Len())
	}
	moved, err := katui.Frame(game, katui.FrameOptions{Width: 200, Height: 120, Steps: 20, Held: []string{"right"}})
	if err != nil {
		t.Fatal(err)
	}
	if game.at.X <= 60 || game.at.Y != 40 {
		t.Fatalf("holding right moves the comet right only, to %v", game.at)
	}
	if err := moved.Save(shots.Path("comet")); err != nil {
		t.Fatal(err)
	}
	key, _ := katui.Press("a")
	game.Mouse(key, katui.Point{X: 5, Y: 6})
	if game.at.X <= 60 {
		t.Fatal("only a mouse press moves the comet")
	}
	if _, err := katui.Frame(game, katui.FrameOptions{Held: []string{"no such key"}}); err == nil {
		t.Fatal("an unknown key name fails")
	}
}

func TestScreenshot(t *testing.T) {
	game := newComet()
	if _, err := katui.Frame(game, katui.FrameOptions{Width: 360, Height: 224, Steps: 40, Held: []string{"right", "down"}}); err != nil {
		t.Fatal(err)
	}
	game.Press(" ")
	picture, err := katui.Frame(game, katui.FrameOptions{Width: 360, Height: 224, Steps: 20, Held: []string{"right"}})
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("example-comet")); err != nil {
		t.Fatal(err)
	}
}
