//! A tree view: nodes that open to show their children, one of them
//! selected.

use std::collections::HashSet;

use katui_proto::input::{InputEvent, Key};
use katui_ui::{Align, Element, Length};

use crate::theme::Theme;
use crate::view::styled;

/// One node of a tree: an id unique in the tree, its text and its
/// children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// The id the tree's messages carry.
    pub id: u64,
    /// The text shown.
    pub text: String,
    /// The nodes under this one.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// A node without children.
    #[must_use]
    pub fn leaf(id: u64, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            children: Vec::new(),
        }
    }

    /// A node holding `children`.
    #[must_use]
    pub fn branch(id: u64, text: impl Into<String>, children: Vec<Self>) -> Self {
        Self {
            id,
            text: text.into(),
            children,
        }
    }
}

/// Which nodes are open and which one is selected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeState {
    open: HashSet<u64>,
    selected: Option<u64>,
}

/// One visible row: its node and how deep it sits.
struct Row<'a> {
    node: &'a TreeNode,
    depth: usize,
}

impl TreeState {
    /// The selected node's id.
    #[must_use]
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    /// Select the node `id`.
    pub fn select(&mut self, id: u64) {
        self.selected = Some(id);
    }

    /// Whether node `id` shows its children.
    #[must_use]
    pub fn is_open(&self, id: u64) -> bool {
        self.open.contains(&id)
    }

    /// Open node `id` if it is closed, close it if it is open.
    pub fn toggle(&mut self, id: u64) {
        if !self.open.remove(&id) {
            self.open.insert(id);
        }
    }

    /// Move through the visible nodes of `roots` on the up and down
    /// arrows, open the selected node on the right arrow and close it on
    /// the left one; `true` when `event` was one of them.
    pub fn handle(&mut self, event: &InputEvent, roots: &[TreeNode]) -> bool {
        let InputEvent::Key(key, _) = event else {
            return false;
        };
        let rows = self.rows(roots);
        let at = rows
            .iter()
            .position(|row| Some(row.node.id) == self.selected);
        let pick = |index: usize| rows.get(index).map(|row| row.node.id);
        let next = match (key, at) {
            (Key::Up, Some(at)) => pick(at.saturating_sub(1)),
            (Key::Down, Some(at)) => pick((at + 1).min(rows.len().saturating_sub(1))),
            (Key::Up | Key::Down, None) => pick(0),
            (Key::Right, Some(at)) => {
                let id = pick(at);
                if let Some(id) = id {
                    self.open.insert(id);
                }
                id
            }
            (Key::Left, Some(at)) => {
                let id = pick(at);
                if let Some(id) = id {
                    self.open.remove(&id);
                }
                id
            }
            _ => return false,
        };
        self.selected = next.or(self.selected);
        true
    }

    fn rows<'a>(&self, roots: &'a [TreeNode]) -> Vec<Row<'a>> {
        let mut rows = Vec::new();
        let mut stack: Vec<(&TreeNode, usize)> = roots.iter().rev().map(|node| (node, 0)).collect();
        while let Some((node, depth)) = stack.pop() {
            rows.push(Row { node, depth });
            if self.is_open(node.id) {
                stack.extend(node.children.iter().rev().map(|child| (child, depth + 1)));
            }
        }
        rows
    }
}

/// The visible nodes of `roots`, indented by depth, the selected one
/// highlighted. A click on a node's marker sends `toggle(id)`, and a click
/// on the node sends `select(id)`.
pub fn tree<M>(
    roots: &[TreeNode],
    state: &TreeState,
    toggle: impl Fn(u64) -> M,
    select: impl Fn(u64) -> M,
) -> Element<M> {
    let theme = Theme::current();
    let rows = state.rows(roots).into_iter().map(|row| {
        let node = row.node;
        let marker = if node.children.is_empty() {
            Element::row([]).width(theme.columns(1.5))
        } else {
            let sign = if state.is_open(node.id) { "−" } else { "+" };
            Element::row([styled(sign, theme.body(), theme.muted).weight(700)])
                .width(theme.columns(1.5))
                .justify(Align::Center)
                .on_click(toggle(node.id))
        };
        let indent = Element::row([]).width(theme.columns(2.0 * depth_cells(row.depth)));
        let line = Element::row([
            indent,
            marker,
            styled(node.text.clone(), theme.body(), theme.text),
        ])
        .align(Align::Center)
        .padding_xy(theme.cell_width() * 0.5, theme.cell_height() * 0.15)
        .radius(theme.cell_height() * 0.3)
        .width(Length::Fill)
        .on_click(select(node.id));
        if state.selected == Some(node.id) {
            line.background(theme.highlight.clone())
        } else {
            line
        }
    });
    Element::column(rows).width(Length::Fill)
}

fn depth_cells(depth: usize) -> f32 {
    f32::from(u16::try_from(depth).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::{TreeNode, TreeState};

    fn roots() -> Vec<TreeNode> {
        vec![
            TreeNode::branch(
                1,
                "src",
                vec![TreeNode::leaf(2, "main.rs"), TreeNode::leaf(3, "lib.rs")],
            ),
            TreeNode::leaf(4, "Cargo.toml"),
        ]
    }

    fn press(state: &mut TreeState, key: Key) -> bool {
        state.handle(&InputEvent::Key(key, Mods::NONE), &roots())
    }

    #[test]
    fn arrows_walk_the_visible_nodes_and_open_and_close_them() {
        let mut state = TreeState::default();
        assert!(press(&mut state, Key::Down));
        assert_eq!(state.selected(), Some(1), "the first press picks the top");
        press(&mut state, Key::Down);
        assert_eq!(
            state.selected(),
            Some(4),
            "a closed branch hides its children"
        );
        press(&mut state, Key::Up);
        press(&mut state, Key::Right);
        assert!(state.is_open(1));
        press(&mut state, Key::Down);
        assert_eq!(state.selected(), Some(2), "an open branch shows them");
        press(&mut state, Key::Up);
        press(&mut state, Key::Left);
        assert!(!state.is_open(1));
        assert!(!press(&mut state, Key::Char('x')));
    }
}
