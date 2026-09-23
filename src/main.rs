//! Interactive sliding-door demo.
//!
//! `cargo run` animates the controller and reads live sensors from the keyboard.
//! `cargo run -- --demo` plays a scripted visitor through the same algorithm.

mod scenario;
mod term;
mod view;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use scenario::{demo_config, format_demo, simulate};
use sliding_door::SlidingDoor;
use view::{render, RenderOptions};

fn main() {
    if let Err(err) = run() {
        eprintln!("sliding-door: {err}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {
            if io::stdin().is_terminal() && io::stdout().is_terminal() {
                run_interactive()
            } else {
                eprintln!("(no terminal detected; running the scripted demo)");
                print_demo()
            }
        }
        Some("--demo") => {
            if args.next().is_some() {
                return unexpected();
            }
            print_demo()
        }
        Some("--help") | Some("-h") => {
            if args.next().is_some() {
                return unexpected();
            }
            print_help();
            Ok(())
        }
        Some(_) => unexpected(),
    }
}

fn unexpected() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: sliding-door [--demo | --help]",
    ))
}

fn print_help() {
    println!(
        "\
sliding-door

Automatic double-panel sliding door. Each frame runs a sensor state machine
and a trapezoidal velocity profile, then draws the panels.

Usage:
  cargo run                 Interactive demo
  cargo run -- --demo       Scripted visitor, printed as a motion trace
  cargo run -- --help

Keys (interactive):
  p   toggle the presence sensor
  b   toggle the safety edge
  r   reset the door
  q   quit"
    );
}

fn print_demo() -> io::Result<()> {
    let samples = simulate();
    let mut out = io::stdout();
    write!(out, "{}", format_demo(&samples))?;
    out.flush()
}

fn run_interactive() -> io::Result<()> {
    let config = demo_config();
    let mut door = SlidingDoor::new(config).expect("demo configuration is valid");
    let _raw = term::RawMode::acquire()?;
    let mut elapsed = 0.0;
    let mut last = Instant::now();
    let frame = Duration::from_millis(33);

    while !term::interrupted() {
        let frame_start = Instant::now();
        let dt = frame_start
            .saturating_duration_since(last)
            .as_secs_f64()
            .clamp(0.0, 0.05);
        last = frame_start;
        if dt > 0.0 {
            door.step(dt);
            elapsed += dt;
        }

        let picture = render(
            &door,
            &RenderOptions {
                color: true,
                show_keys: true,
                elapsed_secs: elapsed,
            },
        );
        let mut out = io::stdout();
        write!(out, "\x1b[H{picture}")?;
        out.flush()?;

        let timeout = frame.saturating_sub(frame_start.elapsed());
        for key in term::poll_keys(timeout.as_millis() as i32)? {
            match key {
                b'p' | b'P' => door.set_presence(!door.presence()),
                b'b' | b'B' => door.set_obstructed(!door.obstructed()),
                b'r' | b'R' => {
                    door = SlidingDoor::new(config).expect("demo configuration is valid");
                    elapsed = 0.0;
                }
                b'q' | b'Q' | 3 | 27 => return Ok(()),
                _ => {}
            }
        }
    }
    Ok(())
}
