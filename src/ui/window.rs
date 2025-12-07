#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gtk::glib::subclass::types::ObjectSubclassIsExt;
use gtk::{prelude::*, SignalListItemFactory, SingleSelection};
use gtk::glib;
use gtk::gio;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::config::Config;
use crate::pdf::{extract_pdf_metadata, PdfCache, PdfMetadata, ScanProgress};
use crate::ui::grid_item::ShelfGridItem;
use crate::ui::models::PdfMetadataObject;
use crate::ui::settings_window::ShelfSettingsWindow;
use crate::utils::scan_pdfs_rayon;
use super::models;

mod imp {
    use std::cell::OnceCell;
    use std::sync::{Arc, Mutex, RwLock};

    use gtk::glib;
    use gtk::glib::subclass::types::ObjectSubclass;
    use gtk::subclass::prelude::*;

    use crate::config::Config;
    use crate::pdf::PdfMetadata;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/org/galib/shelf/ui/window.xml")]
    pub struct ShelfWindow {
        #[template_child]
        pub info_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub refresh_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub settings_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub search_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>,

        // Store for PDF files
        pub metadata_list: Arc<Mutex<Vec<PdfMetadata>>>, 
        pub selected: Arc<Mutex<Option<PdfMetadata>>>,
        pub config: OnceCell<Arc<RwLock<Config>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ShelfWindow {
        const NAME: &'static str = "ShelfWindow";
        type Type = super::ShelfWindow;
        type ParentType = gtk::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ShelfWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }
    impl WidgetImpl for ShelfWindow {}
    impl WindowImpl for ShelfWindow {}
    impl ApplicationWindowImpl for ShelfWindow {}
}

glib::wrapper! {
    pub struct ShelfWindow(ObjectSubclass<imp::ShelfWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap,
                    gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native,
                    gtk::Root, gtk::ShortcutManager;
}

impl ShelfWindow {
    pub fn new(app: &gtk::Application, config: Arc<RwLock<Config>>) -> Self {
        let obj: ShelfWindow = glib::Object::builder().property("application", app).build();
        obj.imp().config.set(config).unwrap();
        obj.setup();
        obj
    }

    fn setup(&self) {
        let imp = self.imp();
        let model = gio::ListStore::new::<models::PdfMetadataObject>();
        self.setup_grid_view(model.clone());
        self.setup_buttons(model.clone());
        self.setup_search_entry(model.clone());
        imp.refresh_button.emit_clicked();
    } 

    fn setup_search_entry(&self, model: gio::ListStore) {
        let imp = self.imp();
        imp.search_entry.connect_search_changed(glib::clone!(
            #[strong] model,
            #[strong(rename_to = selected)] imp.selected,
            #[strong(rename_to = metadata_list)] imp.metadata_list,
            #[weak(rename_to = status_label)] imp.status_label,
            move |entry| {
                let query = entry.text();
                
                let pdf_files = match metadata_list.lock() {
                    Ok(files) => files,
                    Err(poisoned) => poisoned.into_inner()
                };
                
                model.remove_all();
                
                if query.is_empty() {
                    for item in pdf_files.iter() {
                        model.append(&PdfMetadataObject::new(item.clone()));
                    }
                    {
                        let mut selected = selected.lock().unwrap();
                        *selected = Some(pdf_files[0].clone());
                        status_label.set_text(&pdf_files[0].path); 
                    }
                     
                } else {
                    let matcher = SkimMatcherV2::default();
                    let query_str = query.as_str();
                    
                    let mut scored: Vec<(&PdfMetadata, i64)> = pdf_files
                        .par_iter()
                        .filter_map(|pdf| {
                            // Extract filename from path
                            let filename = std::path::Path::new(&pdf.path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            
                            let searchable = format!(
                                "{} {} {}",
                                filename, 
                                pdf.title.as_deref().unwrap_or(""),
                                pdf.author.as_deref().unwrap_or("")
                            );
                            
                            matcher.fuzzy_match(&searchable, query_str)
                                .map(|score| (pdf, score))
                        })
                        .collect();
                    
                    scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                    scored.truncate(10);
                    {
                        let mut selected = selected.lock().unwrap();
                        *selected = Some(scored[0].0.clone());
                        status_label.set_text(&scored[0].0.path); 
                    }
                    for (metadata, _) in scored {
                        model.append(&PdfMetadataObject::new(metadata.clone()));
                    }
                }
            }
        ));
    }

    fn setup_buttons(&self, model: gio::ListStore) {
        let imp = self.imp();
        imp.search_button.connect_clicked(glib::clone!(
            #[weak(rename_to = search_entry)] imp.search_entry,
            move |_| {
                search_entry.set_visible(!search_entry.is_visible());
                search_entry.grab_focus();
            }
        ));

        let config = imp.config.get().unwrap();
        imp.settings_button.connect_clicked(glib::clone!(
            #[strong] config,
            move |_| {
                let dialog = ShelfSettingsWindow::new(config.clone());
                dialog.present();
            }
        ));

        imp.refresh_button.connect_clicked(glib::clone!(
            #[strong] model,
            #[strong] config,
            #[strong(rename_to = metadata_list)] imp.metadata_list,
            #[strong(rename_to = selected)] imp.selected,
            #[weak(rename_to = refresh_button)] imp.refresh_button,
            #[weak(rename_to = search_button)] imp.search_button,
            #[weak(rename_to = search_entry)] imp.search_entry,
            #[weak(rename_to = status_label)] imp.status_label,
            move |_| {
                // Disable button during scan
                refresh_button.set_sensitive(false);
                search_button.set_sensitive(false);
                search_entry.set_sensitive(false);
                status_label.set_text("Scanning...");
                
                // Clear previous results
                model.remove_all();
                search_entry.set_text("");
                let (tx, rx) = async_channel::unbounded::<ScanProgress>();
                std::thread::spawn(glib::clone!(
                    #[strong] config,
                    move || {
                        let start_time = Instant::now(); 
                        let cache = match PdfCache::new() {
                            Ok(c) => Arc::new(c),
                            Err(e) => {
                                let _ = tx.send_blocking(ScanProgress::Error(
                                    PathBuf::from("cache"),
                                    format!("Failed to initialize cache: {}", e)
                                ));
                                return;
                            }
                        };
                        let mut pdf_paths: Vec<PathBuf> = Vec::new();
                        for dir in &config.read().unwrap().scan_dirs {
                             pdf_paths.extend(scan_pdfs_rayon(dir, tx.clone()));
                        } 
                        pdf_paths.sort_unstable(); 
                                        
                        // Process PDFs in parallel
                        // Replace the parallel processing section with:
                        let metadata_list_new: Vec<PdfMetadata> = pdf_paths.par_iter().filter_map(|path| {
                            let _ = tx.send_blocking(ScanProgress::Processing(path.clone()));
                            let cache = cache.clone();

                            match extract_pdf_metadata(path, &cache, &tx) {
                                Ok(metadata) => Some(metadata),
                                Err(e) => {
                                    let _ = tx.send_blocking(ScanProgress::Error(
                                        path.clone(),
                                        format!("Extraction failed: {}", e),
                                    ));
                                    None
                                }
                            }
                        })
                        .collect();

                        let duration = start_time.elapsed();
                        let _ = tx.send_blocking(ScanProgress::Complete(metadata_list_new, duration));
                    }
                ));

                gtk::glib::spawn_future_local(glib::clone!(
                    #[strong] model,
                    #[strong] selected,
                    #[strong] metadata_list,
                    async move {
                        use std::cell::Cell;
                        let count = Cell::new(0);
                        
                        while let Ok(msg) = rx.recv().await {
                            match msg {
                                ScanProgress::Found(_path) => {
                                    count.set(count.get() + 1);
                                    status_label.set_text(&format!("Found {} PDFs...", count.get())); 
                                }
                                ScanProgress::Processing(path) => {
                                    status_label.set_text(&format!("Processing: {}...", path.display()));
                                }
                                ScanProgress::Extracted(_hash, metadata) => {
                                    status_label.set_text(&format!("Extracted: {}...", 
                                        metadata.title.as_deref().unwrap_or("Untitled")));
                                }
                                ScanProgress::DuplicateDetected(original, duplicate) => {
                                    println!("Duplicate detected: {} is duplicate of {}", 
                                        duplicate.display(), original.display());
                                }
                                ScanProgress::Error(path, error) => {
                                    eprintln!("Error processing {}: {}", path.display(), error);
                                }
                                ScanProgress::Complete(metadata_list_new, duration) => {
                                    for item in &metadata_list_new {
                                        model.append(&PdfMetadataObject::new(item.to_owned()));
                                    }
                                    status_label.set_text(&format!(
                                        "Complete! Found {} PDF files in {:.2?}",
                                        metadata_list_new.len(),
                                        duration
                                    ));
                                    // Store all PDFs for searching
                                    {
                                        let mut selected = selected.lock().unwrap();
                                        let mut files = metadata_list.lock().unwrap();
                                        *files = metadata_list_new;
                                        *selected = Some(files[0].clone());
                                    }
          
                                    refresh_button.set_sensitive(true);
                                    search_button.set_sensitive(true);
                                    search_entry.set_sensitive(true);
                                    search_entry.grab_focus();
                                    break;
                                }
                            }
                        }
                    }
                ));
            }
        ));
    }

    fn setup_grid_view(&self, model: gio::ListStore) {
        let imp = self.imp();
        let selection_model = SingleSelection::new(Some(model.clone()));
        selection_model.set_selected(0);
        let factory = SignalListItemFactory::new();

        selection_model.connect_selection_changed(glib::clone!(
            #[strong(rename_to = selected)] imp.selected, 
            move |_self, _, _| {
                let item = _self.selected_item().unwrap();
                let metadata_object = item.downcast_ref::<PdfMetadataObject>().unwrap();
                {
                    let mut selected = selected.lock().unwrap();
                    *selected = metadata_object.metadata();
                }
            }
        ));

        factory.connect_setup(glib::clone!(
            #[strong(rename_to = selected)] imp.selected,
            #[weak(rename_to = status_label)] imp.status_label,
            move |_, item| {
                let grid_item = ShelfGridItem::new();
                let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
                list_item.set_child(Some(&grid_item));
                
                // Add motion controller once during setup
                let motion_controller = gtk::EventControllerMotion::new();
                
                let list_item_weak = list_item.downgrade();
                motion_controller.connect_enter(glib::clone!(
                    #[weak] status_label,
                    move |_, _, _| {
                        if let Some(list_item) = list_item_weak.upgrade() {
                            if let Some(obj) = list_item.item() {
                                if let Some(pdf_obj) = obj.downcast_ref::<PdfMetadataObject>() {
                                    if let Some(metadata) = pdf_obj.metadata() {
                                        status_label.set_text(&metadata.path);
                                    }
                                }
                            }
                        }
                    }
                ));
    
                motion_controller.connect_leave(glib::clone!(
                    #[strong] selected,
                    #[weak] status_label,
                    move |_| {
                        let selected = selected.lock().unwrap();
                        if let Some(metadata) = selected.as_ref() {
                            status_label.set_text(&metadata.path);
                        }
                    }
                ));
                
                grid_item.add_controller(motion_controller);
            }
        ));
        
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let pdf_metadata_object = item.item().and_downcast::<PdfMetadataObject>().unwrap();
            let grid_item = item.child().and_downcast::<ShelfGridItem>().unwrap();
            grid_item.bind(&pdf_metadata_object);
        });

        imp.grid_view.set_model(Some(&selection_model));
        imp.grid_view.set_factory(Some(&factory));
        imp.grid_view.set_min_columns(2);
        imp.grid_view.set_max_columns(6);
        imp.grid_view.set_single_click_activate(false);

        imp.grid_view.connect_activate(glib::clone!(
            #[strong] model,
            move |_, position| {
                let item = model.item(position).unwrap();
                let metadata_object = item.downcast_ref::<PdfMetadataObject>().unwrap(); 
                if let Some(metadata) = metadata_object.metadata() {
                    let path = metadata.path.clone();
                    // Spawn Zathura in a separate process
                    std::thread::spawn(move || {
                        match Command::new("zathura")
                            .arg(path.as_str())
                            .spawn() {
                            Ok(_) => println!("Opened {} with Zathura", path),
                            Err(e) => eprintln!("Failed to open {}: {}", path, e),
                        }
                    });
                } 
            }
        ));
    }
}
