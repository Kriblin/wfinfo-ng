use std::error::Error;

use wfinfo::settings::SettingsApp;

pub fn run_settings() -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "WFinfo-ng Settings",
        options,
        Box::new(|_cc| Ok(Box::new(SettingsApp::new()))),
    )
    .map_err(|e| Box::new(e) as Box<dyn Error>)
}

fn main() -> Result<(), Box<dyn Error>> {
    run_settings()
}
