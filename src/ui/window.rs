#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gtk::glib::subclass::types::ObjectSubclassIsExt;
use gtk::{prelude::*, SignalListItemFactory, SingleSelection};
use gtk::glib;
use gtk::gio;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::pdf::{extract_pdf_metadata, PdfCache, PdfMetadata, ScanProgress};
use crate::ui::grid_item::ShelfGridItem;
use crate::ui::models::PdfMetadataObject;
use crate::utils::scan_pdfs_rayon;
use super::models;

mod imp {
    use std::sync::{Arc, Mutex};

    use gtk::glib;
    use gtk::glib::subclass::types::ObjectSubclass;
    use gtk::subclass::prelude::*;

    use crate::pdf::PdfMetadata;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/org/galib/shelf/ui/window.xml")]
    pub struct ShelfWindow {
        #[template_child]
        pub info_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub scan_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>,

        // Store for PDF files
        pub metadata_list: Arc<Mutex<Vec<PdfMetadata>>>, 
        pub selected: Arc<Mutex<Option<PdfMetadata>>>,
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
            let obj = self.obj();
            obj.setup();
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
    pub fn new(app: &gtk::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn setup(&self) {
        let imp = self.imp();
        let model = gio::ListStore::new::<models::PdfMetadataObject>();
        let selection_model = SingleSelection::new(Some(model.clone()));
        selection_model.set_selected(0);
        let factory = SignalListItemFactory::new();

        let selected_clone = imp.selected.clone();
        selection_model.connect_selection_changed(move |_self, _, _| {
            let item = _self.selected_item().unwrap();
            let metadata_object = item.downcast_ref::<PdfMetadataObject>().unwrap();
            {
                let mut selected = selected_clone.lock().unwrap();
                *selected = metadata_object.metadata();
            }
        });

        let status_label_for_factory = imp.status_label.clone();
        let selected_for_factory = imp.selected.clone();
        factory.connect_setup(move |_grid, item| {
            let grid_item = ShelfGridItem::new();
            let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
            list_item.set_child(Some(&grid_item));
            
            // Add motion controller once during setup
            let motion_controller = gtk::EventControllerMotion::new();
            let status_label = status_label_for_factory.clone();
            let selected = selected_for_factory.clone();
            
            let list_item_weak = list_item.downgrade();
            motion_controller.connect_enter(move |_, _, _| {
                if let Some(list_item) = list_item_weak.upgrade() {
                    if let Some(obj) = list_item.item() {
                        if let Some(pdf_obj) = obj.downcast_ref::<PdfMetadataObject>() {
                            if let Some(metadata) = pdf_obj.metadata() {
                                status_label.set_text(&metadata.path);
                            }
                        }
                    }
                }
            });
    
            let status_label = status_label_for_factory.clone();
            motion_controller.connect_leave(move |_| {
                let selected = selected.lock().unwrap();
                if let Some(metadata) = selected.as_ref() {
                    status_label.set_text(&metadata.path);
                }
            });
            
            grid_item.add_controller(motion_controller);
        });
        
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

        let model_clone = model.clone();
        let status_label_clone = imp.status_label.clone();
        let scan_button_clone = imp.scan_button.clone();
        let pdf_files_clone = imp.metadata_list.clone();
        let search_entry_clone = imp.search_entry.clone();
        imp.scan_button.connect_clicked(move |_| {
            // we are cloning again because we will spawn our own thread
            let model = model_clone.clone();
            let progress_label = status_label_clone.clone();
            let scan_button = scan_button_clone.clone();
            let pdf_files = pdf_files_clone.clone();
            let search_entry = search_entry_clone.clone();
            
            // Disable button during scan
            scan_button.set_sensitive(false);
            search_entry.set_sensitive(false);
            progress_label.set_text("Scanning...");
            
            // Clear previous results
            model.remove_all();
            search_entry.set_text("");
            let (tx, rx) = async_channel::unbounded::<ScanProgress>();
            std::thread::spawn(move || {
                let scan_dirs: Vec<PathBuf> = vec![
                    PathBuf::from("/home/galib"),
                    PathBuf::from("/mnt/data")
                ];

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
                for dir in &scan_dirs {
                     pdf_paths.extend(scan_pdfs_rayon(dir, tx.clone()));
                } 
                pdf_paths.sort_unstable(); 
                                
                // Process PDFs in parallel
                // Replace the parallel processing section with:
                let metadata_list: Vec<PdfMetadata> = pdf_paths.par_iter().filter_map(|path| {
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
                let _ = tx.send_blocking(ScanProgress::Complete(metadata_list, duration));
            });

            gtk::glib::spawn_future_local(async move {
                use std::cell::Cell;
                let count = Cell::new(0);
                
                while let Ok(msg) = rx.recv().await {
                    match msg {
                        ScanProgress::Found(_path) => {
                            count.set(count.get() + 1);
                            progress_label.set_text(&format!("Found {} PDFs...", count.get())); 
                        }
                        ScanProgress::Processing(path) => {
                            progress_label.set_text(&format!("Processing: {}...", path.display()));
                        }
                        ScanProgress::Extracted(_hash, metadata) => {
                            progress_label.set_text(&format!("Extracted: {}...", 
                                metadata.title.as_deref().unwrap_or("Untitled")));
                            // model.append(&PdfMetadataObject::new(metadata));
                        }
                        ScanProgress::DuplicateDetected(original, duplicate) => {
                            println!("Duplicate detected: {} is duplicate of {}", 
                                duplicate.display(), original.display());
                        }
                        ScanProgress::Error(path, error) => {
                            eprintln!("Error processing {}: {}", path.display(), error);
                        }
                        ScanProgress::Complete(metadata_list, duration) => {
                            for item in &metadata_list {
                                model.append(&PdfMetadataObject::new(item.to_owned()));
                            }
                            progress_label.set_text(&format!(
                                "Complete! Found {} PDF files in {:.2?}",
                                metadata_list.len(),
                                duration
                            ));
                            // Store all PDFs for searching
                            {
                                let mut files = pdf_files.lock().unwrap();
                                *files = metadata_list;
                            }
  
                            scan_button.set_sensitive(true);
                            search_entry.set_sensitive(true);
                            search_entry.grab_focus();
                            break;
                        }
                    }
                }
            });
        });

        let model_clone = model.clone();
        imp.grid_view.connect_activate(move |_, position| {
            let item = model_clone.item(position).unwrap();
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
        });

        let model_search = model.clone();
        let pdf_files_search = imp.metadata_list.clone();

        imp.search_entry.connect_search_changed(move |entry| {
            let query = entry.text();
            
            let pdf_files = match pdf_files_search.lock() {
                Ok(files) => files,
                Err(poisoned) => poisoned.into_inner()
            };
            
            model_search.remove_all();
            
            if query.is_empty() {
                for item in pdf_files.iter() {
                    model_search.append(&PdfMetadataObject::new(item.clone()));
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
                
                for (metadata, _) in scored {
                    model_search.append(&PdfMetadataObject::new(metadata.clone()));
                }
            }
        });

        imp.scan_button.emit_clicked();
    } 
}
