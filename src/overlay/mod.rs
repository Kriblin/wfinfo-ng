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

#[derive(Clone, Debug, Default)]
pub struct OverlayState {
    pub rewards: Vec<Reward>,
    pub last_update: Option<Instant>,
}

impl OverlayState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_rewards(&mut self, rewards: Vec<Reward>) {
        self.rewards = rewards;
        self.last_update = Some(Instant::now());
    }

    pub fn try_receive(&mut self, receiver: &Receiver<Vec<Reward>>) -> bool {
        let mut did_update = false;
        while let Ok(rewards) = receiver.try_recv() {
            self.rewards = rewards;
            self.last_update = Some(Instant::now());
            did_update = true;
        }
        did_update
    }

    pub fn clear_if_timed_out(&mut self, timeout: Duration) {
        if let Some(last_update) = self.last_update
            && last_update.elapsed() > timeout
        {
            self.rewards.clear();
            self.last_update = None;
        }
    }

    pub fn is_visible(&self) -> bool {
        !self.rewards.is_empty()
    }
}

pub fn draw_overlay(ctx: &egui::Context, state: &OverlayState) {
    let panel_frame = egui::Frame {
        fill: egui::Color32::TRANSPARENT,
        corner_radius: egui::CornerRadius::same(8),
        inner_margin: egui::Margin::same(10),
        ..Default::default()
    };

    if !state.rewards.is_empty() {
        info!(
            "Overlay update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
            state.rewards.len(),
            state.rewards,
            state.last_update.map(|t| t.elapsed()),
        );
        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.heading(egui::RichText::new("Rewards").color(egui::Color32::WHITE));
                for chunk in state.rewards.chunks(4) {
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

pub struct OverlayApp {
    state: OverlayState,
    receiver: Receiver<Vec<Reward>>,
}

impl OverlayApp {
    pub fn new(receiver: Receiver<Vec<Reward>>) -> Self {
        Self {
            state: OverlayState::new(),
            receiver,
        }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let did_update = self.state.try_receive(&self.receiver);
        self.state.clear_if_timed_out(Duration::from_secs(10));

        if did_update {
            debug!(
                "Overlay received update: rewards_count={}, rewards={:?}, last_update_elapsed={:?}",
                self.state.rewards.len(),
                self.state.rewards,
                self.state.last_update.map(|t| t.elapsed()),
            );
        }

        draw_overlay(ctx, &self.state);
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
        let mut state = OverlayState::new();
        let rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        state.set_rewards(rewards.clone());
        assert_eq!(state.rewards, rewards);
    }

    #[test]
    fn test_receiver_updates_rewards() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = OverlayState::new();
        let rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        tx.send(rewards.clone()).unwrap();

        state.try_receive(&rx);
        assert_eq!(state.rewards, rewards);
    }

    #[test]
    fn test_timeout_clears_rewards() {
        let mut state = OverlayState::new();
        state.rewards = vec![Reward {
            name: "Test Item".to_string(),
            platinum: 10.0,
            ducats: 15,
            is_best: true,
        }];
        state.last_update = Some(Instant::now() - Duration::from_secs(11));

        state.clear_if_timed_out(Duration::from_secs(10));

        assert!(state.rewards.is_empty());
        assert!(state.last_update.is_none());
    }
}
