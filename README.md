# Ring World - Voxel Game

A Minecraft-inspired voxel game built on a **ring world** (Niven Ring / Halo-style) using Rust and wgpu.

## Concept

The player exists on the **inner surface** of a massive ring structure with a sun at its center. The world appears flat due to the radial nature of the geometry, but it's actually a complete ring:

- **Going forward** (along the ring circumference) will eventually bring you back to where you started
- **Going sideways** (along the ring width) has **edges** - you can fall off!
- **Looking up** you see the sun at the center, and the opposite side of the ring curving overhead
- **Gravity** points radially outward (away from the sun, toward the ring surface)

## Architecture

```
┌─────────────────────────────────────────┐
│              SUN (center)                │
│                  ☀                       │
│         ╱─────────────────╲             │
│        ╱   inner surface   ╲            │
│       │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │          │
│       │  ▓ PLAYER HERE  ▓  │           │
│       │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │          │
│        ╲                   ╱            │
│         ╲─────────────────╱             │
│              (ring)                      │
└─────────────────────────────────────────┘
```

## Ring World Coordinates

- **theta** (θ): Angle around the ring [0, 2π) - wraps around
- **y**: Axial position along ring width [-W/2, W/2] - has edges
- **height**: Distance above surface toward sun [0, max_height]

## Project Structure

```
src/
├── main.rs          # Entry point, window & event loop
├── ring_world.rs    # Ring geometry, coordinate system, chunk mapping
├── voxel.rs         # Voxel types and properties
├── chunk.rs         # Chunk storage, mesh generation, chunk manager
├── renderer.rs      # wgpu rendering pipeline
├── camera.rs        # Camera, projection, FPS controller
├── terrain.rs       # Procedural terrain generation with noise
├── sun.rs           # Sun lighting at ring center
├── input.rs         # Input state management
├── player.rs        # Player state and physics
└── shader.wgsl      # WGSL vertex/fragment shaders
```

## Controls

| Key | Action |
|-----|--------|
| W/S | Move forward/backward (along ring) |
| A/D | Strafe left/right (along width) |
| Space | Jump / Move up |
| Shift | Move down |
| Mouse | Look around |
| Escape | Quit |

## Building & Running

### Prerequisites

1. Install Rust: https://rustup.rs
2. Ensure you have a GPU with Vulkan, DX12, or Metal support

### Build & Run

```bash
cargo run --release
```

### Debug build (faster compile, slower runtime)

```bash
cargo run
```

## Technical Details

### Ring Topology
- Chunks are indexed by (ring_index, width_index, height_index)
- ring_index wraps around modulo `chunks_circumference`
- width_index is bounded [0, chunks_width) - edges exist here
- Each chunk is transformed to its correct position on the ring using rotation matrices

### Rendering
- Uses wgpu for cross-platform GPU rendering
- Per-chunk mesh generation with greedy face culling
- Sun lighting from the center of the ring
- Distance fog for atmosphere
- Chunks loaded/unloaded based on player proximity

### Terrain Generation
- Multi-octave Perlin noise for terrain shape
- Cave generation using 3D noise
- Biome-like variation (grass, sand near water, stone underground)
- Bedrock layer at the ring surface

## Future Enhancements

- [ ] Block placement/destruction
- [ ] Physics and collision detection
- [ ] Day/night cycle (ring rotation or sun dimming)
- [ ] Multiplayer
- [ ] Biome system
- [ ] Trees and vegetation
- [ ] Water physics
- [ ] View the opposite side of the ring in the sky
- [ ] Ring edge walls/barriers


## Store
while on a scientific mission to explore an unusual star. your spaceship