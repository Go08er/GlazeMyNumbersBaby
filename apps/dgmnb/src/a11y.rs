//! The frame's accessibility nodes → an AccessKit tree.

use accesskit::{
    Action, Live, Node as AkNode, NodeId, Rect as AkRect, Role, TextPosition, TextSelection,
    Toggled, TreeId, TreeInfo, TreeUpdate,
};

use crate::ui::{Hit, Id, Node, Sense};

/// The control an assistive-technology request reaches, if any: only one
/// in the topmost modal layer, and only for an action it takes. A target
/// that an overlay covers, or that is gone from the frame (a stale
/// reference), gets nothing.
pub fn target(hits: &[Hit], id: Id, action: Action) -> Option<&Hit> {
    let h = crate::ui::active_layer(hits)
        .iter()
        .rev()
        .find(|h| h.id == id)?;
    let fits = match action {
        Action::Click => h.msg.is_some() || h.sense == Sense::Text,
        Action::Focus => h.focusable,
        Action::ScrollIntoView => true,
        Action::SetTextSelection | Action::ReplaceSelectedText => h.sense == Sense::Text,
        Action::SetValue => matches!(h.sense, Sense::Text | Sense::Drag),
        Action::Increment | Action::Decrement => h.sense == Sense::Drag,
        Action::ScrollUp | Action::ScrollDown => h.sense == Sense::Scroll,
        _ => false,
    };
    fits.then_some(h)
}

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
        if let Some((text, by)) = &n.description {
            // The Linux adapter exports the text (AT-SPI's Description);
            // the relation is for the other platforms.
            node.set_description(text.as_str());
            if let Some(by) = by.filter(|by| ids.contains(by)) {
                node.set_described_by(vec![NodeId(by)]);
            }
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
        // What the app hands AccessKit: the topmost modal layer's nodes.
        (f.take_nodes().unwrap(), std::mem::take(&mut f.hits))
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
        assert!(doc.contains("CORE-MATH"), "vendored CORE-MATH's notice");

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

    /// The date pickers' calendar by ear and keys: opened, the focus is on
    /// the chosen day under a heading naming the month; the keys move it,
    /// into other months too; one day takes Tab.
    #[test]
    fn the_calendar_is_read_and_moved_by_keys() {
        use appcore::input::{KeyPress, Named};
        let today = chrono::Local::now().date_naive();
        let mut d = crate::date::DatePage::new();
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = crate::app::Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        d.update(crate::date::Msg::Calendar(Some(1)), &mut cx);
        let read = |d: &mut crate::date::DatePage, focus: Option<Id>| {
            let (nodes, hits) = frame_nodes(|f, r| d.overlay(f, r));
            let heading: Vec<_> = nodes
                .iter()
                .filter(|n| n.role == Role::Heading)
                .map(|n| n.label.clone())
                .collect();
            let tab: Vec<_> = nodes
                .iter()
                .filter(|n| n.role == Role::ListBoxOption && n.focusable)
                .map(|n| (n.id, n.label.clone()))
                .collect();
            assert_eq!(tab.len(), 1, "one day takes Tab");
            assert_eq!(Some(tab[0].0), focus, "the focus is on it");
            assert!(hits.iter().any(|h| h.id == tab[0].0 && h.focusable));
            (heading, tab[0].1.clone())
        };
        let long = |d: chrono::NaiveDate| datecalc::format_long_date(&datecalc::utc_midnight(d));
        let (heading, day) = read(&mut d, *cx.focus);
        assert_eq!(heading, [today.format("%B %Y").to_string()]);
        assert_eq!(day, long(today));
        for (key, to) in [
            (KeyPress::named(Named::Right), today.succ_opt().unwrap()),
            (
                KeyPress::named(Named::PageDown),
                today
                    .succ_opt()
                    .unwrap()
                    .checked_add_months(chrono::Months::new(1))
                    .unwrap(),
            ),
        ] {
            assert!(d.key(&key, &mut cx));
            let (heading, day) = read(&mut d, *cx.focus);
            assert_eq!(heading, [to.format("%B %Y").to_string()]);
            assert_eq!(day, long(to));
        }
    }

    /// Run `m` through a page's `update` with a throwaway context.
    fn with_cx(run: impl FnOnce(&mut crate::app::Cx)) {
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = crate::app::Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        run(&mut cx);
    }

    fn full() -> Rect {
        Rect::new(0.0, 0.0, 760.0, 700.0)
    }

    /// Every action assistive technology can request.
    const ACTIONS: [Action; 10] = [
        Action::Click,
        Action::Focus,
        Action::ScrollIntoView,
        Action::SetValue,
        Action::SetTextSelection,
        Action::ReplaceSelectedText,
        Action::Increment,
        Action::Decrement,
        Action::ScrollUp,
        Action::ScrollDown,
    ];

    /// Every action an exported node advertises reaches it.
    fn check_actions(what: &str, nodes: &[Node], hits: &[crate::ui::Hit]) {
        for n in nodes {
            for (on, a) in [
                (n.clickable, Action::Click),
                (n.focusable, Action::Focus),
                (n.focusable, Action::ScrollIntoView),
                (n.scrollable, Action::ScrollUp),
                (n.scrollable, Action::ScrollDown),
                (n.numeric.is_some(), Action::Increment),
                (n.numeric.is_some(), Action::SetValue),
                (n.role == Role::TextInput, Action::SetTextSelection),
                (n.role == Role::TextInput, Action::SetValue),
            ] {
                assert!(
                    !on || target(hits, n.id, a).is_some(),
                    "{what}: {:?} doesn't take the {a:?} it offers",
                    n.label
                );
            }
        }
    }

    /// `draw(f, r, page, overlay)` draws a page, an overlay, or the overlay
    /// over the page. Over the page, nothing the overlay covers is exported
    /// (announcers aside) or answers any action, the overlay's own controls
    /// answer the actions they advertise, and the tree is sound.
    fn check_modal(what: &str, mut draw: impl FnMut(&mut Frame, Rect, bool, bool)) {
        let (under, _) = frame_nodes(|f, r| draw(f, r, true, false));
        let (alone, _) = frame_nodes(|f, r| draw(f, r, false, true));
        let (nodes, hits) = frame_nodes(|f, r| draw(f, r, true, true));
        let own: HashSet<Id> = alone.iter().map(|n| n.id).collect();
        let covered: Vec<&Node> = under.iter().filter(|n| !own.contains(&n.id)).collect();
        assert!(
            covered.iter().any(|n| n.clickable),
            "{what}: covers no control"
        );
        for n in covered {
            let kept = nodes.iter().find(|m| m.id == n.id);
            if n.role == Role::Status && n.live {
                assert!(kept.is_some_and(|m| m.parent == 0), "{what}: announcer");
                continue;
            }
            assert!(kept.is_none(), "{what}: covered {:?} exported", n.label);
            for a in ACTIONS {
                assert!(
                    target(&hits, n.id, a).is_none(),
                    "{what}: covered {:?} takes {a:?}",
                    n.label
                );
            }
        }
        check_actions(what, &nodes, &hits);
        let update = tree(&nodes, "test", None, 1.0);
        let by_id: HashMap<_, _> = update.nodes.iter().map(|(i, n)| (*i, n)).collect();
        let mut seen = HashSet::new();
        let mut stack = vec![NodeId(0)];
        while let Some(i) = stack.pop() {
            assert!(seen.insert(i), "{what}: cycle at {i:?}");
            stack.extend(by_id[&i].children().iter().copied());
        }
        assert_eq!(seen.len(), update.nodes.len(), "{what}: unreachable nodes");
    }

    /// R12-M-10, the review's case: the navigation open over 7 + 8. Only
    /// the navigation is exported; Clear, under it, can't be clicked (looked
    /// up afresh or from a stale reference), focused or scrolled to.
    #[test]
    fn the_navigation_covers_the_calculator_for_assistive_technology() {
        let mut p = crate::calc::CalcPage::new(None);
        press(&mut p, "7+8");
        let (page, page_hits) = frame_nodes(|f, r| p.view(f, r, false));
        let clear = page
            .iter()
            .find(|n| n.label == "Clear (Esc)")
            .expect("the Clear key")
            .id;
        assert!(target(&page_hits, clear, Action::Click).is_some());

        let (nodes, hits) = frame_nodes(|f, r| {
            p.view(f, r, false);
            crate::app::draw_nav(f, full(), appcore::modes::ViewMode::Standard, false);
        });
        let out = exported(&nodes);
        for (_, name, _) in &out {
            assert!(
                !["Clear (Esc)", "Display is 8", "Expression is 7 + "].contains(&name.as_str()),
                "{name:?} is under the navigation"
            );
        }
        assert!(out.iter().any(|(_, name, _)| name == "Close Navigation"));
        for a in ACTIONS {
            assert!(
                target(&hits, clear, a).is_none(),
                "covered Clear takes {a:?}"
            );
        }
        let close = target(&hits, crate::ui::id("nav-close"), Action::Click).expect("close");
        assert_eq!(close.msg, Some(crate::app::Msg::Nav(false)));
        // Gone from the frame altogether, or asked for what it doesn't do.
        assert!(target(&hits, crate::ui::id("no such control"), Action::Click).is_none());
        assert!(target(&hits, crate::ui::id("nav-close"), Action::Increment).is_none());
        // Results are still announced, from the window.
        let announcer = nodes
            .iter()
            .find(|n| n.id == crate::ui::id("announcer"))
            .expect("the announcer");
        assert_eq!(announcer.parent, 0);
        // Closed again, Clear is back.
        let (_, hits) = frame_nodes(|f, r| p.view(f, r, false));
        assert!(target(&hits, clear, Action::Click).is_some());
    }

    /// R12-M-10, every other overlay: the licences, the calculator's
    /// flyouts, sheet and display menu, the unit picker, the calendar, and
    /// the graph's style and window popups.
    #[test]
    fn every_overlay_is_all_assistive_technology_reaches() {
        let mut p = crate::calc::CalcPage::new(None);
        press(&mut p, "7+8");
        check_modal("navigation", |f, r, page, over| {
            if page {
                p.view(f, r, false);
            }
            if over {
                crate::app::draw_nav(f, full(), appcore::modes::ViewMode::Standard, false);
            }
        });
        check_modal("licences", |f, r, page, over| {
            if page {
                p.view(f, r, false);
            }
            if over {
                crate::app::draw_licences(f, full());
            }
        });
        use crate::calc::Popup;
        for (mode, popup) in [
            (calcvm::CalcMode::Standard, Popup::Panel),
            (calcvm::CalcMode::Standard, Popup::DisplayMenu(600.0, 200.0)),
            (calcvm::CalcMode::Scientific, Popup::Trig),
            (calcvm::CalcMode::Scientific, Popup::Functions),
            (calcvm::CalcMode::Programmer, Popup::Bitwise),
            (calcvm::CalcMode::Programmer, Popup::Shift),
        ] {
            let mut p = crate::calc::CalcPage::new(None);
            p.set_mode(mode);
            press(&mut p, "7+8");
            p.popup = Some(popup);
            check_modal(&format!("{popup:?}"), |f, r, page, over| {
                if page {
                    p.view(f, r, false);
                }
                if over {
                    p.overlay(f, r);
                }
            });
        }

        let mut c = crate::conv::ConvPage::new(None);
        with_cx(|cx| c.update(crate::conv::Msg::Units(Some(1)), cx));
        check_modal("unit picker", |f, r, page, over| {
            if page {
                c.view(f, r);
            }
            if over {
                c.overlay(f, r);
            }
        });

        let mut d = crate::date::DatePage::new();
        with_cx(|cx| d.update(crate::date::Msg::Calendar(Some(0)), cx));
        check_modal("calendar", |f, r, page, over| {
            if page {
                d.view(f, r);
            }
            if over {
                d.overlay(f, r);
            }
        });

        let mut g = crate::graph::GraphPage::for_test(appcore::graph::from_list("x^2;sin(x)"));
        let (_, hits) = frame_nodes(|f, r| g.view(f, r));
        let style = hits
            .iter()
            .find_map(|h| match &h.msg {
                Some(crate::app::Msg::Graph(crate::graph::Msg::StylePopup(Some(e)))) => Some(*e),
                _ => None,
            })
            .expect("an equation's style button");
        for m in [
            crate::graph::Msg::StylePopup(Some(style)),
            crate::graph::Msg::SettingsPopup(true),
        ] {
            let what = format!("{m:?}");
            with_cx(|cx| g.update(m, cx));
            check_modal(&what, |f, r, page, over| {
                if page {
                    g.view(f, r);
                }
                if over {
                    g.overlay(f, r);
                }
            });
        }
    }

    /// R13-L-06: Settings exports what it shows: its headings, the About
    /// card's name, version and description, and, when the desktop shares
    /// no accent colour, why the accent switch is greyed out (as text, and
    /// as the disabled switch's description).
    #[test]
    fn settings_exports_its_text_and_why_a_setting_is_off() {
        let settings = crate::app::Settings::default();
        for accent in [None, Some([0.2, 0.4, 0.8])] {
            let desktop = crate::app::Desktop {
                accent,
                ..Default::default()
            };
            let draw = |f: &mut Frame, r: Rect| {
                crate::app::draw_settings(f, r, &settings, desktop);
            };
            check(draw);
            let (nodes, hits) = frame_nodes(draw);
            check_actions("settings", &nodes, &hits);
            let names: Vec<String> = exported(&nodes).into_iter().map(|(_, n, _)| n).collect();
            for shown in [
                "Appearance",
                "Style",
                "About",
                "DGMNB",
                "Don't Glaze My Numbers, Baby",
                &format!("Version {}", env!("CARGO_PKG_VERSION")),
            ] {
                assert!(names.iter().any(|n| n == shown), "{shown:?} not exported");
            }
            assert!(
                names
                    .iter()
                    .any(|n| n.starts_with("The lean twin of GMNB") && n.ends_with("Microsoft.")),
                "the description isn't exported"
            );
            let update = tree(&nodes, "test", None, 1.0);
            let switch = update
                .nodes
                .iter()
                .find(|(_, n)| n.role() == Role::Switch)
                .map(|(_, n)| n)
                .expect("the accent switch");
            assert_eq!(switch.label(), Some("Use the desktop's accent colour"));
            let explained = names.iter().any(|n| n == crate::app::NO_ACCENT);
            if accent.is_none() {
                assert!(switch.is_disabled());
                assert_eq!(switch.description(), Some(crate::app::NO_ACCENT));
                assert_eq!(switch.described_by().len(), 1);
                assert!(!switch.supports_action(Action::Click));
                assert!(explained, "the reason isn't exported");
                let id = nodes.iter().find(|n| n.role == Role::Switch).unwrap().id;
                assert!(target(&hits, id, Action::Click).is_none());
            } else {
                assert!(!switch.is_disabled() && switch.description().is_none());
                assert!(switch.supports_action(Action::Click));
                assert!(!explained);
            }
            // Graphing's number precision: a slider from 5 to 21 (Off),
            // its value said in full and what it does as its description;
            // assistive technology can step and set it (check_actions).
            let id = crate::app::precision_slider();
            let slider = update
                .nodes
                .iter()
                .find(|(n, _)| n.0 == id)
                .map(|(_, n)| n)
                .expect("the number precision slider");
            assert_eq!(slider.role(), Role::Slider);
            assert_eq!(slider.label(), Some("Number precision"));
            assert_eq!(slider.value(), Some("14 digits (TI-84 Plus CE)"));
            assert_eq!(
                (
                    slider.numeric_value(),
                    slider.min_numeric_value(),
                    slider.max_numeric_value()
                ),
                (Some(14.0), Some(5.0), Some(21.0))
            );
            let words = format!(
                "14 digits (TI-84 Plus CE). {}",
                appcore::graph::NUMBER_PRECISION_HELP
            );
            assert_eq!(slider.description(), Some(words.as_str()));
            assert!(names.iter().any(|n| n == "14 digits (TI-84 Plus CE)"));
            assert!(
                names
                    .iter()
                    .any(|n| n == appcore::graph::NUMBER_PRECISION_HELP)
            );
            for a in [
                Action::Increment,
                Action::Decrement,
                Action::SetValue,
                Action::Focus,
            ] {
                assert!(target(&hits, id, a).is_some(), "{a:?}");
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
            let (nodes, hits) = frame_nodes(|f, r| p.view(f, r, false));
            check_actions(&format!("{mode:?}"), &nodes, &hits);
        }
        let mut d = crate::date::DatePage::new();
        check(|f, r| d.view(f, r));
        let (nodes, hits) = frame_nodes(|f, r| d.view(f, r));
        check_actions("date", &nodes, &hits);
        let mut c = crate::conv::ConvPage::new(None);
        let (nodes, hits) = frame_nodes(|f, r| c.view(f, r));
        check_actions("converter", &nodes, &hits);
        let mut g =
            crate::graph::GraphPage::for_test(appcore::graph::from_list("x^2;y<sin(x);a*x"));
        check(|f, r| g.view(f, r));
        let (nodes, hits) = frame_nodes(|f, r| g.view(f, r));
        check_actions("graphing", &nodes, &hits);
        // The licences' text overflows: its view scrolls.
        let (nodes, hits) = frame_nodes(crate::app::draw_licences);
        check_actions("licences", &nodes, &hits);
        assert!(nodes.iter().any(|n| n.scrollable));
    }

    /// R17-M-04: the focused controls that ignore Enter, leaving it to the
    /// page ("=" on the calculator, nothing on the converter or the graph),
    /// are, on every page and overlay, those that press one of its keys, as
    /// upstream's `CalculatorButton`s and bit `FlipButtons`: every keypad's
    /// and flyout's key (`calc::Msg::Key`) but the 2nd and hyp toggles, MC,
    /// MR, M+, M− and MS, the bits, and the converter's and the graph's
    /// keys. By what they do, not by how they are drawn (2nd and hyp look
    /// like keys, MS and the bits like buttons). Every other control
    /// (menus, toggles, the angle, radix and word size, the History and
    /// Memory items and their buttons, the navigation and Settings) takes
    /// Enter itself, as in GMNB.
    #[test]
    fn enter_is_equals_on_the_calculators_own_keys() {
        use crate::app::Msg as App;
        use crate::calc::{Msg, Popup};
        use crate::ui::{Hit, id};
        use appcore::keys::{KEY_HYP, KEY_SECOND, KEY_TRIG_SECOND};
        use calcvm::{Button as B, CalcMode};

        let presses_a_key = |h: &Hit| match &h.msg {
            Some(App::Calc(Msg::Key(k))) => ![KEY_SECOND, KEY_TRIG_SECOND, KEY_HYP].contains(k),
            Some(App::Calc(Msg::FlipBit(_))) => true,
            Some(App::Conv(crate::conv::Msg::Key(_)) | App::Graph(crate::graph::Msg::Pad(_))) => {
                true
            }
            _ => false,
        };
        // Every focusable control drawn, by where it was drawn.
        let mut all: Vec<(String, Hit)> = Vec::new();
        let mut keep = |what: &str, hits: Vec<Hit>| {
            for h in hits.into_iter().filter(|h| h.focusable) {
                all.push((what.to_string(), h));
            }
        };
        let wide = Rect::new(0.0, 46.0, 1000.0, 654.0);
        let narrow = Rect::new(0.0, 46.0, 400.0, 654.0);
        for mode in [
            CalcMode::Standard,
            CalcMode::Scientific,
            CalcMode::Programmer,
        ] {
            for bits in [false, true] {
                if bits && mode != CalcMode::Programmer {
                    continue;
                }
                let mut p = crate::calc::CalcPage::new(None);
                p.set_mode(mode);
                // Memory and History hold an item each: their rows and
                // buttons are drawn, and MC and MR are enabled.
                press(&mut p, "2+3={ctrl+m}2+3");
                if bits {
                    with_cx(|cx| p.update(Msg::BitView(true), cx));
                }
                let what = format!("{mode:?}{}", if bits { " bits" } else { "" });
                keep(&what, frame_nodes(|f, _| p.view(f, wide, false)).1);
                keep(&what, frame_nodes(|f, _| p.view(f, narrow, false)).1);
                let popups: &[Popup] = match mode {
                    CalcMode::Standard => &[Popup::Panel, Popup::DisplayMenu(300.0, 200.0)],
                    CalcMode::Scientific => &[Popup::Trig, Popup::Functions, Popup::Panel],
                    CalcMode::Programmer => &[Popup::Bitwise, Popup::Shift, Popup::Panel],
                };
                for popup in popups {
                    p.popup = Some(*popup);
                    let r = if *popup == Popup::Panel { narrow } else { wide };
                    let what = format!("{what} {popup:?}");
                    keep(&what, frame_nodes(|f, _| p.overlay(f, r)).1);
                }
            }
        }
        let mut c = crate::conv::ConvPage::new(None);
        keep("converter", frame_nodes(|f, r| c.view(f, r)).1);
        let mut d = crate::date::DatePage::new();
        keep("date", frame_nodes(|f, r| d.view(f, r)).1);
        with_cx(|cx| d.update(crate::date::Msg::Calendar(Some(0)), cx));
        keep("calendar", frame_nodes(|f, r| d.overlay(f, r)).1);
        let mut g = crate::graph::GraphPage::for_test(appcore::graph::from_list("x^2;a*x"));
        keep("graphing", frame_nodes(|f, r| g.view(f, r)).1);
        keep(
            "navigation",
            frame_nodes(|f, _| {
                crate::app::draw_nav(f, full(), appcore::modes::ViewMode::Standard, false)
            })
            .1,
        );
        let settings = crate::app::Settings::default();
        let desktop = crate::app::Desktop::default();
        keep(
            "settings",
            frame_nodes(|f, r| crate::app::draw_settings(f, r, &settings, desktop)).1,
        );

        for (what, h) in &all {
            assert_eq!(
                h.enter_is_equals,
                presses_a_key(h),
                "{what}: {:?} {:?}",
                h.id,
                h.msg
            );
            // Space presses every button, a calculator key too; Enter
            // every button but a calculator key. (Fields, sliders, the
            // graph and scroll views take their keys elsewhere.)
            if h.sense == Sense::Click && h.msg.is_some() {
                assert!(h.activated_by(false), "{what}: {:?}", h.msg);
                assert_eq!(
                    h.activated_by(true),
                    !presses_a_key(h),
                    "{what}: {:?}",
                    h.msg
                );
            } else {
                assert!(!h.activated_by(false) && !h.activated_by(true), "{what}");
            }
        }
        let flag = |what: &str, hid: Id| {
            let found: Vec<bool> = all
                .iter()
                .filter(|(w, h)| w == what && h.id == hid)
                .map(|(_, h)| h.enter_is_equals)
                .collect();
            assert!(!found.is_empty(), "{what}: no {hid:?}");
            found[0]
        };
        // The review's three, and their neighbours.
        assert!(flag("Standard", id(("mem", B::Memory as u32))));
        assert!(flag("Standard", id(("mem", B::MemoryClear as u32))));
        assert!(flag("Standard", id(("keypad", B::Seven.id()))));
        assert!(!flag("Standard", id("mem-toggle")));
        assert!(!flag("Standard", id(("hist", 0usize))));
        assert!(!flag("Programmer", id(("memrow", 0usize))));
        assert!(!flag("Programmer", id(("memop", 0usize, 0usize))));
        assert!(!flag("Scientific", id(("keypad", KEY_SECOND))));
        assert!(!flag("Scientific", id("trig-btn")));
        assert!(!flag("Scientific Trig", id(("trig", KEY_TRIG_SECOND))));
        assert!(!flag("Scientific Trig", id(("trig", KEY_HYP))));
        assert!(flag("Programmer bits", id(("bit", 0u32))));
        assert!(flag("Programmer bits", id(("mem", B::Memory as u32))));
        assert!(!flag("Programmer", id("word")));
        assert!(!flag("Programmer Shift", id(("shift", 0usize))));
        // Some of each kind on the calculator, the converter's and the
        // graph's keypads; none on the date page.
        for page in [
            "Scientific Trig",
            "Programmer Bitwise",
            "converter",
            "graphing",
        ] {
            assert!(
                all.iter().any(|(w, h)| w == page && h.enter_is_equals),
                "{page}"
            );
        }
        for page in ["date", "calendar"] {
            assert!(all.iter().any(|(w, _)| w == page), "{page}: nothing drawn");
            assert!(
                all.iter().all(|(w, h)| w != page || !h.enter_is_equals),
                "{page}"
            );
        }
    }
}
