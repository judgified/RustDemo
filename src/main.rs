//! Interactive sliding-door demo.
//!
//! `cargo run` animates the controller and reads live sensors from the keyboard.
//! `cargo run -- --demo` plays a scripted visitor through the same algorithm.

mod scenario;
mod term;
mod view;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use scenario::{demo_config, format_demo, simulate, Playback};
use sliding_door::{MotionRegime, SlidingDoor};
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
        Some("--play") => {
            if args.next().is_some() {
                return unexpected();
            }
            if io::stdin().is_terminal() && io::stdout().is_terminal() {
                run_playback()
            } else {
                eprintln!("(no terminal detected; printing the scripted trace)");
                print_demo()
            }
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
        "usage: sliding-door [--play | --demo | --help]",
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
  cargo run -- --play       Animated visitor story, including the safety reverse
  cargo run -- --demo       That story printed as a motion trace
  cargo run -- --help

Keys (interactive):
  p   toggle the presence sensor
  b   toggle the safety edge
  r   reset the door
  q   quit"
    );
}

/// Append an erase-to-end-of-line on every row so a shorter status line does
/// not leave characters from the previous frame.
fn clear_lines(picture: &str) -> String {
    let mut out = String::with_capacity(picture.len() + 16);
    for line in picture.split_inclusive('\n') {
        if let Some(body) = line.strip_suffix('\n') {
            out.push_str(body);
            out.push_str("\x1b[K\n");
        } else {
            out.push_str(line);
            out.push_str("\x1b[K");
        }
    }
    out
}

fn print_demo() -> io::Result<()> {
    let samples = simulate();
    let mut out = io::stdout();
    write!(out, "{}", format_demo(&samples))?;
    out.flush()
}

fn run_playback() -> io::Result<()> {
    let _raw = term::RawMode::acquire()?;
    let mut playback = Playback::new();
    let mut last = Instant::now();
    let frame = Duration::from_millis(33);

    loop {
        let frame_start = Instant::now();
        let mut dt = frame_start
            .saturating_duration_since(last)
            .as_secs_f64()
            .clamp(0.0, 0.05);
        // Stretch the reversing stroke so the brake-and-reopen stays on screen.
        if playback.door().regime() == MotionRegime::Reversing {
            dt *= 0.12;
        }
        last = frame_start;
        if dt > 0.0 {
            playback.step(dt);
        }

        let mut picture = render(
            playback.door(),
            &RenderOptions {
                color: true,
                show_keys: false,
                elapsed_secs: playback.time(),
            },
        );
        picture.push_str(&format!("Script        {}\n", playback.beat()));
        picture.push_str("q  quit\n");
        let mut out = io::stdout();
        write!(out, "\x1b[H{}\x1b[J", clear_lines(&picture))?;
        out.flush()?;

        if playback.finished() {
            std::thread::sleep(Duration::from_secs(1));
            break;
        }

        let timeout = frame.saturating_sub(frame_start.elapsed());
        for key in term::poll_keys(timeout.as_millis() as i32)? {
            if matches!(key, b'q' | b'Q' | 3 | 27) || term::interrupted() {
                return Ok(());
            }
        }
    }
    Ok(())
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
        // Home the cursor, erase leftover glyphs from the previous frame, and
        // erase anything below the new picture.
        write!(out, "\x1b[H{}\x1b[J", clear_lines(&picture))?;
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

#[cfg(test)]
mod tests {
    use super::clear_lines;

    #[test]
    fn each_frame_row_erases_leftover_glyphs() {
        assert_eq!(
            clear_lines("Closed\nIdle\n"),
            "Closed\u{1b}[K\nIdle\u{1b}[K\n"
        );
        assert_eq!(clear_lines("Open"), "Open\u{1b}[K");
    }
}
