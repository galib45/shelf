use gtk::glib::{self, subclass::types::ObjectSubclassIsExt};

use crate::pdf::PdfMetadata;

mod imp {
    use std::cell::RefCell;
    use gtk::glib;
    use gtk::glib::subclass::{object::ObjectImpl, types::ObjectSubclass};

    use crate::pdf::PdfMetadata;

    #[derive(Debug, Default)]
    pub struct PdfMetadataObject {
        pub metadata: RefCell<Option<PdfMetadata>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PdfMetadataObject {
        const NAME: &'static str = "PdfMetadataObject";
        type Type = super::PdfMetadataObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for PdfMetadataObject {}
}

glib::wrapper! {
    pub struct PdfMetadataObject(ObjectSubclass<imp::PdfMetadataObject>);
}

impl PdfMetadataObject {
    pub fn new(metadata: PdfMetadata) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().metadata.replace(Some(metadata));
        obj
    }

    pub fn metadata(&self) -> Option<PdfMetadata> {
        self.imp().metadata.borrow().clone()
    }
}
