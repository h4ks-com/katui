// A to-do list: type a task, press Enter to add it, Tab to tick the selected one.
package main

import (
	"log"
	"strings"

	katui "github.com/h4ks-com/katui/bindings/go"
)

var filters = []string{"All", "Active", "Done"}

const visible = 10

type task struct {
	text string
	done bool
}

type todo struct {
	tasks     []task
	draft     *katui.TextField
	filter    int
	selection *katui.ListState
}

func newTodo() *todo {
	tasks := []task{{"water the plants", false}, {"fix the bike", true}}
	return &todo{
		tasks:     tasks,
		draft:     katui.NewTextField(""),
		selection: katui.NewListState(len(tasks), visible),
	}
}

func (t *todo) shown() []int {
	var indexes []int
	for index, task := range t.tasks {
		if t.filter == 0 || task.done == (t.filter == 2) {
			indexes = append(indexes, index)
		}
	}
	return indexes
}

func (t *todo) toggle(index int) {
	t.tasks[index].done = !t.tasks[index].done
}

func (t *todo) pickFilter(index int) {
	t.filter = index
	t.selection.Resize(len(t.shown()), visible)
}

func (t *todo) View() *katui.Element {
	shown := t.shown()
	done := 0
	for _, task := range t.tasks {
		if task.done {
			done++
		}
	}
	body := katui.Hint("Type a task and press Enter")
	if len(shown) > 0 {
		body = katui.List(t.selection, func(position int, selected bool) *katui.Element {
			index := shown[position]
			return katui.Checkbox(t.tasks[index].text, t.tasks[index].done, func() { t.toggle(index) })
		})
	}
	progress := float32(0)
	if len(t.tasks) > 0 {
		progress = float32(done) / float32(len(t.tasks))
	}
	card := katui.Panel(
		katui.Title("Tasks"),
		katui.Tabs(filters, t.filter, t.pickFilter),
		katui.TextFieldView(t.draft, "Add a task", true),
		body,
		katui.Progress(progress),
		katui.Hint("Press Up and Down to pick a task, Tab to toggle it"),
	).Width(katui.Px(katui.CurrentTheme().Columns(60)))
	return katui.Page(card).Align(katui.Center)
}

func (t *todo) Update(message any) katui.Control {
	return katui.Redraw
}

func (t *todo) Input(event *katui.Event) katui.Control {
	if t.selection.Handle(event) {
		return katui.Redraw
	}
	shown := t.shown()
	if event.Key() == "tab" && len(shown) > 0 {
		t.toggle(shown[min(t.selection.Selected(), len(shown)-1)])
		return katui.Redraw
	}
	edit := t.draft.Handle(event)
	if edit == katui.Submitted && strings.TrimSpace(t.draft.Text()) != "" {
		t.tasks = append(t.tasks, task{strings.TrimSpace(t.draft.Take()), false})
		t.selection.Resize(len(t.shown()), visible)
	}
	if edit == katui.Ignored {
		return katui.Wait
	}
	return katui.Redraw
}

func main() {
	if err := katui.Run(newTodo()); err != nil {
		log.Fatal(err)
	}
}
