#![allow(dead_code)]
use gtk::glib::subclass::types::ObjectSubclassIsExt;
use gtk::glib;
use gtk::gio;
use gtk::pango::AttrList;
use gtk::pango::AttrSize;
use gtk::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use crate::config::Config;

mod imp {
    use gtk::glib;
    use gtk::glib::subclass::types::ObjectSubclass;
    use gtk::subclass::prelude::*;
    use std::cell::OnceCell;
    use std::sync::{Arc, RwLock};

    use crate::config::Config;
    
    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/org/galib/shelf/ui/settings_window.xml")]
    pub struct ShelfSettingsWindow {
        #[template_child]
        pub dirs_list: TemplateChild<gtk::Box>,
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub file_dialog: TemplateChild<gtk::FileDialog>,

        // Store the current directories
        pub config: OnceCell<Arc<RwLock<Config>>>,
    }
    
    #[glib::object_subclass]
    impl ObjectSubclass for ShelfSettingsWindow {
        const NAME: &'static str = "ShelfSettingsWindow";
        type Type = super::ShelfSettingsWindow;
        type ParentType = gtk::Window;
        
        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }
        
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }
    
    impl ObjectImpl for ShelfSettingsWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }
    
    impl WidgetImpl for ShelfSettingsWindow {}
    impl WindowImpl for ShelfSettingsWindow {}
}

glib::wrapper! {
    pub struct ShelfSettingsWindow(ObjectSubclass<imp::ShelfSettingsWindow>)
        @extends gtk::Widget, gtk::Window,
        @implements gio::ActionGroup, gio::ActionMap,
                    gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native,
                    gtk::Root, gtk::ShortcutManager;
}

impl ShelfSettingsWindow {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        let obj: ShelfSettingsWindow = glib::Object::builder().build();
        obj.imp().config.set(config).unwrap();
        obj.setup();
        obj
    }
    
    fn setup(&self) {
        let imp = self.imp();
        
        // Load config
        // let config = Config::load().unwrap();
        // *imp.dirs.borrow_mut() = config.scan_dirs.clone();
        
        // Setup add button
        let clone = self.clone();
        imp.add_button.connect_clicked( move |_| {
            let _self = clone.clone();
            // clone.show_add_directory_dialog();
            clone.imp().file_dialog.select_folder(Some(&clone), None::<&gio::Cancellable>, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        _self.add_directory(path);
                    }
                }
            });
        });
        
        // Populate the list
        self.refresh_directory_list();
    }
    
    fn refresh_directory_list(&self) {
        let imp = self.imp();
        
        // Clear existing items
        while let Some(child) = imp.dirs_list.first_child() {
            imp.dirs_list.remove(&child);
        }
        
        // Add each directory
        let config = imp.config.get().unwrap();
        let config_reader = config.read().unwrap();
        for (idx, path) in config_reader.scan_dirs.iter().enumerate() {
            let row = self.create_directory_row(path, idx);
            imp.dirs_list.append(&row);
        }
    }
    
    fn create_directory_row(&self, path: &PathBuf, index: usize) -> gtk::Box {
        let hbox = gtk::Box::builder().build();
        
        let label = gtk::Label::new(Some(&path.display().to_string()));
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let attr_list = AttrList::new();
        attr_list.insert(AttrSize::new(10480));
        label.set_attributes(Some(&attr_list));
        
        let button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .has_frame(false)
            .valign(gtk::Align::Center)
            .build();
        button.add_css_class("destructive-action");
        
        // Connect delete button
        let clone = self.clone();
        button.connect_clicked(move |_| {
            clone.remove_directory(index);
        });
        
        hbox.append(&label);
        hbox.append(&button);
        
        hbox
    } 

    fn add_directory(&self, path: PathBuf) {
        let imp = self.imp();
        let config = imp.config.get().unwrap();
        let mut config_writer = config.write().unwrap();
        let canon = path.canonicalize().unwrap();
        
        // Check if directory already exists
        if !config_writer.scan_dirs.iter().any(|p| canon.starts_with(p)) {
        // if !dirs.contains(&path) {
            config_writer.scan_dirs.push(path.canonicalize().unwrap());
            drop(config_writer); // Release the borrow
            
            // Save and refresh
            self.save_config();
            self.refresh_directory_list();
        } else {
            eprintln!("The path {} or one of its ancestors is already added", path.display());
        }
    }
    
    fn remove_directory(&self, index: usize) {
        let imp = self.imp();
        let config = imp.config.get().unwrap();
        let mut config_writer = config.write().unwrap();
        
        if index < config_writer.scan_dirs.len() {
            config_writer.scan_dirs.remove(index);

            drop(config_writer); // Release the borrow
            
            // Save and refresh
            self.save_config();
            self.refresh_directory_list();
        }
    }
    
    fn save_config(&self) {
        let imp = self.imp();
        let config = imp.config.get().unwrap();
        let config_writer = config.write().unwrap();
        // let mut config = Config::load().unwrap_or_default();
        // config.scan_dirs = imp.dirs.borrow().clone();
        
        if let Err(e) = config_writer.save() {
            eprintln!("Failed to save config: {}", e);
            // Optionally show an error dialog to the user
        }
    }
}
