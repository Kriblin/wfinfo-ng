use std::collections::HashMap;

use image::{DynamicImage, GenericImageView, Pixel};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::error::{Result, ThemeError};

use super::Theme;

const BASE_WIDTH: f32 = 1920.0;
const BASE_HEIGHT: f32 = 1080.0;
const PIXEL_REWARD_WIDTH: f32 = 968.0;
const PIXEL_REWARD_LINE_HEIGHT: f32 = 48.0;

/// Detects the theme from the given screenshot.
pub fn detect_theme(image: &DynamicImage) -> Result<Theme> {
    let screen_scaling = get_screen_scaling(image.width(), image.height());

    let line_height = PIXEL_REWARD_LINE_HEIGHT / 2.0 * screen_scaling;
    let most_width = PIXEL_REWARD_WIDTH * screen_scaling;
    let min_width = most_width / 4.0;

    let weights = (line_height as u32..image.height())
        .into_par_iter()
        .fold(HashMap::new, |mut weights: HashMap<Theme, f32>, y| {
            let total_height = image.height() as f32 - line_height;
            let perc = (y as f32 - line_height) / total_height;
            let total_width = min_width * perc + min_width;
            let x_offset = (most_width - total_width) as u32 / 2;

            for x in 0..total_width as u32 {
                let pixel = image.get_pixel(x + x_offset, y).to_rgb();
                let (closest_theme, distance) = Theme::closest_from_color(pixel);
                *weights.entry(closest_theme).or_insert(0.0) += 1.0 / (1.0 + distance).powi(4);
            }
            weights
        })
        .reduce(HashMap::new, |mut a, b| {
            for (theme, weight) in b {
                *a.entry(theme).or_insert(0.0) += weight;
            }
            a
        });

    weights
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(theme, _)| theme)
        .ok_or_else(|| ThemeError::DetectionError("Failed to detect theme".to_string()).into())
}

fn get_screen_scaling(width: u32, height: u32) -> f32 {
    if width as f32 * BASE_HEIGHT > height as f32 * BASE_WIDTH {
        height as f32 / BASE_HEIGHT
    } else {
        width as f32 / BASE_WIDTH
    }
}
