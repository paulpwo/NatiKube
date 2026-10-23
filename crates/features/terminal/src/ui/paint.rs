//! Cell grid painting: backgrounds, text runs, cursor and scroll indicator.
//! This is our own renderer; it only consumes the pure [`Snapshot`].

use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use gpui_kit::*;

use crate::domain::{GridMetrics, Snapshot};

pub const FONT_FAMILY: &str = "Menlo";
/// Default terminal font size in px.
pub const DEFAULT_FONT_SIZE: f32 = 12.0;
pub const LINE_HEIGHT_FACTOR: f32 = 1.3;

/// Process-wide terminal font (Settings → Terminal): size in px and an
/// optional family override. Read on every paint, so a change applies live.
static STYLE: std::sync::RwLock<(f32, Option<String>)> =
    std::sync::RwLock::new((DEFAULT_FONT_SIZE, None));

/// Sets the terminal font size (px) and monospace family (`None` = Menlo).
pub fn set_font(size: f32, family: Option<String>) {
    if let Ok(mut style) = STYLE.write() {
        *style = (size.clamp(6.0, 48.0), family.filter(|f| !f.is_empty()));
    }
}

/// Current terminal font size in px.
pub fn current_font_size() -> f32 {
    STYLE.read().map(|s| s.0).unwrap_or(DEFAULT_FONT_SIZE)
}

fn family() -> SharedString {
    STYLE
        .read()
        .ok()
        .and_then(|s| s.1.clone())
        .map(SharedString::from)
        .unwrap_or_else(|| SharedString::from(FONT_FAMILY))
}

pub fn to_hsla(color: Rgb) -> Hsla {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

pub fn from_hsla(color: Hsla) -> Rgb {
    let rgba: Rgba = color.into();
    Rgb {
        r: (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        g: (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        b: (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

/// Cell size in logical pixels for the terminal font.
pub fn cell_size(window: &Window) -> (f32, f32) {
    let font_size = px(current_font_size());
    let font_id = window.text_system().resolve_font(&font(family()));
    let width = window
        .text_system()
        .advance(font_id, font_size, 'm')
        .map(|size| size.width)
        .unwrap_or(px(current_font_size() * 0.6));
    let width: f32 = width.into();
    // Whole pixel line height keeps rows crisp.
    (
        (width * 100.0).round() / 100.0,
        (current_font_size() * LINE_HEIGHT_FACTOR).ceil(),
    )
}

/// Everything the paint pass needs, gathered during prepaint.
pub struct Frame {
    pub snapshot: Snapshot,
    pub metrics: GridMetrics,
    pub focused: bool,
    pub cursor_color: Rgb,
    pub background: Rgb,
}

pub fn paint(bounds: Bounds<Pixels>, frame: Frame, window: &mut Window, cx: &mut App) {
    let Frame {
        snapshot,
        metrics,
        focused,
        cursor_color,
        background,
    } = frame;
    let (cell_w, cell_h) = (px(metrics.cell_width), px(metrics.cell_height));
    let origin = bounds.origin;
    let font_size = px(current_font_size());

    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        // Pass 1: backgrounds.
        for (row, runs) in snapshot.lines.iter().enumerate() {
            let y = origin.y + cell_h * row as f32;
            for run in runs {
                if let Some(bg) = run.bg {
                    let x = origin.x + cell_w * run.col as f32;
                    window.paint_quad(fill(
                        Bounds::new(point(x, y), size(cell_w * run.cells as f32, cell_h)),
                        to_hsla(bg),
                    ));
                }
            }
        }

        // Pass 2: text.
        for (row, runs) in snapshot.lines.iter().enumerate() {
            let y = origin.y + cell_h * row as f32;
            for run in runs {
                if run.text.chars().all(|c| c == ' ') && !run.underline && !run.strikeout {
                    continue;
                }
                let mut run_font = font(family());
                if run.bold {
                    run_font = run_font.bold();
                }
                if run.italic {
                    run_font = run_font.italic();
                }
                let color = to_hsla(run.fg);
                let text_run = TextRun {
                    len: run.text.len(),
                    font: run_font,
                    color,
                    background_color: None,
                    underline: run.underline.then_some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(color),
                        wavy: false,
                    }),
                    strikethrough: run.strikeout.then_some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(color),
                    }),
                };
                let force = cell_w * (run.cells as f32 / run.text.chars().count().max(1) as f32);
                let shaped = window.text_system().shape_line(
                    SharedString::from(run.text.clone()),
                    font_size,
                    &[text_run],
                    Some(force),
                );
                let x = origin.x + cell_w * run.col as f32;
                let _ = shaped.paint(point(x, y), cell_h, TextAlign::Left, None, window, cx);
            }
        }

        // Cursor.
        if let Some(cursor) = snapshot.cursor {
            let x = origin.x + cell_w * cursor.col as f32;
            let y = origin.y + cell_h * cursor.row as f32;
            let width = cell_w * cursor.cells as f32;
            let color = to_hsla(cursor_color);
            let cell_bounds = Bounds::new(point(x, y), size(width, cell_h));
            let hollow = !focused || cursor.shape == CursorShape::HollowBlock;
            if hollow && cursor.shape != CursorShape::Beam && cursor.shape != CursorShape::Underline
            {
                window.paint_quad(outline(cell_bounds, color, BorderStyle::Solid));
            } else {
                match cursor.shape {
                    CursorShape::Beam => window
                        .paint_quad(fill(Bounds::new(point(x, y), size(px(2.), cell_h)), color)),
                    CursorShape::Underline => window.paint_quad(fill(
                        Bounds::new(point(x, y + cell_h - px(2.)), size(width, px(2.))),
                        color,
                    )),
                    _ => {
                        window.paint_quad(fill(cell_bounds, color));
                        // Redraw the glyph in the background color so it stays legible.
                        if let Some(ch) = snapshot.char_at(cursor.row, cursor.col)
                            && ch != ' '
                        {
                            let text = ch.to_string();
                            let text_run = TextRun {
                                len: text.len(),
                                font: font(family()),
                                color: to_hsla(background),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from(text),
                                font_size,
                                &[text_run],
                                Some(width),
                            );
                            let _ = shaped.paint(
                                point(x, y),
                                cell_h,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                    }
                }
            }
        }

        // Scroll position indicator while looking at history.
        if snapshot.display_offset > 0 && snapshot.history_size > 0 {
            let total = (snapshot.history_size + snapshot.rows) as f32;
            let track = bounds.size.height;
            let thumb_h = (track * (snapshot.rows as f32 / total)).max(px(16.));
            let above = (snapshot.history_size - snapshot.display_offset) as f32 / total;
            let thumb_y = bounds.origin.y
                + (track - thumb_h)
                    * (above / (1.0 - snapshot.rows as f32 / total)).clamp(0.0, 1.0);
            let thumb = Bounds::new(
                point(bounds.origin.x + bounds.size.width - px(6.), thumb_y),
                size(px(4.), thumb_h),
            );
            let mut color = to_hsla(cursor_color);
            color.a = 0.5;
            window.paint_quad(quad(
                thumb,
                px(2.),
                color,
                0.,
                transparent_black(),
                BorderStyle::Solid,
            ));
        }
    });
}
