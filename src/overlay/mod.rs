use eframe::egui;
use log::{debug, info};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Reward {
    pub name: String,
    pub platinum: f32,
    pub ducats: usize,
    pub is_best: bool,
}

pub struct OverlayApp {
    pub rewards: Vec<Reward>,
    pub receiver: Receiver<Vec<Reward>>,
    pub last_update: Option<Instant>,
}

impl OverlayApp {
    pub fn new(receiver: Receiver<Vec<Reward>>) -> Self {
        Self {
            rewards: Vec::new(),
            receiver,
            last_update: None,
        }
    }

    pub fn set_rewards(&mut self, rewards: Vec<Reward>) {
        self.rewards = rewards;
        self.last_update = Some(Instant::now());
    }

    pub fn check_updates(&mut self) -> bool {
        let mut did_update = false;
        if let Ok(rewards) = self.receiver.try_recv() {
            self.rewards = rewards;
            self.last_update = Some(Instant::now());
            did_update = true;
        }

        if let Some(last_update) = self.last_update
            && last_update.elapsed() > Duration::from_secs(10)
        {
            self.rewards.clear();
            self.last_update = None;
        }
        did_update
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let did_update = self.check_updates();

        if did_update {
            debug!(
                "Overlay received update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
                self.rewards.len(),
                self.rewards,
                self.last_update.map(|t| t.elapsed()),
            );
        }

        let panel_frame = egui::Frame {
            fill: egui::Color32::from_black_alpha(150),
            rounding: egui::Rounding::same(8.0),
            inner_margin: egui::Margin::same(10.0),
            ..Default::default()
        };

        if !self.rewards.is_empty() {
            info!(
                "Overlay update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
                self.rewards.len(),
                self.rewards,
                self.last_update.map(|t| t.elapsed()),
            );
            egui::CentralPanel::default()
                .frame(panel_frame)
                .show(ctx, |ui| {
                    ui.heading(egui::RichText::new("Rewards").color(egui::Color32::WHITE));
                    for chunk in self.rewards.chunks(4) {
                        ui.columns(4, |columns| {
                            for (i, reward) in chunk.iter().enumerate() {
                                let ui = &mut columns[i];
                                ui.vertical(|ui| {
                                    if reward.is_best {
                                        ui.label(
                                            egui::RichText::new(&reward.name)
                                                .color(egui::Color32::GREEN)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}p, {}d",
                                                reward.platinum, reward.ducats
                                            ))
                                            .color(egui::Color32::GREEN)
                                            .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new("BEST")
                                                .color(egui::Color32::GOLD)
                                                .strong(),
                                        );
                                    } else {
                                        ui.label(
                                            egui::RichText::new(&reward.name)
                                                .color(egui::Color32::LIGHT_GRAY),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}p, {}d",
                                                reward.platinum, reward.ducats
                                            ))
                                            .color(egui::Color32::LIGHT_GRAY),
                                        );
                                    }
                                });
                            }
                        });
                    }
                });
            ctx.request_repaint();
        } else {
            // Request repaint less frequently when hidden
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_rewards() {
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut app = OverlayApp::new(rx);
        let rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        app.set_rewards(rewards.clone());
        assert_eq!(app.rewards, rewards);
    }

    #[test]
    fn test_receiver_updates_rewards() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = OverlayApp::new(rx);
        let rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        tx.send(rewards.clone()).unwrap();

        // In a real app, update would be called by eframe
        // We can simulate it by calling update with mock objects or just testing the logic
        // Since we want to test that it receives the rewards:
        if let Ok(received) = app.receiver.try_recv() {
            app.rewards = received;
        }
        assert_eq!(app.rewards, rewards);
    }

    #[test]
    fn test_timeout_clears_rewards() {
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut app = OverlayApp::new(rx);
        app.rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        app.last_update = Some(Instant::now() - Duration::from_secs(11));

        app.check_updates();

        assert!(app.rewards.is_empty());
        assert!(app.last_update.is_none());
    }
}
