use glib::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

mod imp {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct CardFrame {
        pub ratio: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardFrame {
        const NAME: &'static str = "CardFrame";
        type Type = super::CardFrame;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for CardFrame {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for CardFrame {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let ratio = self.ratio.get();
            match (orientation, for_size) {
                // Width is derived from height × ratio (if height known)
                (gtk::Orientation::Horizontal, fs) if fs > 0 => {
                    let w = (fs as f64 * ratio) as i32;
                    (w, w, -1, -1)
                }
                // Default natural width: 210px (matches .mod-card CSS)
                (gtk::Orientation::Horizontal, _) => (210, 210, -1, -1),
                // Height is derived from width / ratio (if width known)
                (gtk::Orientation::Vertical, fs) if fs > 0 => {
                    let h = (fs as f64 / ratio) as i32;
                    (h, h, -1, -1)
                }
                // Default: natural height 140px (210 / 1.5)
                _ => (140, 140, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let ratio = self.ratio.get();
            let max_h = (width as f64 / ratio) as i32;
            let h = height.min(max_h);
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, h, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct CardFrame(ObjectSubclass<imp::CardFrame>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::Accessible;
}

impl CardFrame {
    pub fn new(ratio: f64) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_ratio(ratio);
        obj
    }

    fn set_ratio(&self, ratio: f64) {
        self.imp().ratio.set(ratio);
    }

    pub fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        while let Some(c) = self.first_child() {
            c.unparent();
        }
        if let Some(child) = child {
            child.set_parent(self);
        }
    }
}
