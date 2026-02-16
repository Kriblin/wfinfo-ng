use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct DetectionState {
    running: Arc<AtomicBool>,
}

impl DetectionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn status_label(&self) -> &'static str {
        if self.is_running() { "Running" } else { "Idle" }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowTitleState {
    title: Arc<Mutex<String>>,
}

impl WindowTitleState {
    pub fn new(initial: String) -> Self {
        Self {
            title: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn get(&self) -> String {
        self.title
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set(&self, value: String) {
        if let Ok(mut guard) = self.title.lock() {
            *guard = value;
        }
    }
}

#[derive(Clone, Debug)]
pub struct WindowCaptureState {
    status: Arc<Mutex<WindowCaptureStatus>>,
}

#[derive(Clone, Debug)]
struct WindowCaptureStatus {
    found: bool,
    window_title: String,
}

impl WindowCaptureState {
    pub fn new(initial_window: String) -> Self {
        Self {
            status: Arc::new(Mutex::new(WindowCaptureStatus {
                found: false,
                window_title: initial_window,
            })),
        }
    }

    pub fn set_found(&self, window_title: String) {
        self.set_status(true, window_title);
    }

    pub fn set_not_found(&self, window_title: String) {
        self.set_status(false, window_title);
    }

    pub fn status_label(&self) -> String {
        self.status
            .lock()
            .map(|status| {
                if status.found {
                    "Window Capture attached".to_string()
                } else {
                    format!("No Window '{}' found", status.window_title)
                }
            })
            .unwrap_or_else(|_| "Window capture status unavailable".to_string())
    }

    fn set_status(&self, found: bool, window_title: String) {
        if let Ok(mut guard) = self.status.lock() {
            guard.found = found;
            guard.window_title = window_title;
        }
    }
}

pub trait SettingsLauncher {
    fn open_settings(&self) -> Result<(), String>;
}

pub struct MainUiState<L: SettingsLauncher> {
    pub detection: DetectionState,
    pub last_error: Option<String>,
    launcher: L,
}

impl<L: SettingsLauncher> MainUiState<L> {
    pub fn new(detection: DetectionState, launcher: L) -> Self {
        Self {
            detection,
            last_error: None,
            launcher,
        }
    }

    pub fn open_settings(&mut self) -> Result<(), String> {
        match self.launcher.open_settings() {
            Ok(()) => {
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                self.last_error = Some(err.clone());
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct TestLauncher {
        calls: Cell<usize>,
        result: Result<(), String>,
    }

    impl TestLauncher {
        fn new(result: Result<(), String>) -> Self {
            Self {
                calls: Cell::new(0),
                result,
            }
        }
    }

    impl SettingsLauncher for TestLauncher {
        fn open_settings(&self) -> Result<(), String> {
            self.calls.set(self.calls.get() + 1);
            self.result.clone()
        }
    }

    #[test]
    fn detection_state_tracks_running() {
        let state = DetectionState::new();
        assert!(!state.is_running());

        state.set_running(true);
        assert!(state.is_running());
        assert_eq!(state.status_label(), "Running");

        state.set_running(false);
        assert!(!state.is_running());
        assert_eq!(state.status_label(), "Idle");
    }

    #[test]
    fn open_settings_clears_error_on_success() {
        let detection = DetectionState::new();
        let launcher = TestLauncher::new(Ok(()));
        let mut state = MainUiState::new(detection, launcher);
        state.last_error = Some("previous".to_string());

        let result = state.open_settings();

        assert!(result.is_ok());
        assert!(state.last_error.is_none());
        assert_eq!(state.launcher.calls.get(), 1);
    }

    #[test]
    fn open_settings_stores_error_on_failure() {
        let detection = DetectionState::new();
        let launcher = TestLauncher::new(Err("boom".to_string()));
        let mut state = MainUiState::new(detection, launcher);

        let result = state.open_settings();

        assert!(result.is_err());
        assert_eq!(state.last_error.as_deref(), Some("boom"));
        assert_eq!(state.launcher.calls.get(), 1);
    }

    #[test]
    fn window_title_state_updates_across_clones() {
        let state = WindowTitleState::new("Warframe".to_string());
        let clone = state.clone();

        assert_eq!(state.get(), "Warframe");
        clone.set("Warframe Test".to_string());

        assert_eq!(state.get(), "Warframe Test");
        assert_eq!(clone.get(), "Warframe Test");
    }

    #[test]
    fn window_capture_state_reports_status() {
        let state = WindowCaptureState::new("Warframe".to_string());
        assert_eq!(state.status_label(), "No Window 'Warframe' found");

        state.set_found("Warframe".to_string());
        assert_eq!(state.status_label(), "Window Capture attached");

        state.set_not_found("Warframe Test".to_string());
        assert_eq!(state.status_label(), "No Window 'Warframe Test' found");
    }
}
