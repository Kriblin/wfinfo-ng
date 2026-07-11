use std::error::Error;

use crate::ui::settings::SettingsApp;

pub fn run() -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "WFinfo-ng Settings",
        options,
        Box::new(|_cc| Ok(Box::new(SettingsApp::new()))),
    )
    .map_err(|e| Box::new(e) as Box<dyn Error>)
}
