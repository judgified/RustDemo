//! Scripted walk-up that exercises open, dwell, reverse, and the safety edge.

use sliding_door::{DoorConfig, DoorPhase, MotionRegime, SlidingDoor};

use crate::view::{render, RenderOptions};

/// Demo limits. Fast enough to watch, slow enough to see each ramp.
pub fn demo_config() -> DoorConfig {
    DoorConfig {
        travel: 1.0,
        max_speed: 0.9,
        acceleration: 1.8,
        dwell: 1.4,
        epsilon: 1e-3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Beat {
    IdleClosed,
    WaitUntilOpen,
    HoldOpen,
    WaitUntilHalfClosed,
    WaitUntilReopened,
    HoldOpenAgain,
    WaitUntilHalfClosedAgain,
    WaitUntilSafetyOpen,
    HoldSafety,
    WaitUntilClosed,
    Done,
}

impl Beat {
    fn label(self) -> &'static str {
        match self {
            Beat::IdleClosed => "idle",
            Beat::WaitUntilOpen => "visitor arrives",
            Beat::HoldOpen | Beat::HoldOpenAgain => "holding open",
            Beat::WaitUntilHalfClosed | Beat::WaitUntilHalfClosedAgain => "visitor leaves",
            Beat::WaitUntilReopened => "visitor returns",
            Beat::WaitUntilSafetyOpen => "safety edge",
            Beat::HoldSafety => "edge held",
            Beat::WaitUntilClosed => "edge clears",
            Beat::Done => "done",
        }
    }
}

struct Sensors {
    presence: bool,
    obstructed: bool,
    next: Option<Beat>,
}

pub struct Sample {
    pub time: f64,
    pub phase: DoorPhase,
    pub regime: MotionRegime,
    pub position: f64,
    pub velocity: f64,
    pub openness: f64,
    pub presence: bool,
    pub obstructed: bool,
    pub beat: &'static str,
    pub frame: Option<String>,
}

/// Run the scripted visitor and return one sample per integration tick.
pub fn simulate() -> Vec<Sample> {
    let config = demo_config();
    let mut door = SlidingDoor::new(config).expect("demo configuration is valid");
    let mut beat = Beat::IdleClosed;
    let mut beat_time = 0.0;
    let mut time = 0.0;
    let dt = 0.01;
    let mut samples = Vec::new();
    let mut previous_phase = door.phase();

    samples.push(take_sample(&door, time, beat, true));

    loop {
        time += dt;
        beat_time += dt;
        let sensors = advance(&door, beat, beat_time);
        if let Some(next) = sensors.next {
            beat = next;
            beat_time = 0.0;
        }
        door.set_presence(sensors.presence);
        door.set_obstructed(sensors.obstructed);
        door.step(dt);

        let phase_changed = door.phase() != previous_phase;
        previous_phase = door.phase();
        samples.push(take_sample(&door, time, beat, phase_changed));

        if beat == Beat::Done && beat_time >= 0.4 {
            break;
        }
        if time > 60.0 {
            break;
        }
    }
    samples
}

fn take_sample(door: &SlidingDoor, time: f64, beat: Beat, capture_frame: bool) -> Sample {
    let frame = if capture_frame {
        Some(render(
            door,
            &RenderOptions {
                color: false,
                show_keys: false,
                elapsed_secs: time,
            },
        ))
    } else {
        None
    };
    Sample {
        time,
        phase: door.phase(),
        regime: door.regime(),
        position: door.position(),
        velocity: door.velocity(),
        openness: door.openness(),
        presence: door.presence(),
        obstructed: door.obstructed(),
        beat: beat.label(),
        frame,
    }
}

fn advance(door: &SlidingDoor, beat: Beat, beat_time: f64) -> Sensors {
    let travel = door.config().travel;
    match beat {
        Beat::IdleClosed => {
            if beat_time >= 0.40 {
                Sensors {
                    presence: true,
                    obstructed: false,
                    next: Some(Beat::WaitUntilOpen),
                }
            } else {
                clear()
            }
        }
        Beat::WaitUntilOpen => hold_until_open(door, true, false, Beat::HoldOpen),
        Beat::HoldOpen => {
            if beat_time >= 0.60 {
                Sensors {
                    presence: false,
                    obstructed: false,
                    next: Some(Beat::WaitUntilHalfClosed),
                }
            } else {
                Sensors {
                    presence: true,
                    obstructed: false,
                    next: None,
                }
            }
        }
        Beat::WaitUntilHalfClosed => {
            half_closed(door, travel, Beat::WaitUntilReopened, true, false)
        }
        Beat::WaitUntilReopened => hold_until_open(door, true, false, Beat::HoldOpenAgain),
        Beat::HoldOpenAgain => {
            if beat_time >= 0.40 {
                Sensors {
                    presence: false,
                    obstructed: false,
                    next: Some(Beat::WaitUntilHalfClosedAgain),
                }
            } else {
                Sensors {
                    presence: true,
                    obstructed: false,
                    next: None,
                }
            }
        }
        Beat::WaitUntilHalfClosedAgain => {
            half_closed(door, travel, Beat::WaitUntilSafetyOpen, false, true)
        }
        Beat::WaitUntilSafetyOpen => hold_until_open(door, false, true, Beat::HoldSafety),
        Beat::HoldSafety => {
            if beat_time >= 0.40 {
                Sensors {
                    presence: false,
                    obstructed: false,
                    next: Some(Beat::WaitUntilClosed),
                }
            } else {
                Sensors {
                    presence: false,
                    obstructed: true,
                    next: None,
                }
            }
        }
        Beat::WaitUntilClosed => {
            if door.phase() == DoorPhase::Closed {
                Sensors {
                    presence: false,
                    obstructed: false,
                    next: Some(Beat::Done),
                }
            } else {
                clear()
            }
        }
        Beat::Done => clear(),
    }
}

fn clear() -> Sensors {
    Sensors {
        presence: false,
        obstructed: false,
        next: None,
    }
}

fn hold_until_open(door: &SlidingDoor, presence: bool, obstructed: bool, next: Beat) -> Sensors {
    Sensors {
        presence,
        obstructed,
        next: if door.phase() == DoorPhase::Open {
            Some(next)
        } else {
            None
        },
    }
}

fn half_closed(
    door: &SlidingDoor,
    travel: f64,
    next: Beat,
    presence: bool,
    obstructed: bool,
) -> Sensors {
    if door.phase() == DoorPhase::Closing && door.position() <= travel * 0.6 {
        Sensors {
            presence,
            obstructed,
            next: Some(next),
        }
    } else {
        clear()
    }
}

pub fn format_demo(samples: &[Sample]) -> String {
    let config = demo_config();
    let mut out = String::new();
    out.push_str("Sliding door algorithm\n");
    out.push_str("======================\n");
    out.push_str("Each tick the controller:\n");
    out.push_str("  1. Reads the presence sensor and the closing-edge safety sensor.\n");
    out.push_str("  2. Advances Closed → Opening → Open → Closing → Closed.\n");
    out.push_str("  3. Aims both panels at fully open or fully closed.\n");
    out.push_str("  4. Integrates a trapezoidal profile: accelerate, cruise, brake.\n");
    out.push_str("  5. Latches the pose when a stop is reached at rest.\n\n");
    out.push_str("A closing door reopens when either sensor trips.\n");
    out.push_str("An opening stroke always finishes, so the panels do not pinch.\n\n");
    out.push_str(&format!(
        "Travel {:0.2} m    max speed {:0.2} m/s    accel {:0.2} m/s²    dwell {:0.2} s\n\n",
        config.travel, config.max_speed, config.acceleration, config.dwell
    ));
    out.push_str("Script: a visitor arrives, leaves, returns mid-close, then the\n");
    out.push_str("safety edge trips on the next close.\n\n");

    let mut next_trace = 0.0;
    for sample in samples {
        let on_grid = sample.time + 1e-9 >= next_trace;
        if on_grid || sample.frame.is_some() {
            out.push_str(&format_sample(sample));
            out.push('\n');
            if on_grid {
                next_trace += 0.5;
            }
        }
        if let Some(frame) = &sample.frame {
            out.push('\n');
            out.push_str(frame);
            out.push('\n');
        }
    }
    out
}

fn format_sample(sample: &Sample) -> String {
    let sensors = match (sample.presence, sample.obstructed) {
        (true, true) => "presence+safety",
        (true, false) => "presence",
        (false, true) => "safety",
        (false, false) => "-",
    };
    format!(
        "{:6.2}  {:<16}  {:<8}  {:<13}  {:5.1}%  {:0.3} m  {:+5.2} m/s  {}",
        sample.time,
        sample.beat,
        sample.phase,
        sample.regime,
        sample.openness * 100.0,
        sample.position,
        sample.velocity,
        sensors
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phases(samples: &[Sample]) -> Vec<DoorPhase> {
        let mut out = Vec::new();
        for sample in samples {
            if out.last() != Some(&sample.phase) {
                out.push(sample.phase);
            }
        }
        out
    }

    #[test]
    fn script_opens_reopens_for_the_visitor_and_for_the_safety_edge() {
        let samples = simulate();
        assert_eq!(
            phases(&samples),
            vec![
                DoorPhase::Closed,
                DoorPhase::Opening,
                DoorPhase::Open,
                DoorPhase::Closing,
                DoorPhase::Opening,
                DoorPhase::Open,
                DoorPhase::Closing,
                DoorPhase::Opening,
                DoorPhase::Open,
                DoorPhase::Closing,
                DoorPhase::Closed,
            ]
        );

        let config = demo_config();
        for sample in &samples {
            assert!((0.0..=config.travel).contains(&sample.position));
            assert!(sample.velocity.abs() <= config.max_speed + 1e-6);
        }
        let last = samples.last().unwrap();
        assert_eq!(last.phase, DoorPhase::Closed);
        assert!(!last.presence);
        assert!(!last.obstructed);
        assert_eq!(last.beat, "done");

        let text = format_demo(&samples);
        assert!(text.contains("trapezoidal profile"));
        assert!(text.contains("visitor returns"));
        assert!(text.contains("safety edge"));
        assert!(text.contains("Closed"));
    }
}
