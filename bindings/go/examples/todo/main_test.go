package main

import (
	"slices"
	"testing"

	katui "github.com/matheusfillipe/katui/bindings/go"
	"github.com/matheusfillipe/katui/bindings/go/internal/shots"
)

func press(t *testing.T, screen *katui.Screen, name string) {
	t.Helper()
	event, err := katui.Press(name)
	if err != nil {
		t.Fatal(err)
	}
	if err := screen.Send(event); err != nil {
		t.Fatal(err)
	}
}

func TestTodoAddsTypedTasksTicksAndFiltersThem(t *testing.T) {
	app := newTodo()
	screen, err := katui.NewScreen(app, 900, 480)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	if _, err := screen.Render(); err != nil {
		t.Fatal(err)
	}
	if err := screen.TypeText("buy bread"); err != nil {
		t.Fatal(err)
	}
	press(t, screen, "enter")
	if got := app.tasks[len(app.tasks)-1]; got != (task{"buy bread", false}) {
		t.Fatalf("the last task is %v", got)
	}
	press(t, screen, "tab")
	if !app.tasks[0].done {
		t.Fatal("tab ticks the selected task")
	}
	if err := screen.Click(335, 108); err != nil {
		t.Fatal(err)
	}
	if app.filter != 2 || !slices.Equal(app.shown(), []int{0, 1}) {
		t.Fatalf("the Done tab shows %v under filter %d", app.shown(), app.filter)
	}
	picture, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := picture.Save(shots.Path("todo")); err != nil {
		t.Fatal(err)
	}
}
