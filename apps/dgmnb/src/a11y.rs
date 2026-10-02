//! The frame's accessibility nodes → an AccessKit tree.

use accesskit::{
    Action, Live, Node as AkNode, NodeId, Rect as AkRect, Role, Toggled, TreeId, TreeInfo,
    TreeUpdate,
};

use crate::ui::{Id, Node};

pub fn tree(nodes: &[Node], title: &str, focus: Option<Id>, scale: f64) -> TreeUpdate {
    let ids: std::collections::HashSet<Id> = nodes.iter().map(|n| n.id).collect();
    let mut children: std::collections::HashMap<Id, Vec<NodeId>> = Default::default();
    let mut listed = std::collections::HashSet::new();
    for n in nodes {
        if !listed.insert(n.id) {
            continue;
        }
        // A missing parent would orphan the node; hang it off the window.
        let parent = if ids.contains(&n.parent) { n.parent } else { 0 };
        children.entry(parent).or_default().push(NodeId(n.id));
    }
    let mut out = Vec::with_capacity(nodes.len() + 1);
    let mut root = AkNode::new(Role::Window);
    root.set_label(title);
    root.set_children(children.remove(&0).unwrap_or_default());
    out.push((NodeId(0), root));
    let mut seen = std::collections::HashSet::new();
    for n in nodes {
        // Ids are unique per frame by construction; skip accidental repeats
        // rather than hand AccessKit an inconsistent tree.
        if !seen.insert(n.id) {
            continue;
        }
        let mut node = AkNode::new(n.role);
        node.set_label(n.label.as_str());
        if let Some(v) = &n.value {
            node.set_value(v.as_str());
        }
        let s = scale;
        node.set_bounds(AkRect {
            x0: n.rect.x as f64 * s,
            y0: n.rect.y as f64 * s,
            x1: n.rect.right() as f64 * s,
            y1: n.rect.bottom() as f64 * s,
        });
        if let Some(t) = n.toggled {
            node.set_toggled(if t { Toggled::True } else { Toggled::False });
        }
        if let Some(sel) = n.selected {
            node.set_selected(sel);
        }
        if n.disabled {
            node.set_disabled();
        }
        if n.live {
            node.set_live(Live::Polite);
        }
        if n.clickable {
            node.add_action(Action::Click);
        }
        if n.focusable {
            node.add_action(Action::Focus);
        }
        if let Some(kids) = children.remove(&n.id) {
            node.set_children(kids);
        }
        out.push((NodeId(n.id), node));
    }
    let focus = focus.filter(|f| seen.contains(f)).map_or(NodeId(0), NodeId);
    TreeUpdate {
        nodes: out,
        tree: Some(TreeInfo::new(NodeId(0))),
        tree_id: TreeId::ROOT,
        focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{Canvas, Rect};
    use crate::text::Text;
    use crate::theme::Theme;
    use crate::ui::{Frame, Icons, Input};
    use std::collections::{HashMap, HashSet};

    /// Render a page with accessibility on and check the tree is sound:
    /// unique ids, every node reachable from the window, named controls.
    fn check(draw: impl FnOnce(&mut Frame, Rect)) {
        let mut pm = tiny_skia::Pixmap::new(900, 700).unwrap();
        let (mut text, mut icons, input, mut scrolls) = (
            Text::new(),
            Icons::default(),
            Input::default(),
            HashMap::new(),
        );
        let nodes = {
            let canvas = Canvas::new(pm.as_mut(), 1.0, false);
            let mut f = Frame::new(
                canvas,
                &mut text,
                &mut icons,
                Theme::new(false, None),
                &input,
                &mut scrolls,
                true,
            );
            draw(&mut f, Rect::new(0.0, 46.0, 900.0, 654.0));
            f.nodes.take().unwrap()
        };
        assert!(!nodes.is_empty());
        let update = tree(&nodes, "test", None, 1.0);
        let mut ids = HashSet::new();
        for (id, _) in &update.nodes {
            assert!(ids.insert(*id), "duplicate node {id:?}");
        }
        let by_id: HashMap<_, _> = update.nodes.iter().map(|(i, n)| (*i, n)).collect();
        let mut seen = HashSet::new();
        let mut stack = vec![NodeId(0)];
        while let Some(i) = stack.pop() {
            assert!(seen.insert(i), "cycle at {i:?}");
            stack.extend(by_id[&i].children().iter().copied());
        }
        assert_eq!(seen.len(), update.nodes.len(), "unreachable nodes");
        for (_, n) in &update.nodes {
            if n.role() == Role::Button {
                assert!(n.label().is_some_and(|l| !l.is_empty()), "unnamed button");
            }
        }
    }

    #[test]
    fn every_page_builds_a_sound_tree() {
        for mode in [
            calcvm::CalcMode::Standard,
            calcvm::CalcMode::Scientific,
            calcvm::CalcMode::Programmer,
        ] {
            let mut p = crate::calc::CalcPage::new(None);
            p.set_mode(mode);
            check(|f, r| p.view(f, r, false));
        }
        let mut d = crate::date::DatePage::new();
        check(|f, r| d.view(f, r));
        let mut g = crate::graph::GraphPage::new(appcore::graph::from_list("x^2;y<sin(x);a*x"));
        check(|f, r| g.view(f, r));
    }
}
