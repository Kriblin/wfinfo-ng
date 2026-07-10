use std::{
    sync::mpsc::{
        Receiver, Sender,
        TryRecvError::{Disconnected, Empty},
        channel,
    },
    thread,
};

use eframe::egui;
use eframe::egui::Key;
use eframe::epaint::ColorImage;
use image::{DynamicImage, ImageReader, Rgb};
use palette::{FromColor, Hsl, Srgb};
use wfinfo::{
    config::Config,
    database::Database,
    ocr::{self, normalize_string},
    theme::{HslRange, Theme},
    utils::ensure_database_files,
};

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Tune theme detection",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
    .expect("TODO: panic message");
}

struct MyApp {
    original_images: Vec<DynamicImage>,
    selected_image_index: usize,
    image: Option<egui::TextureHandle>,

    ocr_request_sender: Sender<(usize, HslRange<f32>)>,
    ocr_response_receiver: Receiver<Vec<(String, String)>>,
    ocr_result: Option<Vec<(String, String)>>,

    settings: HslRange<f32>,
}

impl Default for MyApp {
    fn default() -> Self {
        let original_images = std::env::args()
            .skip(1)
            .filter_map(|name| match ImageReader::open(&name) {
                Ok(reader) => match reader.decode() {
                    Ok(image) => Some(image),
                    Err(e) => {
                        eprintln!("Error decoding image {}: {}", name, e);
                        None
                    }
                },
                Err(e) => {
                    eprintln!("Error opening image {}: {}", name, e);
                    None
                }
            })
            .collect();
        let settings = HslRange {
            saturation: 0.50..1.0,
            lightness: 0.15..1.0,
            hue: -10.0..10.0,
        };
        let (ocr_request_sender, ocr_response_receiver) = spawn_ocr_thread(&original_images);
        Self {
            original_images,
            selected_image_index: 0,
            image: None,

            ocr_request_sender,
            ocr_response_receiver,
            ocr_result: None,

            settings,
        }
    }
}

#[allow(clippy::type_complexity)]
fn spawn_ocr_thread(
    images: &Vec<DynamicImage>,
) -> (
    Sender<(usize, HslRange<f32>)>,
    Receiver<Vec<(String, String)>>,
) {
    let (request_sender, request_receiver): (Sender<_>, Receiver<_>) = channel();
    let (response_sender, response_receiver) = channel();
    let images = images.to_owned();

    thread::spawn(move || {
        let (prices_path, items_path) = match ensure_database_files(&Config::default()) {
            Ok(paths) => paths,
            Err(e) => {
                eprintln!("Error updating database files: {}", e);
                if let Err(send_err) = response_sender.send(vec![(
                    "ERROR".to_string(),
                    format!("Failed to update database files: {}", e),
                )]) {
                    eprintln!("Error sending database error: {}", send_err);
                }
                return;
            }
        };

        // Try to load the database
        let database = match Database::load_from_file(Some(&prices_path), Some(&items_path)) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Error loading database: {}", e);
                // Send an error message to the UI and exit the thread
                if let Err(send_err) = response_sender.send(vec![(
                    "ERROR".to_string(),
                    format!("Failed to load database: {}", e),
                )]) {
                    eprintln!("Error sending database error: {}", send_err);
                }
                return;
            }
        };

        loop {
            // Wait for a request
            let (mut index, mut last_request): (usize, HslRange<f32>) =
                match request_receiver.recv() {
                    Ok(request) => request,
                    Err(e) => {
                        eprintln!("Error receiving request: {}", e);
                        return; // Exit the thread if the channel is closed
                    }
                };

            // Process any additional requests that came in while we were processing
            loop {
                match request_receiver.try_recv() {
                    Ok(request) => (index, last_request) = request,
                    Err(Empty) => break,
                    Err(Disconnected) => return,
                }
            }

            // Ensure index is valid
            if index >= images.len() {
                eprintln!("Invalid image index: {}", index);
                continue;
            }

            let image: &DynamicImage = &images[index];
            let (strings, _theme) = ocr::reward_image_to_reward_names(
                image.clone(),
                Some(Theme::Custom(last_request.to_ordered())),
            )
            .unwrap_or_else(|e| {
                eprintln!("OCR error: {:?}", e);
                (vec![], Theme::Vitruvian)
            });

            let results = strings
                .iter()
                .map(|string| {
                    let item = database.find_item(&normalize_string(string), None);
                    (
                        string.to_owned(),
                        item.map(|item| item.drop_name.to_owned())
                            .unwrap_or_else(|| "None".to_string()),
                    )
                })
                .collect();

            if let Err(e) = response_sender.send(results) {
                eprintln!("Error sending OCR results: {}", e);
                return; // Exit the thread if the channel is closed
            }
        }
    });

    (request_sender, response_receiver)
}

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // Handle next image key press
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::N))
            && !self.original_images.is_empty()
        {
            self.selected_image_index =
                (self.selected_image_index + 1) % self.original_images.len();
            self.image = None;

            if let Err(e) = self
                .ocr_request_sender
                .send((self.selected_image_index, self.settings.clone()))
            {
                eprintln!("Error sending OCR request: {}", e);
            }

            self.ocr_result = None;
        }

        // Handle previous image key press
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::P))
            && !self.original_images.is_empty()
        {
            self.selected_image_index = if self.selected_image_index == 0 {
                self.original_images.len() - 1
            } else {
                self.selected_image_index - 1
            };
            self.image = None;

            if let Err(e) = self
                .ocr_request_sender
                .send((self.selected_image_index, self.settings.clone()))
            {
                eprintln!("Error sending OCR request: {}", e);
            }

            self.ocr_result = None;
        }

        // Process image if needed
        if self.image.is_none() && !self.original_images.is_empty() {
            let image = self.process_image(&self.original_images[self.selected_image_index]);
            self.image = Some(convert_image(ctx, &image));

            if let Err(e) = self
                .ocr_request_sender
                .send((self.selected_image_index, self.settings.clone()))
            {
                eprintln!("Error sending OCR request: {}", e);
            }
        }

        // Check for OCR results
        match self.ocr_response_receiver.try_recv() {
            Ok(response) => self.ocr_result = Some(response),
            Err(Empty) => {} // No new results yet, that's fine
            Err(Disconnected) => {
                eprintln!("OCR thread disconnected");
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.vertical(|ui| {
            match self.image.as_ref() {
                Some(texture) => {
                    let size = texture.size_vec2();
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size * 3.0));
                    if let Some(detections) = self.ocr_result.as_ref() {
                        ui.label(format!("{:#?}", detections));
                    } else {
                        ui.spinner();
                    }
                }
                None => {
                    ui.spinner();
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if ui
                    .add(
                        egui::Slider::new(&mut self.settings.saturation.start, 0.0..=1.0)
                            .text("Saturation min"),
                    )
                    .changed()
                    || ui
                        .add(
                            egui::Slider::new(&mut self.settings.saturation.end, 0.0..=1.0)
                                .text("Saturation max"),
                        )
                        .changed()
                    || ui
                        .add(
                            egui::Slider::new(&mut self.settings.lightness.start, 0.0..=1.0)
                                .text("Lightness min"),
                        )
                        .changed()
                    || ui
                        .add(
                            egui::Slider::new(&mut self.settings.lightness.end, 0.0..=1.0)
                                .text("Lightness max"),
                        )
                        .changed()
                    || ui
                        .add(
                            egui::Slider::new(&mut self.settings.hue.start, -180.0..=180.0)
                                .text("Hue min"),
                        )
                        .changed()
                    || ui
                        .add(
                            egui::Slider::new(&mut self.settings.hue.end, -180.0..=180.0)
                                .text("Hue max"),
                        )
                        .changed()
                {
                    self.image = None;
                    if let Err(e) = self
                        .ocr_request_sender
                        .send((self.selected_image_index, self.settings.clone()))
                    {
                        eprintln!("Error sending OCR request after slider change: {}", e);
                    }
                    self.ocr_result = None;
                };
            });
        });
    }
}

impl MyApp {
    fn process_image(&self, image: &DynamicImage) -> DynamicImage {
        const PIXEL_REWARD_WIDTH: f32 = 968.0;
        const PIXEL_REWARD_HEIGHT: f32 = 235.0;
        const PIXEL_REWARD_YDISPLAY: f32 = 316.0;
        const PIXEL_REWARD_LINE_HEIGHT: f32 = 48.0;

        let screen_scaling = if image.width() * 9 > image.height() * 16 {
            image.height() as f32 / 1080.0
        } else {
            image.width() as f32 / 1920.0
        };

        let width = image.width() as f32;
        let height = image.height() as f32;
        let most_width = PIXEL_REWARD_WIDTH * screen_scaling;
        let most_left = width / 2.0 - most_width / 2.0;
        // Most Top = pixleRewardYDisplay - pixleRewardHeight + pixelRewardLineHeight
        //                   (316          -        235        +       44)    *    1.1    =    137
        let most_top = height / 2.0
            - ((PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT + PIXEL_REWARD_LINE_HEIGHT)
                * screen_scaling);
        let most_bot =
            height / 2.0 - ((PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT) * screen_scaling * 0.5);

        let mut new_image = image
            .crop_imm(
                most_left as u32,
                most_top as u32,
                most_width as u32,
                (most_bot - most_top) as u32,
            )
            .to_rgb8();

        for pixel in new_image.pixels_mut() {
            let rgb = Srgb::from_components((
                pixel.0[0] as f32 / 255.0,
                pixel.0[1] as f32 / 255.0,
                pixel.0[2] as f32 / 255.0,
            ));
            let test = Hsl::from_color(rgb);

            let is_theme = self.settings.saturation.contains(&test.saturation)
                && self.settings.lightness.contains(&test.lightness)
                && self.settings.hue.contains(&test.hue.into_degrees());

            *pixel = if is_theme { Rgb([0; 3]) } else { Rgb([255; 3]) }
        }

        DynamicImage::ImageRgb8(new_image)
    }
}

fn convert_image(ctx: &egui::Context, original_image: &DynamicImage) -> egui::TextureHandle {
    let ui_image = ColorImage::from_rgba_unmultiplied(
        [original_image.width() as _, original_image.height() as _],
        &original_image.to_rgba8(),
    );
    ctx.load_texture("Temp", ui_image, egui::TextureOptions::NEAREST)
}
