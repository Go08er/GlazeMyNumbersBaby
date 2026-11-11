//! A still picture a theme drew, shown as a widget: the tile of one
//! choice of a `choice` option in Settings (docs/theming.md, "Previews").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gmnb_theme::draw::DisplayList;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Tile {
        pub list: RefCell<Option<Rc<DisplayList>>>,
        pub size: Cell<(i32, i32)>,
        /// Not drawn yet: Settings asks for it in a pass to come.
        pub pending: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Tile {
        const NAME: &'static str = "GmnbThemeTile";
        type Type = super::Tile;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("themetile");
            klass.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Tile {}

    impl WidgetImpl for Tile {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (w, h) = self.size.get();
            let n = if orientation == gtk::Orientation::Horizontal {
                w
            } else {
                h
            };
            (n, n, -1, -1)
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let obj = self.obj();
            // The picture is drawn at the tile's own size, in the middle
            // of whatever room the widget was given, and never outside.
            let (w, h) = self.size.get();
            let dx = ((obj.width() - w) / 2).max(0) as f32;
            let dy = ((obj.height() - h) / 2).max(0) as f32;
            let Some(list) = self.list.borrow().clone() else {
                if self.pending.get() {
                    // Its place, until the theme has drawn it: a faint
                    // wash of the text colour.
                    let rect = gtk::graphene::Rect::new(dx, dy, w as f32, h as f32);
                    let color = obj.color();
                    let color = color.with_alpha(color.alpha() * 0.08);
                    s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(rect, 6.0));
                    s.append_color(&color, &rect);
                    s.pop();
                }
                return;
            };
            s.save();
            s.translate(&gtk::graphene::Point::new(dx, dy));
            s.push_clip(&gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            super::super::paint(s, &*obj, &list, [&[], &[]]);
            s.pop();
            s.restore();
        }
    }
}

glib::wrapper! {
    pub struct Tile(ObjectSubclass<imp::Tile>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Tile {
    /// A tile of this size, in pixels, with nothing in it yet: it shows
    /// its place until [`Tile::set_list`].
    pub fn new(width: f32, height: f32) -> Tile {
        let tile: Tile = glib::Object::new();
        tile.imp()
            .size
            .set((width.round() as i32, height.round() as i32));
        tile.imp().pending.set(true);
        tile
    }

    /// What the tile shows; `None`: nothing (the preview failed).
    pub fn set_list(&self, list: Option<DisplayList>) {
        self.imp().pending.set(false);
        self.imp().list.replace(list.map(Rc::new));
        self.queue_draw();
    }
}
