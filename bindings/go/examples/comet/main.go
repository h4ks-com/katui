// A comet you steer with the arrow keys, trailing sparks. Space bursts.
package main

import (
	"log"

	katui "github.com/h4ks-com/katui/bindings/go"
)

const speed = 90

var (
	tail = katui.SRGB(255, 140, 60)
	head = katui.SRGB(255, 230, 190)
)

type comet struct {
	at     katui.Point
	sparks *katui.Sparks
	rng    *katui.Rng
}

func newComet() *comet {
	return &comet{at: katui.Point{X: 60, Y: 40}, sparks: katui.NewSparks(), rng: katui.NewRng(7)}
}

func (c *comet) burst() {
	for range 40 {
		velocity := c.rng.Direction().Mul(c.rng.Range(20, 60))
		c.sparks.Add(katui.Spark{At: katui.V3{X: c.at.X, Y: c.at.Y}, Velocity: velocity, Color: tail, Life: 0.8, Width: 1.5})
	}
}

func (c *comet) Press(key string) {
	if key == " " {
		c.burst()
	}
}

func axis(keys *katui.Keys, less, more string) float32 {
	var value float32
	if keys.IsHeld(more) {
		value++
	}
	if keys.IsHeld(less) {
		value--
	}
	return value
}

func (c *comet) Step(keys *katui.Keys, dt float32) {
	c.at.X += speed * dt * axis(keys, "left", "right")
	c.at.Y += speed * dt * axis(keys, "up", "down")
	drift := c.rng.Direction().Mul(8)
	c.sparks.Add(katui.Spark{At: katui.V3{X: c.at.X, Y: c.at.Y}, Velocity: drift, Color: tail, Life: 0.5, Width: 1})
	c.sparks.Step(dt, katui.V3{})
}

func (c *comet) Render(light *katui.Light) {
	light.Dim(0.8)
	c.sparks.Draw(light, func(at katui.V3) (katui.Point, bool) {
		return katui.Point{X: at.X, Y: at.Y}, true
	})
	light.Glow(c.at, 9, katui.Scaled(tail, 0.35))
	light.Glow(c.at, 3, head)
	light.Text(katui.Point{X: 4, Y: 4}, "arrows to steer, space to burst", 1, katui.SRGB(120, 120, 140))
}

func main() {
	if err := katui.Play(newComet()); err != nil {
		log.Fatal(err)
	}
}
