use gtk::glib;
use gtk::subclass::prelude::*;

use crate::ui::models::PdfMetadataObject;

mod imp {
    use super::*; 

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(string = r#"
        <interface>
          <template class="ShelfGridItem" parent="GtkBox">
            <property name="orientation">vertical</property>
            <!-- <property name="spacing">24</property> -->
            <property name="margin-start">12</property>
            <property name="margin-end">12</property>
            <property name="margin-top">12</property>
            <property name="margin-bottom">12</property>
            
            <!-- <style> -->
            <!--   <class name="card"/> -->
            <!-- </style> -->
            
            <child>
              <object class="GtkImage" id="cover_image">
                <property name="pixel-size">128</property>
                <property name="halign">center</property>
              </object>
            </child>
            
          </template>
        </interface>
        "#)]
    pub struct ShelfGridItem {
        #[template_child]
        pub cover_image: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ShelfGridItem {
        const NAME: &'static str = "ShelfGridItem";
        type Type = super::ShelfGridItem;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ShelfGridItem {}
    impl WidgetImpl for ShelfGridItem {}
    impl BoxImpl for ShelfGridItem {}
}

glib::wrapper! {
    pub struct ShelfGridItem(ObjectSubclass<imp::ShelfGridItem>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ShelfGridItem {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn bind(&self, pdf_metadata_object: &PdfMetadataObject) {
        let imp = self.imp();
        if let Some(metadata) = pdf_metadata_object.metadata() {
            if let Some(cover_path) = metadata.cover_path {
                let cover_path = dirs::home_dir().unwrap().join(".shelf").join("covers").join(cover_path);
                if std::path::Path::new(&cover_path).exists() {
                    imp.cover_image.set_from_file(Some(&cover_path));
                } else {
                    imp.cover_image.set_icon_name(Some("x-office-document"));
                }
            } else {
                imp.cover_image.set_icon_name(Some("x-office-document"));
            } 
        }
    } 
}
