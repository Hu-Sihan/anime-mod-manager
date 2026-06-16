use glib::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

mod imp {
    use std::cell::Cell;

    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct FixedSizeFrame {
        pub width: Cell<i32>,
        pub height: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FixedSizeFrame {
        const NAME: &'static str = "FixedSizeFrame";
        type Type = super::FixedSizeFrame;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for FixedSizeFrame {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FixedSizeFrame {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let width = self.width.get().max(0);
                    (width, width, -1, -1)
                }
                gtk::Orientation::Vertical => {
                    let height = self.height.get().max(0);
                    (height, height, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct FixedSizeFrame(ObjectSubclass<imp::FixedSizeFrame>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::Accessible;
}

impl FixedSizeFrame {
    pub fn new(width: i32, height: i32) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_fixed_size(width, height);
        obj
    }

    fn set_fixed_size(&self, width: i32, height: i32) {
        self.imp().width.set(width);
        self.imp().height.set(height);
    }

    pub fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        while let Some(existing) = self.first_child() {
            existing.unparent();
        }
        if let Some(child) = child {
            child.set_parent(self);
        }
    }
}
