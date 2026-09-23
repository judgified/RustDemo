# Sliding door algorithm

Interactive Rust demo of an automatic double-panel sliding door.

The doorway is driven by a small controller, not by a canned animation. Each tick reads two sensors, updates a state machine, and moves both panels with a trapezoidal velocity profile. The terminal view is that motion drawn as two leaves sliding apart.

## Run

```bash
cargo run                 # interactive
cargo run -- --demo       # scripted visitor and a motion trace
cargo test
```

Interactive keys:

| Key | Action |
| --- | --- |
| `p` | Toggle the presence sensor |
| `b` | Toggle the closing-edge safety sensor |
| `r` | Reset the door |
| `q` | Quit |

Press `p` and the panels accelerate, cruise, and brake open. Press `p` again and, after the dwell, they close. Press `p` or `b` during the close and the stroke reverses.

## Algorithm

Each call to `SlidingDoor::step(dt)`:

1. Reads the presence sensor and the safety edge.
2. Advances the phase: **Closed → Opening → Open → Closing → Closed**.
3. Aims the panels at fully open or fully closed.
4. Integrates one sample of a trapezoidal profile (accelerate, cruise, brake).
5. Latches **Open** or **Closed** when a stop is reached at rest.

Rules:

- Presence while shut starts an opening stroke. That stroke always finishes, so the leaves do not reverse onto someone in the doorway.
- While the door is open, either sensor resets the dwell timer.
- When dwell expires with both sensors clear, the door closes.
- Presence or the safety edge during a close immediately retargets the profile to fully open. If the panels are still moving shut, the profile brakes through zero and then opens. That is the reversing regime.
- The safety edge does not open a door that is already shut. It only interrupts closing and holds the open dwell.

`position` is the shared leaf travel: `0` shut, `travel` fully open. The left panel's translation is `-position` and the right panel's is `+position`. Those two displacements are what a graphics scene would write into each leaf's model matrix:

```text
left_panel  = door_frame * translate(left_panel_displacement(),  0, 0)
right_panel = door_frame * translate(right_panel_displacement(), 0, 0)
```

## Layout

| File | Role |
| --- | --- |
| `src/door.rs` | State machine and trapezoidal profile |
| `src/view.rs` | Panel drawing |
| `src/scenario.rs` | Scripted arrive / return / safety-edge walk-up |
| `src/term.rs` | Single-keypress terminal mode |
| `src/main.rs` | Interactive loop and `--demo` |

The demo configuration is a 1 m leaf, 0.9 m/s cruise, 1.8 m/s² ramps, and a 1.4 s dwell. The library default is a slower pedestrian door: 0.5 m/s and 0.8 m/s².
