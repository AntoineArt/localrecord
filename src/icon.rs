use tray_icon::Icon;

const SIZE: u32 = 32;

/// Tray icon: microphone on a dark round badge (red accent when recording).
pub fn tray_icon(recording: bool) -> Icon {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let bg = if recording {
        (210, 45, 45)
    } else {
        (45, 52, 70)
    };
    let fg = (245, 247, 250);
    let accent = if recording { (255, 90, 90) } else { (90, 160, 255) };

    fill_circle(&mut rgba, SIZE / 2, SIZE / 2, 15.0, bg);

    // Mic body
    fill_round_rect(&mut rgba, 11, 8, 21, 20, 4, fg);
    // Mic grille lines
    for y in [11, 14, 17] {
        draw_hline(&mut rgba, 13, 19, y, accent);
    }
    // Mic stem + base
    fill_rect(&mut rgba, 14, 20, 18, 24, fg);
    fill_round_rect(&mut rgba, 10, 24, 22, 27, 2, fg);

    // Sound arc accent
    if recording {
        draw_ring_arc(&mut rgba, 16, 14, 12.0, accent);
    } else {
        draw_ring_arc(&mut rgba, 16, 14, 12.0, (70, 120, 200));
    }

    Icon::from_rgba(rgba, SIZE, SIZE).expect("valid icon")
}

fn fill_circle(rgba: &mut [u8], cx: u32, cy: u32, radius: f32, color: (u8, u8, u8)) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx as f32;
            let dy = y as f32 + 0.5 - cy as f32;
            if dx * dx + dy * dy <= radius * radius {
                set_px(rgba, x, y, color, 255);
            }
        }
    }
}

fn draw_ring_arc(rgba: &mut [u8], cx: u32, cy: u32, radius: f32, color: (u8, u8, u8)) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx as f32;
            let dy = y as f32 + 0.5 - cy as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= radius - 1.0 && dist <= radius + 0.5 && dx > 1.0 {
                set_px(rgba, x, y, color, 220);
            }
        }
    }
}

fn fill_rect(rgba: &mut [u8], x0: u32, y0: u32, x1: u32, y1: u32, color: (u8, u8, u8)) {
    for y in y0..y1.min(SIZE) {
        for x in x0..x1.min(SIZE) {
            set_px(rgba, x, y, color, 255);
        }
    }
}

fn fill_round_rect(
    rgba: &mut [u8],
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    radius: u32,
    color: (u8, u8, u8),
) {
    for y in y0..y1.min(SIZE) {
        for x in x0..x1.min(SIZE) {
            let in_x = x >= x0 + radius && x < x1.saturating_sub(radius);
            let in_y = y >= y0 + radius && y < y1.saturating_sub(radius);
            let corner = !in_x && !in_y;
            let mut draw = in_x || in_y;
            if corner {
                let cx = if x < x0 + radius {
                    x0 + radius
                } else {
                    x1.saturating_sub(radius + 1)
                };
                let cy = if y < y0 + radius {
                    y0 + radius
                } else {
                    y1.saturating_sub(radius + 1)
                };
                let dx = x as i32 - cx as i32;
                let dy = y as i32 - cy as i32;
                draw = (dx * dx + dy * dy) <= (radius * radius) as i32;
            }
            if draw {
                set_px(rgba, x, y, color, 255);
            }
        }
    }
}

fn draw_hline(rgba: &mut [u8], x0: u32, x1: u32, y: u32, color: (u8, u8, u8)) {
    for x in x0..x1.min(SIZE) {
        set_px(rgba, x, y, color, 255);
    }
}

fn set_px(rgba: &mut [u8], x: u32, y: u32, rgb: (u8, u8, u8), a: u8) {
    let idx = ((y * SIZE + x) * 4) as usize;
    if idx + 3 >= rgba.len() {
        return;
    }
    rgba[idx] = rgb.0;
    rgba[idx + 1] = rgb.1;
    rgba[idx + 2] = rgb.2;
    rgba[idx + 3] = a;
}
