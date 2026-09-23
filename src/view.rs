//! Terminal picture of the two panels driven by the controller.

use sliding_door::{DoorPhase, MotionRegime, SlidingDoor};

const POCKET: usize = 12;
const PANEL: usize = 12;
const OPENING: usize = PANEL * 2;
const INNER: usize = POCKET + OPENING + POCKET;
const DOOR_ROWS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Panel,
    Pocket,
    Gap,
}

pub struct RenderOptions {
    pub color: bool,
    pub show_keys: bool,
    pub elapsed_secs: f64,
}

pub fn render(door: &SlidingDoor, options: &RenderOptions) -> String {
    let mut out = String::new();
    let openness = door.openness();
    let percent = openness * 100.0;

    push_line(
        &mut out,
        &format!(
            "Sliding door        t = {:5.2} s",
            options.elapsed_secs.max(0.0)
        ),
    );
    push_line(&mut out, "");
    push_field(&mut out, "Phase", &door.phase().to_string());
    push_field(&mut out, "Motion", &door.regime().to_string());
    push_field(
        &mut out,
        "Openness",
        &format!("{percent:5.1}%  {}", bar(openness)),
    );
    push_field(
        &mut out,
        "Position",
        &format!(
            "{:0.3} m of {:0.3} m",
            door.position(),
            door.config().travel
        ),
    );
    push_field(
        &mut out,
        "Velocity",
        &format!("{:+0.3} m/s", door.velocity()),
    );
    push_field(&mut out, "Dwell", &dwell_text(door));
    push_field(
        &mut out,
        "Presence",
        if door.presence() { "detected" } else { "clear" },
    );
    push_field(
        &mut out,
        "Safety edge",
        if door.obstructed() {
            "tripped"
        } else {
            "clear"
        },
    );
    push_line(&mut out, "");
    push_line(&mut out, &narration(door));
    push_line(&mut out, "");

    let width = INNER;
    push_line(&mut out, &format!("┌{}┐", "─".repeat(width)));
    for row in 0..DOOR_ROWS {
        out.push('│');
        for x in 0..INNER {
            let cell = cell_at(x, openness);
            if row == DOOR_ROWS / 2 && x == opening_center() && cell == Cell::Gap && door.presence()
            {
                paint(&mut out, options.color, "93", "●");
            } else {
                paint_cell(&mut out, options.color, cell);
            }
        }
        out.push('│');
        out.push('\n');
    }
    push_line(&mut out, &format!("└{}┘", "─".repeat(width)));

    let mut ground = vec![' '; INNER];
    // The visitor stands in the opening once the leaves have cleared the
    // center, and in front of the door while the leaves still cover it.
    let center_clear = cell_at(opening_center(), openness) == Cell::Gap;
    if door.presence() && !center_clear {
        ground[opening_center()] = '●';
    }
    let ground: String = ground.into_iter().collect();
    push_line(&mut out, &format!(" {ground} "));
    let caption = if door.presence() {
        "visitor in the sensor zone"
    } else if door.obstructed() {
        "safety edge is held"
    } else {
        "sensor zone clear"
    };
    push_line(&mut out, &center(caption, INNER + 2));

    if options.show_keys {
        push_line(&mut out, "");
        push_line(
            &mut out,
            "p  presence     b  safety edge     r  reset     q  quit",
        );
    }
    out
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

fn push_field(out: &mut String, label: &str, value: &str) {
    push_line(out, &format!("{label:<14}{value}"));
}

fn dwell_text(door: &SlidingDoor) -> String {
    match door.dwell_remaining() {
        Some(remaining) => format!("{remaining:0.2} s remaining"),
        None => "—".to_string(),
    }
}

fn bar(openness: f64) -> String {
    const WIDTH: usize = 24;
    let filled = ((openness.clamp(0.0, 1.0) * WIDTH as f64).round() as usize).min(WIDTH);
    format!("[{}{}]", "#".repeat(filled), "-".repeat(WIDTH - filled))
}

fn narration(door: &SlidingDoor) -> String {
    let text = match (door.phase(), door.regime()) {
        (DoorPhase::Closed, _) => "Shut. Waiting for someone to enter the sensor zone.",
        (DoorPhase::Opening, MotionRegime::Reversing) => {
            "Opening. Braking the old velocity, then reversing toward open."
        }
        (DoorPhase::Opening, MotionRegime::Accelerating) => {
            "Opening. Accelerating up to the speed limit."
        }
        (DoorPhase::Opening, MotionRegime::Cruising) => "Opening. Cruising at the speed limit.",
        (DoorPhase::Opening, MotionRegime::Braking) => {
            "Opening. Braking so the panels stop fully open."
        }
        (DoorPhase::Opening, MotionRegime::Idle) => "Opening. Settling on the open stop.",
        (DoorPhase::Open, _) if door.presence() || door.obstructed() => {
            "Open. The dwell timer stays reset while a sensor is active."
        }
        (DoorPhase::Open, _) => "Open. Dwell is counting down. Then the panels close.",
        (DoorPhase::Closing, MotionRegime::Reversing) => {
            "Closing was interrupted. Reversing back to open."
        }
        (DoorPhase::Closing, MotionRegime::Accelerating) => {
            "Closing. Accelerating the panels toward the shut pose."
        }
        (DoorPhase::Closing, MotionRegime::Cruising) => "Closing. Cruising at the speed limit.",
        (DoorPhase::Closing, MotionRegime::Braking) => "Closing. Braking onto the shut stop.",
        (DoorPhase::Closing, MotionRegime::Idle) => "Closing. Settling on the shut stop.",
    };
    text.to_string()
}

fn center(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let left = (width - len) / 2;
    format!("{}{text}", " ".repeat(left))
}

fn opening_center() -> usize {
    POCKET + OPENING / 2
}

fn slide_amount(openness: f64) -> usize {
    let slide = (PANEL as f64 * openness.clamp(0.0, 1.0)).round() as usize;
    slide.min(PANEL)
}

fn cell_at(x: usize, openness: f64) -> Cell {
    let slide = slide_amount(openness) as isize;
    let panel = PANEL as isize;
    let pocket = POCKET as isize;
    let left = pocket - slide;
    let right = pocket + panel + slide;
    let xi = x as isize;
    if (xi >= left && xi < left + panel) || (xi >= right && xi < right + panel) {
        Cell::Panel
    } else if xi < pocket || xi >= pocket + OPENING as isize {
        Cell::Pocket
    } else {
        Cell::Gap
    }
}

fn paint_cell(out: &mut String, color: bool, cell: Cell) {
    match cell {
        Cell::Panel => paint(out, color, "38;5;39", "█"),
        Cell::Pocket => paint(out, color, "38;5;236", "▓"),
        Cell::Gap => out.push(' '),
    }
}

fn paint(out: &mut String, color: bool, code: &str, glyph: &str) {
    if color {
        out.push_str("\u{1b}[");
        out.push_str(code);
        out.push('m');
        out.push_str(glyph);
        out.push_str("\u{1b}[0m");
    } else {
        out.push_str(glyph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliding_door::DoorConfig;

    #[test]
    fn closed_panels_cover_the_opening_and_open_panels_clear_it() {
        for x in 0..POCKET {
            assert_eq!(cell_at(x, 0.0), Cell::Pocket);
            assert_eq!(cell_at(x, 1.0), Cell::Panel);
        }
        for x in POCKET..(POCKET + OPENING) {
            assert_eq!(cell_at(x, 0.0), Cell::Panel);
            assert_eq!(cell_at(x, 1.0), Cell::Gap);
        }
        for x in (POCKET + OPENING)..INNER {
            assert_eq!(cell_at(x, 0.0), Cell::Pocket);
            assert_eq!(cell_at(x, 1.0), Cell::Panel);
        }

        // Half open: each panel has slid six columns, leaving a gap in the middle.
        for x in 18..30 {
            assert_eq!(cell_at(x, 0.5), Cell::Gap);
        }
        assert_eq!(cell_at(0, 0.5), Cell::Pocket);
        assert_eq!(cell_at(6, 0.5), Cell::Panel);
        assert_eq!(cell_at(47, 0.5), Cell::Pocket);
    }

    #[test]
    fn picture_follows_the_controller() {
        let mut door = SlidingDoor::new(DoorConfig {
            travel: 1.0,
            max_speed: 4.0,
            acceleration: 16.0,
            dwell: 2.0,
            epsilon: 1e-3,
        })
        .unwrap();
        let options = RenderOptions {
            color: false,
            show_keys: true,
            elapsed_secs: 0.0,
        };
        let shut = render(&door, &options);
        assert!(shut.contains("Closed"));
        assert!(shut.contains("sensor zone clear"));
        assert!(shut.contains("▓▓▓▓████"));
        assert!(shut.contains('p'));
        assert!(!shut.contains('\u{1b}'));

        door.set_presence(true);
        door.step(2.0);
        let open = render(
            &door,
            &RenderOptions {
                color: false,
                show_keys: false,
                elapsed_secs: 2.0,
            },
        );
        assert!(open.contains("Open"));
        assert!(open.contains("visitor in the sensor zone"));
        assert!(open.contains("████████████            ●"));
        assert!(!open.contains("q  quit"));

        let colored = render(
            &door,
            &RenderOptions {
                color: true,
                show_keys: false,
                elapsed_secs: 2.0,
            },
        );
        assert!(colored.contains("\u{1b}[38;5;39m"));
    }
}
