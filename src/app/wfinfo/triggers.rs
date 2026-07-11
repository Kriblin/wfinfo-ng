use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::PathBuf,
    sync::mpsc,
    thread::{self, sleep},
    time::Duration,
};

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use log::{debug, error, info};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

pub(super) fn log_watcher(path: PathBuf, event_sender: mpsc::Sender<()>, capture_delay_ms: u64) {
    debug!("Path: {}", path.display());
    let mut position = match File::open(&path) {
        Ok(mut file) => match file.seek(SeekFrom::End(0)) {
            Ok(position) => position,
            Err(err) => {
                error!("Failed to seek to end of file {}: {}", path.display(), err);
                return;
            }
        },
        Err(err) => {
            error!("Failed to open file {}: {}", path.display(), err);
            return;
        }
    };

    thread::spawn(move || {
        debug!("Position: {}", position);

        let (sender, receiver) = mpsc::channel();
        let watcher_config =
            notify::Config::default().with_poll_interval(Duration::from_millis(100));
        let mut watcher = match RecommendedWatcher::new(sender, watcher_config) {
            Ok(watcher) => watcher,
            Err(err) => {
                error!("Failed to create file watcher: {}", err);
                return;
            }
        };

        if let Err(err) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            error!("Failed to watch file {}: {}", path.display(), err);
            return;
        }

        loop {
            match receiver.recv() {
                Ok(event) => {
                    if event.unwrap().kind.is_modify() {
                        let mut file = match File::open(&path) {
                            Ok(file) => file,
                            Err(err) => {
                                error!("Failed to open file {}: {}", path.display(), err);
                                continue;
                            }
                        };

                        if let Err(err) = file.seek(SeekFrom::Start(position)) {
                            error!(
                                "Failed to seek to position {} in file {}: {}",
                                position,
                                path.display(),
                                err
                            );
                            continue;
                        }

                        let mut reward_screen_detected = false;
                        let reader = BufReader::new(file.by_ref());
                        for line in reader.lines() {
                            let line = match line {
                                Ok(line) => line,
                                Err(err) => {
                                    error!("Error reading line: {}", err);
                                    continue;
                                }
                            };
                            if line.contains("Pause countdown done")
                                || line.contains("Got rewards")
                                || line
                                    .contains("Created /Lotus/Interface/ProjectionRewardChoice.swf")
                            {
                                reward_screen_detected = true;
                            }
                        }

                        if reward_screen_detected {
                            info!("Detected, waiting for {} ms...", capture_delay_ms);
                            sleep(Duration::from_millis(capture_delay_ms));
                            if let Err(err) = event_sender.send(()) {
                                error!("Failed to send event: {}", err);
                            }
                        }

                        position = match file.metadata() {
                            Ok(metadata) => metadata.len(),
                            Err(err) => {
                                error!("Failed to get file metadata: {}", err);
                                continue;
                            }
                        };
                        debug!("Log position: {}", position);
                    }
                }
                Err(err) => error!("Error: {:?}", err),
            }
        }
    });
}

pub(super) fn hotkey_watcher(hotkey: HotKey, event_sender: mpsc::Sender<()>) {
    debug!("Watching hotkey: {hotkey:?}");
    thread::spawn(move || {
        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => manager,
            Err(err) => {
                error!("Failed to create hotkey manager: {}", err);
                return;
            }
        };

        if let Err(err) = manager.register(hotkey) {
            error!("Failed to register hotkey {:?}: {}", hotkey, err);
            return;
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
            debug!("Hotkey event: {:?}", event);
            if event.state == HotKeyState::Pressed
                && let Err(err) = event_sender.send(()) {
                    error!("Failed to send event: {}", err);
                }
        }
    });
}
