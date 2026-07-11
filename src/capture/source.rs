use image::DynamicImage;
use log::error;
use xcap::{Monitor, Window};

use crate::{config::CaptureMode, ui::state::WindowCaptureState};

pub fn find_window_by_title(title: &str, capture_state: &WindowCaptureState) -> Option<Window> {
    let title = title.to_string();
    match Window::all() {
        Ok(windows) => {
            let found = windows
                .into_iter()
                .find(|window| window.title().ok().as_ref() == Some(&title));

            if found.is_some() {
                capture_state.set_found(title);
            } else {
                capture_state.set_not_found(title);
            }
            found
        }
        Err(err) => {
            error!("Failed to get windows: {err}");
            capture_state.set_not_found(title);
            None
        }
    }
}

pub fn capture_image(
    mode: &CaptureMode,
    window_title: &str,
    capture_state: &WindowCaptureState,
) -> Result<DynamicImage, String> {
    match mode {
        CaptureMode::Window => {
            let window = find_window_by_title(window_title, capture_state)
                .ok_or_else(|| "Warframe window not found during detection".to_string())?;
            window
                .capture_image()
                .map(DynamicImage::ImageRgba8)
                .map_err(|err| format!("Failed to capture window: {err}"))
        }
        CaptureMode::Monitor => {
            let monitors =
                Monitor::all().map_err(|err| format!("Failed to get monitors: {err}"))?;
            let monitor = monitors
                .first()
                .ok_or_else(|| "No monitors found".to_string())?;
            capture_state.set_found("Primary Monitor".to_string());
            monitor
                .capture_image()
                .map(DynamicImage::ImageRgba8)
                .map_err(|err| format!("Failed to capture monitor: {err}"))
        }
    }
}
