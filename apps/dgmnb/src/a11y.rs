//! The frame's accessibility nodes → an AccessKit tree.

use accesskit::{
    Action, Live, Node as AkNode, NodeId, Rect as AkRect, Role, TextPosition, TextSelection,
    Toggled, TreeId, TreeInfo, TreeUpdate,
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
        let bounds = AkRect {
            x0: n.rect.x as f64 * s,
            y0: n.rect.y as f64 * s,
            x1: n.rect.right() as f64 * s,
            y1: n.rect.bottom() as f64 * s,
        };
        node.set_bounds(bounds);
        // What the Linux adapter actually exports (AT-SPI): a Label's name
        // is its value; a Group shows only its label; a text field offers
        // its text only through text runs. Shape the nodes for that. Ids
        // here are odd (`ui::id`), so `id - 1` is free for a node's one
        // synthetic child.
        let child = NodeId(n.id - 1);
        // Synthetic children: (id, node). Ids `id - 1`, `id - 3`, … are even,
        // so they can't collide with real (odd) ids.
        let mut extra: Vec<(NodeId, AkNode)> = Vec::new();
        match (n.role, &n.value) {
            _ if n.id < 1 << 20 => {} // too small to step down from safely
            (Role::Label, None) => node.set_value(n.label.as_str()),
            (Role::TextInput | Role::Document, Some(text)) => {
                // One run per line (a field has one); each covers its line
                // including the line break.
                let lines: Vec<&str> = if n.role == Role::Document {
                    text.split_inclusive('\n').collect()
                } else {
                    vec![text.as_str()]
                };
                for (k, line) in lines.iter().enumerate() {
                    let mut run = AkNode::new(Role::TextRun);
                    run.set_value(*line);
                    run.set_character_lengths(
                        line.chars()
                            .map(|c| c.len_utf8() as u8)
                            .collect::<Vec<u8>>(),
                    );
                    run.set_bounds(bounds);
                    let rid = NodeId(n.id - 1 - 2 * k as u64);
                    node.push_child(rid);
                    extra.push((rid, run));
                }
                if n.role == Role::TextInput {
                    if let Some((anchor, caret)) = n.text_selection {
                        let at = |i: usize| TextPosition {
                            node: child,
                            character_index: i,
                        };
                        node.set_text_selection(TextSelection {
                            anchor: at(anchor),
                            focus: at(caret),
                        });
                    }
                    node.add_action(Action::SetTextSelection);
                    node.add_action(Action::ReplaceSelectedText);
                    node.add_action(Action::SetValue);
                }
            }
            (Role::Group, Some(text)) if !children.contains_key(&n.id) => {
                // Results held as a group's value (function analysis):
                // the text goes in a label inside it.
                let mut label = AkNode::new(Role::Label);
                label.set_value(text.as_str());
                label.set_bounds(bounds);
                node.push_child(child);
                extra.push((child, label));
            }
            _ => {}
        }
        if let Some([value, min, max, step]) = n.numeric {
            // The Value interface reads these (a string value isn't enough).
            node.set_numeric_value(value);
            node.set_min_numeric_value(min);
            node.set_max_numeric_value(max);
            node.set_numeric_value_step(step);
            node.set_numeric_value_jump(step * 10.0);
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            node.add_action(Action::SetValue);
        }
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
        if let Some(mut kids) = children.remove(&n.id) {
            kids.extend(extra.iter().map(|(id, _)| *id));
            node.set_children(kids);
        }
        out.push((NodeId(n.id), node));
        out.extend(extra);
    }
    let focus = focus.filter(|f| seen.contains(f)).map_or(NodeId(0), NodeId);
    TreeUpdate {
        nodes: out,
        tree: Some(TreeInfo::new(NodeId(0))),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// For tests: what the Linux adapter exports for each node, as
/// (role, name, text). The name follows accesskit_atspi_common's rule (a
/// Label's name is its value, others use the label); `text` is what the
/// Text interface offers, if the node supports it.
#[cfg(test)]
pub fn exported(nodes: &[Node]) -> Vec<(Role, String, Option<String>)> {
    fn walk(n: accesskit_consumer::NodeRef, out: &mut Vec<(Role, String, Option<String>)>) {
        let name = if n.label_comes_from_value() {
            n.value()
        } else {
            n.label()
        };
        let text = n.supports_text_ranges().then(|| n.document_range().text());
        out.push((n.role(), name.unwrap_or_default(), text));
        for c in n.children() {
            walk(c, out);
        }
    }
    let tree = accesskit_consumer::Tree::new(tree(nodes, "test", None, 1.0), false);
    let mut out = Vec::new();
    walk(tree.state().root(), &mut out);
    out
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

    /// A panel of `rows` 40 px rows in a 200 px scroll view; the rows are
    /// buttons or plain content. Returns the view's hit and node, and
    /// whether the frame asked to be drawn again.
    fn panel(
        scrolls: &mut HashMap<crate::ui::Id, crate::ui::Scroll>,
        rows: usize,
        buttons: bool,
    ) -> (crate::ui::Hit, Node, bool) {
        let sid = crate::ui::id("panel");
        let mut pm = tiny_skia::Pixmap::new(400, 400).unwrap();
        let (mut text, mut icons, input) = (Text::new(), Icons::default(), Input::default());
        let mut f = Frame::new(
            Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            Theme::new(false, None),
            &input,
            scrolls,
            true,
        );
        let view = Rect::new(0.0, 0.0, 300.0, 200.0);
        let off = f.scroll_begin(sid, view, "Results");
        for i in 0..rows {
            let r = Rect::new(0.0, i as f32 * 40.0 - off, 300.0, 40.0);
            f.node(crate::ui::id(("row", i)), Role::Label, "result", r);
            if buttons {
                f.hit(
                    crate::ui::id(("row", i)),
                    r,
                    crate::ui::Sense::Click,
                    None,
                    true,
                );
            }
        }
        f.scroll_end(sid, view, rows as f32 * 40.0);
        let hit = f.hits.iter().find(|h| h.id == sid).unwrap().clone();
        let node = f
            .nodes
            .as_ref()
            .unwrap()
            .iter()
            .find(|n| n.id == sid)
            .unwrap()
            .clone();
        (hit, node, f.again)
    }

    /// Read-only content that overflows (analysis results, licences): its
    /// scroll view is a Tab stop with scroll actions from the first frame it
    /// overflows. Views whose rows take focus themselves aren't.
    #[test]
    fn overflowing_read_only_views_take_focus() {
        let mut scrolls = HashMap::new();
        let (hit, node, _) = panel(&mut scrolls, 3, false);
        assert!(!hit.focusable && !node.focusable && !node.scrollable);
        // Grows past the view: focusable and scrollable in that same frame.
        let (hit, node, again) = panel(&mut scrolls, 10, false);
        assert!(hit.focusable && node.focusable && node.scrollable);
        assert!(!again);
        // Focusable rows are the way in instead.
        let (hit, node, _) = panel(&mut scrolls, 10, true);
        assert!(!hit.focusable && !node.focusable && node.scrollable);
        // Shrinks while scrolled to the end: that frame used a stale
        // offset, so it asks for another, which is settled.
        scrolls.get_mut(&crate::ui::id("panel")).unwrap().offset = 200.0;
        let (_, _, again) = panel(&mut scrolls, 6, false);
        assert!(again);
        let (_, _, again) = panel(&mut scrolls, 6, false);
        assert!(!again);
    }

    /// R8-M-05: what Linux assistive technology receives, not just the raw
    /// tree: every label has a name (the date result was nameless), and
    /// text fields offer their text.
    #[test]
    fn linux_export_names_labels_and_offers_field_text() {
        let mut d = crate::date::DatePage::new();
        let (nodes, _) = frame_nodes(|f, r| d.view(f, r));
        let out = exported(&nodes);
        // Today minus today: the result label used to export no name.
        assert!(
            out.iter()
                .any(|(r, n, _)| *r == Role::Label && n.contains("Same dates"))
        );
        for (role, name, _) in &out {
            if *role == Role::Label {
                assert!(!name.is_empty(), "a nameless label");
            }
        }
        let mut g = crate::graph::GraphPage::for_test(appcore::graph::from_list("x^2"));
        let (nodes, _) = frame_nodes(|f, r| g.view(f, r));
        let fields: Vec<_> = exported(&nodes)
            .into_iter()
            .filter(|(r, ..)| *r == Role::TextInput)
            .collect();
        assert!(
            fields
                .iter()
                .any(|(_, _, text)| text.as_deref() == Some("x^2"))
        );
    }

    /// Pre-review: the licence viewer's text reaches screen readers (a
    /// Document offers text only through text runs), and the open
    /// calendar says which day is chosen.
    #[test]
    fn licences_are_readable_and_the_chosen_day_is_selected() {
        let (nodes, _) = frame_nodes(crate::app::draw_licences);
        let doc = exported(&nodes)
            .into_iter()
            .find(|(r, ..)| *r == Role::Document)
            .and_then(|(_, _, text)| text)
            .expect("licence text exported");
        assert!(doc.contains("MIT License") && doc.contains("SIL Open Font License"));

        let mut d = crate::date::DatePage::new();
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = crate::app::Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        d.update(crate::date::Msg::Calendar(Some(0)), &mut cx);
        let (nodes, _) = frame_nodes(|f, r| d.overlay(f, r));
        let chosen: Vec<_> = nodes
            .iter()
            .filter(|n| n.role == Role::ListBoxOption && n.selected == Some(true))
            .collect();
        assert_eq!(chosen.len(), 1, "exactly one day is the chosen one");
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
