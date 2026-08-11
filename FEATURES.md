# Ring World — Full Roadmap & Feature Tracker

## 🎯 Vision Statement

A voxel survival/exploration game set on the inner surface of a Niven Ring — a massive ring-shaped megastructure with a sun at its center. The player lives on the inner surface, experiencing a world that appears flat locally but curves visibly overhead. Walking forward brings you back to where you started. The ring has edges you can reach. The sun never sets.

---

## 📝 Development Status

Versions **v0.1 through v0.4 are now fully implemented**. The project builds successfully (`cargo build`) and all **114 unit tests pass** (`cargo test`). Development focus has now moved to **v0.5**.

### Post-v0.4 Bug-Fix Pass

A round of bug fixes following v0.4 addressed rendering, spawning, and asset issues:

- **Debug overlay (F3)**: Real on-screen bitmap-font text showing FPS, player ring/cartesian position, chunk coord, biome, block being looked at, loaded/rendered chunk counts, entity count, health, mode, grounded/in-water, and culling-toggle states.
- **Culling toggles**: F4 toggles frustum culling, F5 toggles occlusion culling (occlusion now defaults OFF for correctness).
- **Terrain rendering fixed**: Greedy-mesh PosZ/NegZ face winding was inverted, causing +Z/−Z faces to be back-face culled (invisible) and lighting inverted; corrected winding in both the greedy and LOD mesh paths.
- **Occlusion culling corrected**: was culling visible chunks (only checked that neighbors were non-empty); now only culls when all 6 neighbors are fully opaque on the shared face, and is off by default.
- **Spawn/respawn safety**: player now deterministically spawns at theta=0,y=0 on top of the surface via a column scan + AABB clearance check; respawn uses the same safe placement so the player never starts inside/below terrain.
- **Decorative plants**: TallGrass, Flower, Mushroom, and Vine now render as double-sided cross/X billboards instead of full textured cubes.
- **PNG textures**: the texture atlas now loads the PNGs in the `textures/` folder (grass, dirt, stone, sand, water, leaves, snow, bedrock) and falls back to procedural textures for any missing file.

---


### Curved-Mesh Rendering Rewrite (2026-08-11)

Root-cause fix for the long-standing "seams / missing faces / falling through
terrain / spawning inside blocks" cluster:

- **Diagnosis**: collision, raycast, spawn, and placement all live on the exact
  ANGULAR voxel grid (theta uniform in angle), but chunks were RENDERED as flat
  linear boxes via a single `chunk_transform` matrix. A linear matrix cannot
  represent the polar mapping: rendered chunks were ~0.2 blocks too narrow at
  the surface (gaps at every ring-chunk boundary), stacked chunks used different
  tangent scales (misaligned columns), and the transform used a MIRRORED tangent
  `(sin, 0, -cos)` to keep a positive determinant, drawing every chunk reversed
  along the ring axis relative to the collision grid. Visuals and physics could
  never agree.
- **Fix**: `curved_local_to_world` / `curved_normal` (ring_world.rs) map every
  mesh vertex through the exact ring equation; boundary vertices are computed
  from global voxel indices so adjacent chunks produce bit-identical positions
  (seams close exactly). `curve_mesh_data` (chunk.rs) bakes this into all three
  mesh paths (greedy / non-greedy / LOD) and reverses triangle index order to
  compensate for the ring frame's reflection. Chunks now draw with an identity
  model matrix.
- **Greedy merge cap**: merged quads along the ring (theta) axis are capped at
  4 voxels (`RING_MERGE_CAP`) so flat chord quads never sag more than ~0.003
  blocks below the curved surface.
- **Back-face culling re-enabled** on the opaque pipeline (it had been disabled
  as a workaround for the mirrored-tangent winding flips).
- **Highlight/preview box** and frustum-culling chunk centers use the same
  curved mapping.
- Regression tests: boundary bit-exactness (ring wrap, height, width),
  curved-vs-collision-grid agreement, reflection pinning, mesh-level CCW-outward
  winding at ring indices all around the ring, greedy cap enforcement.


### Shadow Squares — Day/Night Cycle (2026-08-11)

Niven-canon day/night: the sun never moves (eternal noon); night is cast by a
chain of dark shadow squares orbiting between the sun and the ring.

- **`shadow_squares.rs`**: 6 panels orbiting at 0.4R, umbral night fraction
  0.35, full day+night cycle 600s (`DAY_CYCLE_SECS`), penumbra softness 0.03
  rad (~20-block terminator band). `daylight_at(theta)` is the CPU mirror of
  the shader formula (tests + F3 overlay).
- **Per-fragment eclipse lighting** (shader.wgsl, `SunUniform.eclipse`):
  occlusion = wrapped angular distance from the fragment's theta to the
  nearest square center. Diffuse/specular fully eclipsed; ambient floors at
  18% (the lit arch overhead keeps night moody, not void); fog sun-glow dims.
  The terminator visibly sweeps the landscape, and the FAR side of the arch
  stays lit during local night (it is shadowed at a different time).
- **Visible panels**: dark silhouettes drawn with the distant-ring pipeline
  (depth-write off) after the arch, before terrain.
- **F8** cycles the time scale 1x / 20x / 120x to watch the eclipse sweep.
- 6 unit tests (umbra/noon values, monotonic terminator, night fraction,
  phase wrap, buffer consistency).

## 📦 Release Milestones

### v0.1 — Foundation (Complete)
Core engine, basic rendering, proof of concept.

### v0.2 — Playable Alpha (Complete)
Solid gameplay loop: mine, build, explore. Textures, proper physics, inventory.

### v0.3 — World Richness (Complete)
Trees, structures, caves, varied biomes, wildlife.

### v0.4 — Polish & Performance (Complete)
Optimized rendering, sound, particles, UI.

### v0.5 — Multiplayer & Persistence (CURRENT)
Save/load, networking, shared worlds.

### v1.0 — Release
Complete game with progression, goals, and content.

---

## ✅ v0.1 — Foundation (Complete)

### Core Systems
- [x] Ring world coordinate system (theta, y, height)
- [x] Chunk system (16³ voxels, 256×16×4 grid)
- [x] wgpu rendering pipeline
- [x] Chunk transform with correct handedness
- [x] Ring circumference wrapping
- [x] Ring width edges

### Rendering
- [x] Per-face block coloring (Minecraft-style)
- [x] Sun lighting from center (diffuse + specular + ambient)
- [x] Distance fog
- [x] Distant ring LOD mesh
- [x] Depth buffer

### Terrain
- [x] Perlin noise terrain generation
- [x] 6 biomes (Ocean, Beach, Plains, Forest, Mountains, Desert)
- [x] Water generation (sea level)
- [x] Sand beaches, snow peaks

### Player
- [x] Ring-aware FPS camera
- [x] Gravity and jumping (1.2 block max)
- [x] AABB collision detection
- [x] Block raycasting (6 block range)
- [x] Left click destroy / Right click place
- [x] 2-block tall player, eye at 1.8

### Block System
- [x] 10 block types with properties
- [x] Hardness, tool type, solidity, transparency
- [x] Unbreakable blocks (Bedrock)

---

## 🔨 v0.2 — Playable Alpha

### Bug Fixes (Priority)
- [x] Fix stretched voxels (radius/width calculated for cubic voxels) — IN PROGRESS
- [x] Fix movement direction consistency when traversing ring
- [x] Fix chunk boundary seam lines (cross-chunk face culling)
- [x] Fix horizontal collision strength (prevent clipping)
- [x] Cap first-frame dt to prevent physics explosion
- [x] Prevent placing blocks inside player

### Textures & Visuals
- [x] Texture atlas system (load PNG textures, create GPU texture array)
- [x] UV coordinates in vertex format
- [x] Shader texture sampling
- [x] Grass texture (top face) — asset: `C:\Users\kryasatt\Orcha\generated_images\a_24x24_pixel_art_grass.png`
- [x] Dirt texture (side/bottom faces)
- [x] Stone texture
- [x] Sand texture
- [x] Water texture (animated UV scroll)
- [x] Wood texture (bark + rings)
- [x] Leaves texture (semi-transparent)
- [x] Snow texture
- [x] Bedrock texture
- [x] Skybox (dark space with stars + sun glow)

### Player & Controls
- [x] Mouse cursor lock/capture (grab cursor on click, release on Escape)
- [x] Crosshair HUD overlay (simple + rendered)
- [x] Block selection hotbar (1-9 keys)
- [x] Current block indicator in HUD
- [x] Sprint (double-tap W or hold Ctrl)
- [x] Crouch (Shift — prevents falling off edges)
- [x] Swimming animation/speed reduction in water
- [x] Fall damage

### Inventory & Items
- [x] Inventory data structure (36 slots + 9 hotbar)
- [x] Block drops when destroyed
- [x] Pick up items (walk over)
- [x] Inventory UI (press E to open)
- [x] Stack sizes (max 64)
- [x] Creative mode toggle (F1 — infinite blocks, fly)

### World Interaction
- [x] Block breaking progress (crack overlay)
- [x] Tool speed multipliers (pickaxe on stone, shovel on dirt)
- [x] Block placement preview (ghost block)
- [x] Reach distance indicator

---

## 🌲 v0.3 — World Richness

### Terrain Generation
- [x] Cave system (3D noise, connected caverns)
- [x] Ore generation (iron, gold, diamond at different depths)
- [x] Ravines and canyons
- [x] Rivers (flowing water between biomes)
- [x] Cliff faces and overhangs
- [x] Underwater terrain detail

### Vegetation
- [x] Tree generation (Oak — trunk + leaf canopy)
- [x] Birch trees (white bark, different shape)
- [x] Pine trees (tall, narrow, mountain biome)
- [x] Cactus (desert biome)
- [x] Tall grass (decorative, 1-block plants)
- [x] Flowers (multiple colors, decorative)
- [x] Mushrooms (cave/dark areas)
- [x] Vines (hang from trees/cliffs)

### Structures
- [x] Village generation (small houses, paths)
- [x] Ruins (ancient ring-builder structures)
- [x] Dungeons (underground rooms with loot)
- [x] Ring edge walls (massive walls at the width boundaries)
- [x] Sun tower (tall structure pointing at the sun)

### Entities & Mobs
- [x] Entity component system (ECS)
- [x] Passive mobs: Sheep, Cow, Pig, Chicken
- [x] Hostile mobs: Zombie, Skeleton, Spider
- [x] Mob AI (pathfinding, day/night behavior)
- [x] Mob spawning rules (light level, biome)
- [x] Mob drops (food, materials)
- [x] Health system (hearts)
- [x] Damage and knockback
- [x] Death and respawn

### Crafting
- [x] Crafting table block
- [x] Recipe system (shaped and shapeless)
- [x] Basic tools: Wood/Stone/Iron pickaxe, axe, shovel, sword
- [x] Furnace (smelting ores)
- [x] Chest (storage block)
- [x] Door, ladder, torch
- [x] Armor (helmet, chestplate, leggings, boots)

### Lighting
- [x] Block light propagation (torches, lava)
- [x] Sunlight from ring center (directional)
- [x] Light level affects mob spawning
- [x] Smooth lighting / ambient occlusion
- [x] Dynamic shadows (optional, performance toggle)

---

## ✨ v0.4 — Polish & Performance
### Bug tracking
- [x] terrain blocks don't render correclttly or are invisable? hard to tell — fixed: greedy-mesh PosZ/NegZ winding was inverted (faces back-face culled)
- [x] spawning below or inside terrain, spawn should be perdicable enough to never end up inside anything — fixed: deterministic safe column-scan + AABB clearance spawn/respawn
- [x] flowers appear as boxes with flowers stamped all sorts a ways around it — fixed: decorative plants now render as double-sided cross billboards
- [x] textures not untilized form texture folder — fixed: atlas now loads `textures/` PNGs with procedural fallback

### Rendering Optimization
- [x] Greedy meshing (merge adjacent same-type faces)
- [x] Frustum culling (don't render chunks behind camera)
- [x] Occlusion culling (don't render chunks behind other chunks)
- [x] LOD system (lower detail chunks at distance)
- [x] Multithreaded chunk generation (rayon)
- [x] Multithreaded mesh building
- [x] GPU instancing for repeated geometry
- [x] Chunk mesh caching (don't rebuild unchanged chunks)

### Visual Effects
- [x] Particle system (block break particles, torch sparks)
- [x] Water reflections (simple planar)
- [x] Underwater tint (blue overlay when submerged)
- [x] Block animation (water flow, leaves sway)
- [x] Weather (rain, snow — visual only initially)
- [x] Sun glow effect (bloom at center)
- [x] Ring shadow bands (opposite side of ring casts shadow)

### Audio
- [x] Audio engine integration (rodio or kira crate)
- [x] Block break/place sounds
- [x] Footstep sounds (varies by surface)
- [x] Ambient sounds (wind, water, birds)
- [x] Music (procedural or looping tracks)
- [x] UI sounds (inventory open/close, button clicks)

### UI & HUD
- [x] Health bar
- [x] Hunger bar
- [x] Experience bar
- [x] Minimap (optional, shows nearby terrain)
- [x] Debug overlay (F3 — FPS, position, chunk, biome)
- [x] Settings menu (render distance, FOV, controls)
- [x] Main menu (New World, Load World, Settings, Quit)
- [x] Pause menu

### Quality of Life
- [x] Auto-jump (step up 1-block ledges)
- [x] Smooth camera (interpolate between frames)
- [x] Key rebinding
- [x] Mouse sensitivity slider
- [x] FOV slider
- [x] Render distance slider
- [x] Fullscreen toggle (F11)

---

## 🌐 v0.5 — Multiplayer & Persistence
**NO MULITPLAYER WILL BE IMPLEMENTED AT THIS TIME**
### Save/Load
- [ ] World serialization format (chunks saved to disk)
- [ ] Player position/inventory persistence
- [ ] Chunk loading from disk (lazy load)
- [ ] World seed storage
- [ ] Multiple save slots

### Networking
- [ ] Client-server architecture
- [ ] Player synchronization (position, rotation)
- [ ] Chunk streaming (server sends chunks to clients)
- [ ] Block change synchronization
- [ ] Chat system
- [ ] Player list
- [ ] Server browser or direct connect

### Anti-grief
- [ ] Spawn protection
- [ ] Player permissions (build, break, admin)
- [ ] World backup system

---

## 🏆 v1.0 — Release

### Progression
- [ ] Achievement system
- [ ] Boss mobs (Ring Guardian?)
- [ ] End-game content (reach the sun? Build to the edge?)
- [ ] Technology tree (primitive → advanced)
- [ ] Story elements (who built the ring? Why?)

### Ring-Specific Features
- [ ] View opposite side of ring in detail (LOD rendering of far terrain)
- [ ] Ring edge exploration (what's beyond the edge?)
- [ ] Zero-gravity zone near the sun
- [ ] Ring rotation effects (Coriolis force?)
- [ ] Different ring sections (industrial, natural, ruined)
- [ ] Ring damage events (meteor impacts creating craters)

### Modding
- [ ] Lua scripting for custom blocks/items
- [ ] Custom texture pack support
- [ ] Plugin API for server-side mods
- [ ] World generation customization

---

## 🧪 Testing Strategy

### Unit Tests
- [x] Ring coordinate math (to_cartesian, from_cartesian, wrapping)
- [x] Chunk coordinate calculations (from_ring_position, neighbor)
- [x] Block properties lookup
- [x] Terrain height sampling (deterministic with seed)
- [x] Biome selection (deterministic with seed)
- [x] AABB collision math
- [x] Raycast hit detection

### Integration Tests
- [ ] Chunk generation produces valid terrain
- [ ] Chunk mesh generation produces valid vertices
- [ ] Player physics doesn't clip through terrain
- [ ] Block placement/destruction modifies correct voxel
- [ ] World wrapping (walk full circumference, return to start)
- [ ] Edge detection (can't walk past ring width)

### Performance Tests
- [ ] Chunk generation speed (target: <5ms per chunk)
- [ ] Mesh generation speed (target: <2ms per chunk)
- [ ] Frame time with 1000 visible chunks (target: <16ms)
- [ ] Memory usage per chunk (target: <8KB)
- [ ] Startup time (target: <3s)

### Visual Tests (Manual)
- [ ] Voxels appear cubic (not stretched)
- [ ] No gaps between chunks
- [ ] Terrain looks natural from ground level
- [ ] Ring is visible curving overhead
- [ ] Sun lighting looks correct
- [ ] Water renders with transparency
- [ ] Biome transitions are smooth

---

## 📊 Technical Parameters

| Parameter | Current | Target |
|-----------|---------|--------|
| Ring Radius | ~652 units | ~652 (cubic voxels) |
| Ring Width | 256 units | 256 |
| Max Height | 64 voxels | 128 (increase later) |
| Chunk Size | 16³ | 16³ |
| Chunks Around | 256 | 512 (larger world) |
| Chunks Wide | 16 | 32 (wider ring) |
| Chunks High | 4 | 8 (taller builds) |
| Render Distance | 8 chunks | 16 chunks |
| Sea Level | 25 | 25 |
| Player Height | 2.0 blocks | 2.0 |
| Eye Height | 1.8 from feet | 1.8 |
| Jump Height | 1.2 blocks | 1.25 |
| Gravity | 20 voxels/s² | 20 |
| Move Speed | 30 units/s | 4.3 blocks/s (MC speed) |
| Sprint Speed | — | 5.6 blocks/s |
| Raycast Range | 6 blocks | 5 blocks (MC default) |
| FOV | 70° | 70° (adjustable) |
| Target FPS | — | 60 |

---

## 🏗️ Architecture

```
src/
├── main.rs              — Entry point, event loop, input dispatch
├── renderer.rs          — wgpu pipeline, mesh management, render loop
├── ring_world.rs        — Ring geometry, coordinates, chunk transforms
├── chunk.rs             — Chunk storage, mesh generation, chunk manager
├── voxel.rs             — VoxelType enum, per-face colors
├── block.rs             — Block properties (hardness, tool, physics)
├── terrain.rs           — Biome system, terrain generation, noise
├── distant_ring.rs      — LOD ring mesh with terrain colors
├── camera.rs            — Ring-aware camera, projection, FPS controller
├── player.rs            — Player state, physics, collision, raycasting
├── sun.rs               — Sun at ring center, lighting uniforms
├── input.rs             — Input state management
├── shader.wgsl          — WGSL vertex/fragment shaders
│
├── (PLANNED)
├── inventory.rs         — Inventory, hotbar, item stacks
├── entity.rs            — ECS for mobs and items
├── crafting.rs          — Recipe system
├── ui.rs                — HUD, menus, overlays
├── audio.rs             — Sound effects and music
├── network.rs           — Multiplayer client/server
├── save.rs              — World serialization
└── texture.rs           — Texture atlas, UV mapping
```

---

## 🎨 Asset Pipeline

### Textures (24×24 pixel art)
| Block | Top | Side | Bottom | Status |
|-------|-----|------|--------|--------|
| Grass | grass_top.png | grass_side.png | dirt.png | 🟡 Top exists |
| Dirt | dirt.png | dirt.png | dirt.png | ⬜ Needed |
| Stone | stone.png | stone.png | stone.png | ⬜ Needed |
| Sand | sand.png | sand.png | sand.png | ⬜ Needed |
| Water | water.png (animated) | — | — | ⬜ Needed |
| Wood | wood_top.png | wood_side.png | wood_top.png | ⬜ Needed |
| Leaves | leaves.png | leaves.png | leaves.png | ⬜ Needed |
| Snow | snow.png | snow_side.png | dirt.png | ⬜ Needed |
| Bedrock | bedrock.png | bedrock.png | bedrock.png | ⬜ Needed |

### Sounds
| Event | File | Status |
|-------|------|--------|
| Block break (stone) | break_stone.ogg | ⬜ |
| Block break (dirt) | break_dirt.ogg | ⬜ |
| Block break (wood) | break_wood.ogg | ⬜ |
| Block place | place.ogg | ⬜ |
| Footstep (grass) | step_grass.ogg | ⬜ |
| Footstep (stone) | step_stone.ogg | ⬜ |
| Jump | jump.ogg | ⬜ |
| Splash (water) | splash.ogg | ⬜ |
| Ambient (wind) | ambient_wind.ogg | ⬜ |

---

## 🐛 Known Bugs (Current Build)

| ID | Severity | Description | Status |
|----|----------|-------------|--------|
| BUG-001 | Fixed | Camera-physics desync | ✅ |
| BUG-002 | Medium | Shift key conflicts with gravity | Fixed |
| BUG-003 | Medium | Chunk boundary face lines | Fixed |
| BUG-004 | Medium | Horizontal collision too weak | Fixed |
| BUG-005 | Low | Raycast normal approximate | Open |
| BUG-006 | High | No mouse cursor lock | Fixed |
| BUG-007 | Medium | Can place blocks inside player | Fixed |
| BUG-008 | Low | Water unbreakable | By design |
| BUG-009 | Low | Distant ring doesn't update | Open |
| BUG-010 | Medium | No crosshair/HUD | Fixed |
| BUG-011 | Low | Chunk boundary seams | Fixed |
| BUG-012 | Fixed | First frame huge dt | ✅ |
| BUG-013 | High | Voxels stretched (non-cubic) | Fixed |
| BUG-014 | Medium | Movement direction inconsistent | Fixed |
| BUG-015 | Low | No fall damage | Fixed |
| BUG-016 | High | v0.4 greedy-mesh regression: PosZ/NegZ face winding inverted (+Z/−Z faces back-face culled & lighting inverted); fixed in greedy and LOD mesh paths | Fixed |
| BUG-017 | High | Occlusion culling removed visible chunks (only checked non-empty neighbors); now requires all 6 neighbors fully opaque, off by default | Fixed |
| BUG-018 | Medium | Player spawned inside/below terrain; now uses deterministic safe column-scan + AABB clearance spawn/respawn | Fixed |
| BUG-019 | Medium | Decorative plants rendered as full cubes; now render as double-sided cross billboards | Fixed |
| BUG-020 | Medium | Texture atlas ignored `textures/` PNGs; now loads PNGs with procedural fallback | Fixed |
## 2026-08-11 Batch: Sky, Entities, Persistence, World Gen

| Feature | Commit | Status |
|---------|--------|--------|
| Water back-to-front sorting (Pass B draws farthest first, fixes blend order) | e2a4662 | ✅ |
| Sun disk + glow ring billboard at ring center, geometrically eclipsed by shadow squares (depth-write pipeline) | e2a4662 | ✅ |
| Starfield: 700 stars on camera-following celestial sphere, alpha fades with daylight (0.10 noon to 0.95 night) | e2a4662 | ✅ |
| Entity rendering: per-mob curved ring-frame boxes through the chunk shader (sun, eclipse, fog apply) | 716b8a8 | ✅ |
| Daylight-gated hostile spawns (block light < 7 or daylight < 0.3) | 716b8a8 | ✅ |
| Save/load persistence: versioned binary format, edit overlay + player state + shadow phase, autosave + save on close, no new deps | afc3a4e | ✅ |
| World gen: seamless ring noise (periodic in theta, no seam at ring wrap) | e511c97 | ✅ |
| World gen: cross-biome height blending (no cliff walls at biome borders) | e511c97 | ✅ |
| World gen: continental swell (+/- 4 blocks large-scale variety, occasional islands) | e511c97 | ✅ |
| World gen: trees generate across the ring seam (wrapped neighbor scan + placement) | e511c97 | ✅ |

Tests: 216 total (185 ring_world + 31 block_gallery), all passing.
Note: terrain shapes changed vs earlier builds (2D noise became periodic 3D/4D), so saves from before e511c97 will place their edits on different terrain.

## 2026-08-11 Batch 2: Mob Polish, Movement Fix, Terminator Warmth, VISION.md

- **Composite mob models** (`entity.rs build_entity_mesh` + `mob_parts`):
  every mob is now a Minecraft-style multi-box model (body, head, legs; snout/
  beak accents; 8 legs on spiders; arms on zombies/skeletons) instead of a
  single cube. Parts are defined in a mob-local (forward, side, up) frame from
  a data table, tinted from `render_color` with per-part multipliers.
- **Facing + walk animation**: new `Entity.facing` yaw (0 = +tangent) set from
  the walk direction, and `Entity.walk_phase` advanced per block walked. The
  model rotates about the surface normal to face travel; legs (and humanoid
  arms) swing in opposition with `sin(walk_phase)`. Rotation preserves frame
  handedness so the CCW-outward winding invariant holds (regression test
  updated to assert it at multiple facings).
- **Mob movement fix** (`can_stand_at` + step-up jump): horizontal collision
  now samples at FEET and HEAD level instead of body center, so mobs stop
  clipping into 1-block rises; a blocked 1-block step triggers a real jump
  impulse (`STEP_JUMP_SPEED = 6.8`, apex ~1.16 blocks) instead of the old
  clip-and-snap teleport that read as constant twitchy "jumping" on slopes.
  Axis-separated sliding kept as the fallback. New integration test walks a
  pig over a 1-block step on a real chunk and asserts it ends ON the step.
- **Terminator warmth** (`shader.wgsl`): `dusk = 4 * daylight * (1-daylight)`
  peaks mid-transition; sunlight and ambient blend toward warm amber there, so
  night arrives as a dawn/dusk-colored band sweeping the landscape. Validated
  with naga.
- **VISION.md**: new living direction doc (pitch: quiet megastructure
  survival; pillars; roadmap near/mid/far; art direction notes).

Tests: 217 total (186 ring_world + 31 block_gallery), all passing.

## 2026-08-11 Batch 3: Player Body, Distant Ring Relief, Biome Pass

- **Third-person player body (F9)**: humanoid composite model (skin head,
  cyan shirt torso + arms, indigo legs) rendered through the shared
  `emit_parts` path (refactored out of `build_entity_mesh` with a
  `model_frame` helper). Faces the camera yaw; limbs swing with ground
  distance walked (`Player.walk_phase`, frozen while airborne). Camera pulls
  back 5 blocks with a 0.6 lift. Known v1 limitation: no terrain clip check
  on the camera ray. Winding + extents test added.
- **Distant ring relief** (`build_inner_surface`, pure + tested): the arch
  overhead is now a real heightmap. Vertices displaced inward by sampled
  terrain height (oceans held flat at sea level), finite-difference normals
  so mountain relief on the arch catches sun light, seam-closed by sampling
  the wrap row at theta = 0, tessellation 128x16 -> 1024x24, depth-shaded
  water colors for any biome below sea level, noise mottling breaks banding.
  The sky is becoming a map (VISION mid-term item).
- **Biome pass** (`terrain.rs`):
  - Universal sea-level flooding: any biome column below SEA_LEVEL fills
    with water, turning blended-border dips and continental-swell lows into
    real lakes instead of dry pits (test scans generated chunks for illegal
    open-air voids).
  - Shoreline sand fringe: Plains/Forest/Mountains surfaces within 2 blocks
    of the waterline become sand, so every lake and coast has a beach.
  - Jittered snowline: mountain snow (~50) and bare-rock (~42) lines wander
    +/- 3-4 blocks by noise instead of razor-straight contours.
  - Vegetation: no trees or ground decorations underwater; Forest gets
    sparse flowers (~1%); Plains gets dense meadow flower patches where a
    low-frequency noise field crests.

Tests: 222 total (191 ring_world + 31 block_gallery), all passing.

