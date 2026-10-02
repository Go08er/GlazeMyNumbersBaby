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
            node.add_action(Action::ScrollIntoView);
        }
        if n.scrollable {
            node.add_action(Action::ScrollUp);
            node.add_action(Action::ScrollDown);
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

    fn frame_nodes(draw: impl FnOnce(&mut Frame, Rect)) -> (Vec<Node>, Vec<crate::ui::Hit>) {
        frame_with(&mut HashMap::new(), draw)
    }

    fn frame_with(
        scrolls: &mut HashMap<crate::ui::Id, crate::ui::Scroll>,
        draw: impl FnOnce(&mut Frame, Rect),
    ) -> (Vec<Node>, Vec<crate::ui::Hit>) {
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) = (Text::new(), Icons::default(), Input::default());
        let canvas = Canvas::new(pm.as_mut(), 1.0, false);
        let mut f = Frame::new(
            canvas,
            &mut text,
            &mut icons,
            Theme::new(false, None),
            &input,
            scrolls,
            true,
        );
        draw(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
        (f.nodes.take().unwrap(), std::mem::take(&mut f.hits))
    }

    fn press(p: &mut crate::calc::CalcPage, keys: &str) {
        for k in appcore::input::parse_key_script(keys) {
            let mut toasts = Vec::new();
            let mut focus = None;
            let mut cx = crate::app::Cx {
                toasts: &mut toasts,
                clipboard: None,
                wide: true,
                focus: &mut focus,
            };
            p.key(&k, &mut cx);
        }
    }

    /// The round-3 review's sequence: 7, =, 9.
    #[test]
    fn display_reports_current_value_and_announces_results() {
        let mut p = crate::calc::CalcPage::new(None);
        let label = |p: &mut crate::calc::CalcPage| {
            let (nodes, _) = frame_nodes(|f, r| p.view(f, r, false));
            let d = nodes
                .iter()
                .find(|n| n.id == crate::ui::id("display"))
                .unwrap();
            let a = nodes
                .iter()
                .find(|n| n.id == crate::ui::id("announcer"))
                .unwrap();
            (d.value.clone().unwrap(), a.value.clone().unwrap())
        };
        press(&mut p, "7");
        assert_eq!(label(&mut p), ("Display is 7".into(), String::new()));
        press(&mut p, "=");
        assert_eq!(
            label(&mut p),
            ("Display is 7".into(), "Display is 7".into())
        );
        press(&mut p, "9");
        assert_eq!(label(&mut p).0, "Display is 9");
    }

    /// Every actionable node (including history rows scrolled out of view)
    /// has a matching hit, so AT actions and Tab can reach it.
    #[test]
    fn scrolled_controls_stay_actionable() {
        let mut p = crate::calc::CalcPage::new(None);
        for _ in 0..20 {
            press(&mut p, "1+1=");
        }
        let (nodes, hits) = frame_nodes(|f, r| p.view(f, r, false));
        let clickable: Vec<_> = nodes.iter().filter(|n| n.clickable).collect();
        let hidden = hits.iter().filter(|h| !h.visible).count();
        assert!(hidden >= 20, "expected scrolled-out rows, got {hidden}");
        for n in clickable {
            let h = hits.iter().find(|h| h.id == n.id);
            assert!(
                h.is_some_and(|h| h.msg.is_some() || h.focusable),
                "no action for {:?}",
                n.label
            );
        }
        // Off-screen ones know their scroll area, so focusing reveals them.
        assert!(
            hits.iter()
                .filter(|h| !h.visible)
                .all(|h| h.scroll.is_some())
        );
    }

    /// Focusing each history row in turn (as Tab does) scrolls it into view.
    #[test]
    fn focusing_reveals_every_history_row() {
        let mut p = crate::calc::CalcPage::new(None);
        for _ in 0..20 {
            press(&mut p, "1+1=");
        }
        let mut scrolls = HashMap::new();
        let rows: Vec<crate::ui::Id> = (0usize..20).map(|i| crate::ui::id(("hist", i))).collect();
        for row in rows {
            let (_, hits) = frame_with(&mut scrolls, |f, r| p.view(f, r, false));
            let h = hits.iter().find(|h| h.id == row).expect("row recorded");
            let (sid, view) = h.scroll.expect("in a scroll area");
            let s = scrolls.get_mut(&sid).unwrap();
            s.offset = s.revealing(h.full, view);
            let (_, hits) = frame_with(&mut scrolls, |f, r| p.view(f, r, false));
            let h = hits.iter().find(|h| h.id == row).unwrap();
            assert!(h.visible, "row {row} still hidden after reveal");
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
        let mut g =
            crate::graph::GraphPage::for_test(appcore::graph::from_list("x^2;y<sin(x);a*x"));
        check(|f, r| g.view(f, r));
    }
}
