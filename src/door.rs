//! Sliding-door motion algorithm.
//!
//! The doorway has two panels on one track. They mirror each other: the left
//! panel translates by `-position` and the right panel by `+position`, where
//! `position` is `0` when the doorway is shut and `travel` when it is fully
//! open.
//!
//! Every call to [`SlidingDoor::step`] does four things:
//!
//! 1. Sample the presence sensor and the closing-edge safety sensor.
//! 2. Advance the phase: Closed → Opening → Open → Closing → Closed.
//! 3. Choose a target pose (fully open or fully closed) and integrate a
//!    trapezoidal velocity profile toward it.
//! 4. Latch Open or Closed once a stop is reached at rest.
//!
//! Either sensor opens a shut door and reopens a closing stroke. If the panels
//! are still moving shut, the profile brakes through zero before opening.
//! An opening stroke always finishes, so the panels cannot pinch.

use std::fmt;

/// Kinematic and timing limits for one doorway.
///
/// Distances are metres, speeds are metres per second, and times are seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorConfig {
    /// Distance one panel travels from shut to fully open.
    pub travel: f64,
    /// Speed cap used while cruising.
    pub max_speed: f64,
    /// Magnitude of acceleration while ramping up or braking.
    pub acceleration: f64,
    /// How long the doorway stays open after both sensors go clear.
    pub dwell: f64,
    /// Distance and speed tolerance for "this stop has been reached".
    pub epsilon: f64,
}

impl Default for DoorConfig {
    fn default() -> Self {
        Self {
            // A typical pedestrian leaf: about half of a 2 m doorway.
            travel: 1.0,
            // Common automatic-door speed limit.
            max_speed: 0.5,
            acceleration: 0.8,
            dwell: 2.0,
            epsilon: 1e-3,
        }
    }
}

impl DoorConfig {
    /// Reject NaN, infinite, and non-physical limits.
    pub fn validate(&self) -> Result<(), ConfigError> {
        require_finite("travel", self.travel)?;
        require_finite("max_speed", self.max_speed)?;
        require_finite("acceleration", self.acceleration)?;
        require_finite("dwell", self.dwell)?;
        require_finite("epsilon", self.epsilon)?;

        if self.travel <= 0.0 {
            return Err(ConfigError::NonPositive("travel"));
        }
        if self.max_speed <= 0.0 {
            return Err(ConfigError::NonPositive("max_speed"));
        }
        if self.acceleration <= 0.0 {
            return Err(ConfigError::NonPositive("acceleration"));
        }
        if self.epsilon <= 0.0 {
            return Err(ConfigError::NonPositive("epsilon"));
        }
        if self.dwell < 0.0 {
            return Err(ConfigError::Negative("dwell"));
        }
        if self.epsilon >= self.travel {
            return Err(ConfigError::OutOfRange(
                "epsilon must be smaller than travel",
            ));
        }
        Ok(())
    }
}

fn require_finite(name: &'static str, value: f64) -> Result<(), ConfigError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ConfigError::NonFinite(name))
    }
}

/// Why [`DoorConfig::validate`] rejected a configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The named field was NaN or infinite.
    NonFinite(&'static str),
    /// The named field must be greater than zero.
    NonPositive(&'static str),
    /// The named field must be zero or positive.
    Negative(&'static str),
    /// A relationship between fields was violated.
    OutOfRange(&'static str),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NonFinite(field) => write!(f, "{field} must be finite"),
            ConfigError::NonPositive(field) => write!(f, "{field} must be greater than zero"),
            ConfigError::Negative(field) => write!(f, "{field} must not be negative"),
            ConfigError::OutOfRange(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Phase of one open/close cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorPhase {
    /// Panels are shut and at rest.
    Closed,
    /// Panels are committed to the open stop.
    Opening,
    /// Panels are fully open. The dwell timer runs while both sensors are clear.
    Open,
    /// Panels are returning to the closed stop, unless a sensor reopens them.
    Closing,
}

impl fmt::Display for DoorPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            DoorPhase::Closed => "Closed",
            DoorPhase::Opening => "Opening",
            DoorPhase::Open => "Open",
            DoorPhase::Closing => "Closing",
        })
    }
}

/// Which piece of the trapezoidal profile the panels are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionRegime {
    /// At the target pose with no speed.
    Idle,
    /// Speeding up toward the cruise limit.
    Accelerating,
    /// Holding the cruise speed.
    Cruising,
    /// Slowing down so the stop is reached at rest.
    Braking,
    /// Speed still opposes the new target, so the profile is braking through zero.
    Reversing,
}

impl fmt::Display for MotionRegime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            MotionRegime::Idle => "Idle",
            MotionRegime::Accelerating => "Accelerating",
            MotionRegime::Cruising => "Cruising",
            MotionRegime::Braking => "Braking",
            MotionRegime::Reversing => "Reversing",
        })
    }
}

/// Runtime state of one automatic sliding door.
///
/// Sensor inputs are latched with [`SlidingDoor::set_presence`] and
/// [`SlidingDoor::set_obstructed`]. Call [`SlidingDoor::step`] with the frame
/// time to advance the algorithm.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingDoor {
    config: DoorConfig,
    phase: DoorPhase,
    /// Metres from the closed stop. `0` is shut, `travel` is fully open.
    position: f64,
    /// Metres per second. Positive speed opens the doorway.
    velocity: f64,
    dwell_elapsed: f64,
    presence: bool,
    obstructed: bool,
}

impl Default for SlidingDoor {
    fn default() -> Self {
        Self::new(DoorConfig::default()).expect("the default door configuration is valid")
    }
}

impl SlidingDoor {
    /// Start shut, at rest, with both sensors clear.
    pub fn new(config: DoorConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            phase: DoorPhase::Closed,
            position: 0.0,
            velocity: 0.0,
            dwell_elapsed: 0.0,
            presence: false,
            obstructed: false,
        })
    }

    /// Someone is standing in the activation zone.
    pub fn set_presence(&mut self, presence: bool) {
        self.presence = presence;
    }

    /// The closing-edge safety sensor is pressed.
    ///
    /// A trip opens a shut door, reverses a closing stroke, and holds an open
    /// door. The opening stroke still finishes if the edge clears partway.
    pub fn set_obstructed(&mut self, obstructed: bool) {
        self.obstructed = obstructed;
    }

    pub fn presence(&self) -> bool {
        self.presence
    }

    pub fn obstructed(&self) -> bool {
        self.obstructed
    }

    pub fn config(&self) -> &DoorConfig {
        &self.config
    }

    pub fn phase(&self) -> DoorPhase {
        self.phase
    }

    /// Metres from the closed stop toward fully open.
    pub fn position(&self) -> f64 {
        self.position
    }

    /// Opening speed in metres per second. Negative while the panels are closing.
    pub fn velocity(&self) -> f64 {
        self.velocity
    }

    /// `0` when shut and `1` when fully open.
    pub fn openness(&self) -> f64 {
        (self.position / self.config.travel).clamp(0.0, 1.0)
    }

    /// Translation to apply to the left panel's closed pose.
    ///
    /// The value is `0` when shut and `-travel` when open.
    pub fn left_panel_displacement(&self) -> f64 {
        -self.position
    }

    /// Translation to apply to the right panel's closed pose.
    ///
    /// The value is `0` when shut and `+travel` when open.
    pub fn right_panel_displacement(&self) -> f64 {
        self.position
    }

    /// Pose the profile is currently steering toward.
    pub fn target_position(&self) -> f64 {
        match self.phase {
            DoorPhase::Opening | DoorPhase::Open => self.config.travel,
            DoorPhase::Closed | DoorPhase::Closing => 0.0,
        }
    }

    /// Seconds already counted toward the open dwell. Zero outside the open phase.
    pub fn dwell_elapsed(&self) -> f64 {
        self.dwell_elapsed
    }

    /// Seconds left before a clear, fully open door starts closing.
    pub fn dwell_remaining(&self) -> Option<f64> {
        if self.phase == DoorPhase::Open {
            Some((self.config.dwell - self.dwell_elapsed).max(0.0))
        } else {
            None
        }
    }

    /// Classify the current sample of the trapezoidal profile.
    pub fn regime(&self) -> MotionRegime {
        let error = self.target_position() - self.position;
        let speed = self.velocity.abs();
        if speed <= self.config.epsilon && error.abs() <= self.config.epsilon {
            return MotionRegime::Idle;
        }
        if self.velocity * error < 0.0 && speed > self.config.epsilon {
            return MotionRegime::Reversing;
        }
        let stop_distance = self.velocity * self.velocity / (2.0 * self.config.acceleration);
        if error.abs() <= stop_distance + self.config.epsilon {
            return MotionRegime::Braking;
        }
        if speed >= self.config.max_speed - self.config.epsilon {
            return MotionRegime::Cruising;
        }
        MotionRegime::Accelerating
    }

    /// Advance the controller by `dt` seconds.
    ///
    /// Non-finite and non-positive timesteps are ignored. Long timesteps are
    /// split internally so the discrete profile stays close to the continuous one.
    ///
    /// # Example
    ///
    /// ```
    /// use sliding_door::SlidingDoor;
    ///
    /// let mut door = SlidingDoor::default();
    /// door.set_presence(true);
    /// for _ in 0..300 {
    ///     door.step(1.0 / 60.0);
    /// }
    /// assert!(door.openness() > 0.9);
    /// ```
    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        // A 120 Hz substep keeps braking distance stable for interactive frame times
        // and for a caller that hands in a much larger tick.
        const MAX_SUBSTEP: f64 = 1.0 / 120.0;
        let mut remaining = dt;
        while remaining > 0.0 {
            let substep = remaining.min(MAX_SUBSTEP);
            self.step_once(substep);
            remaining -= substep;
        }
    }

    fn step_once(&mut self, dt: f64) {
        match self.phase {
            DoorPhase::Closed => {
                if self.presence || self.obstructed {
                    self.phase = DoorPhase::Opening;
                }
            }
            DoorPhase::Opening => {
                // The stroke is committed: clearing the sensor does not reverse it.
            }
            DoorPhase::Open => {
                if self.presence || self.obstructed {
                    self.dwell_elapsed = 0.0;
                } else {
                    self.dwell_elapsed += dt;
                    if self.dwell_elapsed >= self.config.dwell {
                        self.phase = DoorPhase::Closing;
                        self.dwell_elapsed = 0.0;
                    }
                }
            }
            DoorPhase::Closing => {
                if self.presence || self.obstructed {
                    self.phase = DoorPhase::Opening;
                }
            }
        }

        let (position, velocity) = integrate_axis(
            self.position,
            self.velocity,
            self.target_position(),
            self.config.max_speed,
            self.config.acceleration,
            dt,
            self.config.epsilon,
        );
        self.position = position;
        self.velocity = velocity;
        self.clamp_to_track();
        self.settle();
    }

    fn clamp_to_track(&mut self) {
        if self.position >= self.config.travel {
            self.position = self.config.travel;
            if self.velocity > 0.0 {
                self.velocity = 0.0;
            }
        } else if self.position <= 0.0 {
            self.position = 0.0;
            if self.velocity < 0.0 {
                self.velocity = 0.0;
            }
        }
    }

    fn settle(&mut self) {
        let epsilon = self.config.epsilon;
        let at_open =
            self.config.travel - self.position <= epsilon && self.velocity.abs() <= epsilon;
        let at_closed = self.position <= epsilon && self.velocity.abs() <= epsilon;

        if self.phase == DoorPhase::Opening && at_open {
            self.position = self.config.travel;
            self.velocity = 0.0;
            self.phase = DoorPhase::Open;
            self.dwell_elapsed = 0.0;
        } else if self.phase == DoorPhase::Closing && at_closed {
            self.position = 0.0;
            self.velocity = 0.0;
            self.phase = DoorPhase::Closed;
            self.dwell_elapsed = 0.0;
        }
    }
}

/// One sample of a 1D trapezoidal move toward `target`.
///
/// The command is one of:
/// - accelerate toward the target, until `max_speed`
/// - cruise at `max_speed` while the remaining distance is longer than the stop
/// - brake at `acceleration` once the remaining distance fits the stop
/// - brake through zero first when the current velocity points the wrong way
fn integrate_axis(
    position: f64,
    velocity: f64,
    target: f64,
    max_speed: f64,
    acceleration: f64,
    dt: f64,
    epsilon: f64,
) -> (f64, f64) {
    let error = target - position;
    if error.abs() <= epsilon {
        return (target, 0.0);
    }

    let direction = if error > 0.0 { 1.0 } else { -1.0 };
    let stop_distance = velocity * velocity / (2.0 * acceleration);
    let accel_cmd = if velocity * direction < 0.0 {
        direction * acceleration
    } else if error.abs() <= stop_distance {
        -direction * acceleration
    } else if velocity.abs() < max_speed {
        direction * acceleration
    } else {
        0.0
    };

    let velocity = (velocity + accel_cmd * dt).clamp(-max_speed, max_speed);
    let position = position + velocity * dt;
    let new_error = target - position;
    let crossed = (error > 0.0) != (new_error > 0.0);
    if crossed || new_error.abs() <= epsilon {
        (target, 0.0)
    } else {
        (position, velocity)
    }
}

/// Ideal duration of a rest-to-rest trapezoidal (or triangular) move.
#[cfg(test)]
fn analytic_stroke_time(distance: f64, max_speed: f64, acceleration: f64) -> f64 {
    let ramp_distance = max_speed * max_speed / (2.0 * acceleration);
    if distance <= 2.0 * ramp_distance {
        2.0 * (acceleration * distance).sqrt() / acceleration
    } else {
        let cruise = distance - 2.0 * ramp_distance;
        2.0 * max_speed / acceleration + cruise / max_speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_config() -> DoorConfig {
        DoorConfig {
            travel: 1.0,
            max_speed: 2.0,
            acceleration: 8.0,
            dwell: 0.5,
            epsilon: 1e-3,
        }
    }

    fn run_until(
        door: &mut SlidingDoor,
        mut pred: impl FnMut(&SlidingDoor) -> bool,
        seconds: f64,
    ) -> bool {
        let mut elapsed = 0.0;
        while elapsed < seconds {
            door.step(0.01);
            elapsed += 0.01;
            if pred(door) {
                return true;
            }
        }
        false
    }

    #[test]
    fn default_door_starts_shut() {
        let door = SlidingDoor::default();
        assert_eq!(door.phase(), DoorPhase::Closed);
        assert_eq!(door.position(), 0.0);
        assert_eq!(door.velocity(), 0.0);
        assert_eq!(door.openness(), 0.0);
        assert_eq!(door.regime(), MotionRegime::Idle);
        assert_eq!(door.left_panel_displacement(), 0.0);
        assert_eq!(door.right_panel_displacement(), 0.0);
    }

    #[test]
    fn invalid_configs_are_rejected() {
        let base = DoorConfig::default();
        assert!(matches!(
            SlidingDoor::new(DoorConfig {
                travel: 0.0,
                ..base
            }),
            Err(ConfigError::NonPositive("travel"))
        ));
        assert!(matches!(
            SlidingDoor::new(DoorConfig {
                max_speed: f64::NAN,
                ..base
            }),
            Err(ConfigError::NonFinite("max_speed"))
        ));
        assert!(matches!(
            SlidingDoor::new(DoorConfig {
                acceleration: f64::INFINITY,
                ..base
            }),
            Err(ConfigError::NonFinite("acceleration"))
        ));
        assert!(matches!(
            SlidingDoor::new(DoorConfig {
                dwell: -0.1,
                ..base
            }),
            Err(ConfigError::Negative("dwell"))
        ));
        assert!(matches!(
            SlidingDoor::new(DoorConfig {
                epsilon: base.travel,
                ..base
            }),
            Err(ConfigError::OutOfRange(_))
        ));
    }

    #[test]
    fn non_positive_timesteps_do_not_move_the_door() {
        let mut door = SlidingDoor::default();
        door.set_presence(true);
        door.step(f64::NAN);
        door.step(f64::INFINITY);
        door.step(-0.5);
        door.step(0.0);
        assert_eq!(door.phase(), DoorPhase::Closed);
        assert_eq!(door.position(), 0.0);
    }

    #[test]
    fn safety_edge_opens_a_shut_door_and_holds_it() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_obstructed(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        door.step(1.0);
        assert_eq!(door.phase(), DoorPhase::Open);
        assert!(door.openness() > 0.999);
    }

    #[test]
    fn presence_opens_the_door_and_holds_it() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_presence(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        assert!(door.openness() > 0.999);
        assert_eq!(door.left_panel_displacement(), -door.config().travel);
        assert_eq!(door.right_panel_displacement(), door.config().travel);

        door.step(2.0);
        assert_eq!(door.phase(), DoorPhase::Open);
        assert_eq!(door.dwell_remaining(), Some(door.config().dwell));
    }

    #[test]
    fn panels_stay_symmetric_through_a_cycle() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_presence(true);
        for _ in 0..400 {
            door.step(0.01);
            let sum = door.left_panel_displacement() + door.right_panel_displacement();
            assert!(sum.abs() < 1e-9);
            assert!((0.0..=door.config().travel).contains(&door.position()));
            assert!(door.velocity().abs() <= door.config().max_speed + 1e-9);
            assert!((0.0..=1.0).contains(&door.openness()));
        }
    }

    #[test]
    fn full_stroke_accelerates_cruises_and_brakes() {
        let mut door = SlidingDoor::default();
        door.set_presence(true);
        let mut saw_accel = false;
        let mut saw_cruise = false;
        let mut saw_brake = false;
        let mut previous = 0.0;
        let mut elapsed = 0.0;
        loop {
            door.step(1.0 / 120.0);
            elapsed += 1.0 / 120.0;
            if door.phase() == DoorPhase::Open {
                break;
            }
            assert!(
                door.position() + 1e-9 >= previous,
                "opening motion moved backward"
            );
            previous = door.position();
            assert!(door.velocity() >= -1e-9);
            match door.regime() {
                MotionRegime::Accelerating => saw_accel = true,
                MotionRegime::Cruising => saw_cruise = true,
                MotionRegime::Braking => saw_brake = true,
                MotionRegime::Idle | MotionRegime::Reversing => {}
            }
            assert!(elapsed < 8.0, "door did not finish opening");
        }
        assert!(saw_accel, "missing acceleration");
        assert!(saw_cruise, "missing cruise");
        assert!(saw_brake, "missing brake");

        let ideal = analytic_stroke_time(
            door.config().travel,
            door.config().max_speed,
            door.config().acceleration,
        );
        let error = (elapsed - ideal).abs() / ideal;
        assert!(error < 0.15, "stroke took {elapsed:.3}s, ideal {ideal:.3}s");
    }

    #[test]
    fn a_short_leaf_uses_a_triangular_profile() {
        let config = DoorConfig {
            travel: 0.15,
            max_speed: 1.0,
            acceleration: 1.0,
            dwell: 1.0,
            epsilon: 1e-4,
        };
        let mut door = SlidingDoor::new(config).unwrap();
        door.set_presence(true);
        let mut peak = 0.0_f64;
        assert!(run_until(
            &mut door,
            |door| {
                peak = peak.max(door.velocity());
                door.phase() == DoorPhase::Open
            },
            3.0
        ));
        let ideal_peak = (config.acceleration * config.travel).sqrt();
        assert!(peak < config.max_speed * 0.7, "peak {peak} reached cruise");
        assert!(
            (peak - ideal_peak).abs() < 0.08,
            "peak {peak}, ideal {ideal_peak}"
        );
    }

    #[test]
    fn dwell_must_finish_before_the_door_closes() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_presence(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        door.set_presence(false);

        let half = door.config().dwell * 0.5;
        door.step(half);
        assert_eq!(door.phase(), DoorPhase::Open);
        let remaining = door.dwell_remaining().unwrap();
        assert!((remaining - (door.config().dwell - half)).abs() < 0.02);

        door.set_presence(true);
        door.step(0.05);
        assert_eq!(door.dwell_elapsed(), 0.0);

        door.set_presence(false);
        door.step(door.config().dwell * 0.5);
        assert_eq!(door.phase(), DoorPhase::Open);
        door.step(door.config().dwell);
        assert_eq!(door.phase(), DoorPhase::Closing);
    }

    #[test]
    fn returning_presence_reopens_before_the_door_shuts() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_presence(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        door.set_presence(false);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Closing && door.position() < 0.6,
            3.0
        ));

        door.set_presence(true);
        let mut lowest = door.position();
        let mut saw_reverse = false;
        assert!(run_until(
            &mut door,
            |door| {
                lowest = lowest.min(door.position());
                if door.regime() == MotionRegime::Reversing {
                    saw_reverse = true;
                }
                door.phase() == DoorPhase::Open
            },
            3.0
        ));
        assert!(saw_reverse);
        assert!(lowest > 0.25, "door fell to {lowest}");
    }

    #[test]
    fn safety_edge_reverses_a_closing_door_and_holds_it_open() {
        let mut door = SlidingDoor::new(quick_config()).unwrap();
        door.set_presence(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        door.set_presence(false);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Closing && door.position() < 0.7,
            3.0
        ));

        door.set_obstructed(true);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Open,
            3.0
        ));
        door.step(1.5);
        assert_eq!(door.phase(), DoorPhase::Open);

        door.set_obstructed(false);
        assert!(run_until(
            &mut door,
            |door| door.phase() == DoorPhase::Closed,
            4.0
        ));
        assert_eq!(door.position(), 0.0);
        assert_eq!(door.velocity(), 0.0);
    }

    #[test]
    fn sensor_chatter_stays_inside_the_track() {
        let config = quick_config();
        let mut door = SlidingDoor::new(config).unwrap();
        let pattern = [true, true, false, false, false, true, false];
        for i in 0..500 {
            door.set_presence(pattern[i % pattern.len()]);
            door.set_obstructed(i % 17 == 0);
            door.step(0.016);
            assert!((0.0..=config.travel).contains(&door.position()));
            assert!(door.velocity().abs() <= config.max_speed + 1e-9);
            match door.phase() {
                DoorPhase::Closed => assert!(door.position() <= config.epsilon),
                DoorPhase::Open => assert!(config.travel - door.position() <= config.epsilon),
                DoorPhase::Opening | DoorPhase::Closing => {}
            }
        }
    }
}
