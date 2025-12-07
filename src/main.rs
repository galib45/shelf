mod pdf;
mod utils;
mod ui;
mod config;

use std::sync::Arc;
use std::sync::RwLock;

use gtk::prelude::*;
use gtk::glib;
use gtk::gio;

use crate::config::Config;
use crate::ui::window::ShelfWindow;

const APP_ID: &str = "org.galib.shelf";

fn main() -> glib::ExitCode {
    gio::resources_register_include!("compiled.gresource").expect("Failed to register resource");
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(app_main);
    app.run()
}

fn app_main(app: &gtk::Application) {
    let config = Arc::new(RwLock::new(Config::load().unwrap()));
    let window = ShelfWindow::new(app, config.clone()); 
    window.present();
}
