# Ring World — Controls

All bindings verified against `src/main.rs` and `src/camera.rs`.

## Movement

| Input | Action |
|-------|--------|
| W / Up Arrow | Move forward (along the ring) |
| S / Down Arrow | Move backward |
| A / Left Arrow | Strafe left |
| D / Right Arrow | Strafe right |
| Space | Jump (walking) / ascend (flying) |
| Left Shift (hold) | Crouch (walking, prevents walking off edges) / descend (flying) |
| Left Ctrl (hold) | Sprint |
| F | Toggle flying (creative mode) |
| Mouse | Look around |

## World Interaction

| Input | Action |
|-------|--------|
| Left Click (hold) | Break the targeted block (progress shown on the highlight box) |
| Right Click | Place the selected block on the targeted face |
| 1–9 | Select hotbar slot |

## Interface

| Input | Action |
|-------|--------|
| E | Toggle inventory |
| C | Toggle crafting (needs a crafting table within 3 blocks for full recipes) |
| Escape | Release the mouse cursor / regain it by clicking |
| F11 | Toggle borderless fullscreen |

## Game Modes & Time

| Input | Action |
|-------|--------|
| F1 | Toggle creative mode |
| F8 | Cycle day/night time scale: 1x → 20x → 120x (watch the shadow-square terminator sweep) |

## Debug & Diagnostics

| Input | Action |
|-------|--------|
| F3 | Debug overlay (FPS, position, chunk, biome, daylight factor, time scale, culling states) |
| F4 | Toggle frustum culling (A/B test) |
| F5 | Toggle neighbor occlusion culling (A/B test, default off) |
| F6 | Render-diagnostic mode: full-bright per-face normal tints, no fog, no culling |
| F7 | Toggle greedy meshing (A/B test: every face as its own quad) |

## Notes

- The day/night cycle is driven by shadow squares orbiting between the ring
  and the central sun (the sun itself never moves — it is always local noon
  unless a square is passing over you). One full day+night cycle is 10
  minutes at 1x.
- The ring wraps: walk forward long enough and you return to your start.
  Sideways (A/D) has edges — you can fall off.
