//! Two panels slide apart, then back together.
//!
//! `openness` is 0 when the door is shut and 1 when it is open.
//! The gap in the middle is that fraction of the doorway. Each panel
//! gets half of whatever space is left.

const DOOR_WIDTH: usize = 20;

fn main() {
    println!("sliding door\n");
    for openness in frames() {
        println!("{}   {:3.0}%", draw(openness), openness * 100.0);
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

/// Shut, then fully open, then shut again.
fn frames() -> Vec<f64> {
    let steps = 10;
    let mut frames = Vec::new();
    for step in 0..=steps {
        frames.push(step as f64 / steps as f64);
    }
    for step in (0..steps).rev() {
        frames.push(step as f64 / steps as f64);
    }
    frames
}

/// Draw the doorway at this openness.
fn draw(openness: f64) -> String {
    let gap = (openness.clamp(0.0, 1.0) * DOOR_WIDTH as f64).round() as usize;
    let left = (DOOR_WIDTH - gap) / 2;
    let right = DOOR_WIDTH - gap - left;
    format!(
        "[{}{}{}]",
        "#".repeat(left),
        " ".repeat(gap),
        "#".repeat(right)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shut_door_is_solid() {
        assert_eq!(draw(0.0), "[####################]");
    }

    #[test]
    fn an_open_door_is_a_gap() {
        assert_eq!(draw(1.0), "[                    ]");
    }

    #[test]
    fn halfway_splits_the_two_panels() {
        assert_eq!(draw(0.5), "[#####          #####]");
    }

    #[test]
    fn the_demo_opens_and_then_closes() {
        let frames = frames();
        assert_eq!(frames.first().copied(), Some(0.0));
        assert!(frames.contains(&1.0));
        assert_eq!(frames.last().copied(), Some(0.0));
    }
}
