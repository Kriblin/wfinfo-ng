use crate::theme::Theme;
use image::{DynamicImage, GenericImageView, Pixel, Rgb};
use std::f32::consts::PI;

// Constants for UI elements at 1920x1080
const BASE_WIDTH: f32 = 1920.0;
const BASE_HEIGHT: f32 = 1080.0;
const PIXEL_REWARD_WIDTH: f32 = 968.0;
const PIXEL_REWARD_HEIGHT: f32 = 235.0;
const PIXEL_REWARD_YDISPLAY: f32 = 316.0;
const PIXEL_REWARD_LINE_HEIGHT: f32 = 48.0;

// Text segment markers (multipliers for line height/scaling)
const TEXT_SEGMENTS: [f32; 4] = [2.0, 4.0, 16.0, 21.0];

// Expected row fill ratios for scaling detection
const RATIO_TOP: f32 = 0.06;
const RATIO_MID_LOW: f32 = 0.24;
const RATIO_MID_HIGH: f32 = 0.26;
const RATIO_BOT: f32 = 0.007;

/// Extracts reward box images from the screenshot.
pub fn extract_parts(image: &DynamicImage, theme: Theme) -> Vec<DynamicImage> {
    let screen_scaling = get_screen_scaling(image.width(), image.height());
    let height = image.height() as f32;

    let (most_top, most_bot, most_left, most_width) =
        calculate_prefilter_bounds(image.width(), image.height(), screen_scaling);

    let prefilter = image.crop_imm(most_left, most_top, most_width, most_bot - most_top);

    let rows = calculate_row_histogram(&prefilter, &theme);
    let scaling = find_best_scaling(prefilter.height(), prefilter.width(), &rows, screen_scaling);

    let high_scaling = if scaling < 1.0 {
        scaling + 0.01
    } else {
        scaling
    };
    let low_scaling = if scaling > 0.5 {
        scaling + 0.01
    } else {
        scaling
    };

    let crop_width = PIXEL_REWARD_WIDTH * screen_scaling * high_scaling;
    let crop_left = prefilter.width() as f32 / 2.0 - crop_width / 2.0;

    let crop_top_abs = height / 2.0
        - (PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT + PIXEL_REWARD_LINE_HEIGHT)
            * screen_scaling
            * high_scaling;

    let crop_bot_abs =
        height / 2.0 - (PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT) * screen_scaling * low_scaling;

    let crop_top = crop_top_abs - most_top as f32;
    let crop_height = crop_bot_abs - crop_top_abs;

    let partial_screenshot = DynamicImage::ImageRgb8(prefilter.into_rgb8()).crop_imm(
        crop_left as u32,
        crop_top as u32,
        crop_width as u32,
        crop_height as u32,
    );

    filter_and_separate_parts_from_part_box(partial_screenshot, theme)
}

/// Filters the reward box image and separates it into individual item images.
pub fn filter_and_separate_parts_from_part_box(
    image: DynamicImage,
    theme: Theme,
) -> Vec<DynamicImage> {
    let mut filtered = image.into_rgb8();
    let mut total_even = 0.0;
    let mut total_odd = 0.0;

    let width = filtered.width();
    let height = filtered.height();

    for x in 0..width {
        let mut count = 0;
        for y in 0..height {
            let pixel = filtered.get_pixel_mut(x, y);
            if theme.threshold_filter(*pixel) {
                *pixel = Rgb([0, 0, 0]);
                count += 1;
            } else {
                *pixel = Rgb([255, 255, 255]);
            }
        }

        let count = count.min(height / 3);
        let cosine = (8.0 * x as f32 * PI / width as f32).cos();
        let weight = cosine.powi(3) * count as f32;

        if cosine < 0.0 {
            total_even -= weight;
        } else if cosine > 0.0 {
            total_odd += weight;
        }
    }

    if total_even == 0.0 && total_odd == 0.0 {
        return vec![];
    }

    let box_width = width / 4;
    let (curr_left, player_count) = if total_odd > total_even {
        (box_width / 2, 3)
    } else {
        (0, 4)
    };

    let dynamic_image = DynamicImage::ImageRgb8(filtered);
    (0..player_count)
        .map(|i| dynamic_image.crop_imm(curr_left + i * box_width, 0, box_width, height))
        .collect()
}

fn get_screen_scaling(width: u32, height: u32) -> f32 {
    if width as f32 * BASE_HEIGHT > height as f32 * BASE_WIDTH {
        height as f32 / BASE_HEIGHT
    } else {
        width as f32 / BASE_WIDTH
    }
}

fn calculate_prefilter_bounds(
    width: u32,
    height: u32,
    screen_scaling: f32,
) -> (u32, u32, u32, u32) {
    let width_f = width as f32;
    let height_f = height as f32;
    let most_width = PIXEL_REWARD_WIDTH * screen_scaling;
    let most_left = width_f / 2.0 - most_width / 2.0;

    let most_top = height_f / 2.0
        - ((PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT + PIXEL_REWARD_LINE_HEIGHT)
            * screen_scaling);
    let most_bot =
        height_f / 2.0 - ((PIXEL_REWARD_YDISPLAY - PIXEL_REWARD_HEIGHT) * screen_scaling * 0.5);

    (
        most_top as u32,
        most_bot as u32,
        most_left as u32,
        most_width as u32,
    )
}

fn calculate_row_histogram(image: &DynamicImage, theme: &Theme) -> Vec<usize> {
    (0..image.height())
        .map(|y| {
            (0..image.width())
                .filter(|&x| theme.threshold_filter(image.get_pixel(x, y).to_rgb()))
                .count()
        })
        .collect()
}

fn find_best_scaling(
    image_height: u32,
    image_width: u32,
    rows: &[usize],
    screen_scaling: f32,
) -> f32 {
    let line_height = (PIXEL_REWARD_LINE_HEIGHT / 2.0 * screen_scaling) as usize;
    let top_line_100 = image_height as usize - line_height;
    let top_line_50 = line_height / 2;

    let mut best_scale = 50;
    let mut lowest_weight = f32::MAX;

    for i in 0..50 {
        let scale = 50 + i;
        let scale_f = scale as f32 / 100.0;
        let scale_width = image_width as f32 * scale_f;

        let y_from_top = image_height as usize
            - (i as f32 * (top_line_100 - top_line_50) as f32 / 50.0 + top_line_50 as f32) as usize;

        let text_top = (screen_scaling * TEXT_SEGMENTS[0] * scale_f) as usize;
        let text_top_bot = (screen_scaling * TEXT_SEGMENTS[1] * scale_f) as usize;
        let text_both_bot = (screen_scaling * TEXT_SEGMENTS[2] * scale_f) as usize;
        let text_tail_bot = (screen_scaling * TEXT_SEGMENTS[3] * scale_f) as usize;

        let mut w_top = 0.0;
        for loc in text_top..=text_top_bot {
            w_top += (scale_width * RATIO_TOP - rows[y_from_top + loc] as f32).abs();
        }
        w_top /= (text_top_bot - text_top + 1) as f32;

        let mut w_mid = 0.0;
        for loc in text_top_bot + 1..text_both_bot {
            let row_fill = rows[y_from_top + loc] as f32;
            if row_fill < scale_width / 15.0 {
                w_mid += (scale_width * RATIO_MID_HIGH - row_fill) * 5.0;
            } else {
                w_mid += (scale_width * RATIO_MID_LOW - row_fill).abs();
            }
        }
        w_mid /= (text_both_bot - text_top_bot - 2).max(1) as f32;

        let mut w_bot = 0.0;
        for loc in text_both_bot..text_tail_bot {
            w_bot += 10.0 * (scale_width * RATIO_BOT - rows[y_from_top + loc] as f32).abs();
        }
        w_bot /= (text_tail_bot - text_both_bot - 1).max(1) as f32;

        let total_weight = w_top + w_mid + w_bot;
        if total_weight < lowest_weight {
            lowest_weight = total_weight;
            best_scale = scale;
        }
    }

    best_scale as f32 / 100.0
}
