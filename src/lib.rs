//! Automatic sliding-door controller.
//!
//! Each tick the algorithm reads two sensors, updates a four-phase state
//! machine, and moves both panels with a trapezoidal velocity profile.
//! The interactive demo in the binary draws that motion in the terminal.

mod door;

pub use door::{ConfigError, DoorConfig, DoorPhase, MotionRegime, SlidingDoor};
