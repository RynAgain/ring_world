# Ring World — Block Visibility & Rendering Pipeline Review

Read-only systemic audit of the meshing / culling / texturing / shading path.
No code was changed. All references use `path:line`.

---

## Executive summary

The pipeline is mostly sound and the previously-fixed winding/PosZ issues are
confirmed correct (covered by unit tests). However the audit found **one
Critical bug guaranteed to produce "untextured / black" blocks** and several
**High** issues that explain recurring missing-face / invisible-block reports,
plus a structural ambiguity in how transparency is used for face culling that
causes double-drawn interior faces and z-fighting on water/leaves.

Prime suspects, in order:

1. **CRITICAL** — `Door` texture index (`TEX_DOOR = 31`) is the last valid layer; `Vine` maps to leaves but more importantly several non-PNG layers are fine, BUT the real problem is the **solidity vs. transparency model**: `Leaves` and `Cactus` are flagged in ways that drop faces. See Issue 2 and Issue 6.
2. **CRITICAL** — Culling uses **`is_transparent` only**, never a separate "occludes" notion, so a solid block next to leaves/water draws correctly, but **two adjacent leaves blocks do NOT cull their shared faces** and **water-against-water double-draws**, while **leaves treated as `is_solid=true, is_transparent=true` produce interior holes** elsewhere. See Issue 2.
3. **HIGH** — Ungenerated-neighbor boundary faces are always drawn (`map_or(true, …)`), producing a shell of faces at the loaded-area frontier and at top/bottom chunk layers. See Issue 1.

---

## Confirmed-correct items (no action)

- **Winding / normals**: [`Face::greedy_positions()`](src/chunk.rs:732) and [`Face::vertices_and_normal_scaled()`](src/chunk.rs:798) both pass the winding-vs-normal unit tests ([`greedy_quad_winding_matches_outward_normal`](src/chunk.rs:1066), [`lod_quad_winding_matches_outward_normal`](src/chunk.rs:1083)). PosZ/NegZ are no longer inverted.
- **Pipeline winding consistency**: `front_face: Ccw` + `cull_mode: Back` ([`renderer.rs:388`](src/renderer.rs:388)) is consistent with the CCW mesh winding **and** with the right-handed `chunk_transform`.
- **`chunk_transform` handedness**: [`chunk_transform()`](src/ring_world.rs:216) basis [tangent=(sin,0,-cos), radial_in=(-cos,0,-sin), axial=(0,1,0)] has determinant +1 (right-handed); it preserves winding. Per-height tangent scaling ([`ring_world.rs:243`](src/ring_world.rs:243)) is a uniform positive scale per axis — it cannot invert faces.
- **Frustum planes**: [`Frustum::from_view_proj()`](src/renderer.rs:56) correctly uses wgpu/D3D `[0,1]` NDC (near = row2, far = row3-row2) and normalizes. Sphere test sign is correct.
- **Block table length**: [`BLOCK_PROPERTIES`](src/block.rs:98) has exactly `VOXEL_TYPE_COUNT` (41) entries (asserted by [`block_properties_array_has_full_length`](src/block.rs:654)).
- **Light floor**: every meshing light path clamps to `.max(0.1)` ([`chunk.rs:117`](src/chunk.rs:117), [`chunk.rs:296`](src/chunk.rs:296)), so nothing is driven to pure black by `light_level=0` alone.

---

## Findings (prioritized)

### Issue 1 — Ungenerated neighbor chunk treated as AIR → boundary face shell
**Severity: HIGH**
**File/Fn**: [`Chunk::is_face_visible()`](src/chunk.rs:438) (all six arms, e.g. [`chunk.rs:441`](src/chunk.rs:441)).

**Root cause**: When the neighbor chunk is `None` (not loaded / off ring edge),
the code does `neighbors[idx].map_or(true, |n| …)`. `map_or(true, …)` means a
**missing neighbor is treated as transparent → the boundary face IS drawn.**
This is the *opposite* of the classic "missing neighbor = solid" bug, so you do
not get holes — instead you get **extra faces drawn at every chunk boundary
where the neighbor hasn't generated yet**, and a full skin of faces on the
top chunk layer (`height_index == chunks_height-1`, whose +Y neighbor is always
`None` per [`ChunkCoord::neighbor()`](src/ring_world.rs:188)) and bottom layer.

These extra faces are usually hidden once the neighbor loads and the chunk is
re-meshed — but dirty/rebuild is **not** triggered on the existing chunk when a
*new neighbor* finishes generating. A chunk only re-meshes when its **own**
`dirty` flag is set ([`renderer.rs:747`](src/renderer.rs:747)). A freshly
generated neighbor never marks the adjacent already-meshed chunk dirty, so the
boundary skin (or, after camera movement, the seam) persists until that chunk
is independently edited.

**Why it reads as "missing faces" too**: at the *top* of the world the +Y
neighbor is permanently `None`, so the top face of the highest solid block is
always emitted (good). But the inverse case — an opaque neighbor that *should*
cull — only happens after a re-mesh that may never fire. The net visual is
inconsistent seams at the load frontier.

**Recommended fix (conceptual)**:
- Decide a single boundary policy. For interior boundaries (neighbor exists in
  the world but isn't loaded yet) the correct behavior is to **defer**: mark the
  chunk dirty when any of its 6 neighbors transitions to `generated`, so it
  re-meshes with real neighbor data. Add a "neighbor became ready" notification
  in [`generate_pending_chunks()`](src/renderer.rs:886) that sets `dirty=true`
  on the 6 neighbors of each newly generated chunk.
- For true world edges (`neighbor()` returns `None` because width/height is out
  of range), keep drawing the face (current behavior is correct there).

---

### Issue 2 — Culling model conflates "transparent" with "non-occluding"; leaves/water faces are wrong
**Severity: CRITICAL**
**File/Fn**: [`Chunk::is_face_visible()`](src/chunk.rs:438) and
[`Chunk::is_face_solid()`](src/chunk.rs:86); block flags in
[`block.rs`](src/block.rs:184).

**Root cause**: Face culling emits a face when the neighbor is
`is_transparent()`. There is only **one** transparency notion in
[`BlockProperties`](src/block.rs:25): `is_transparent`. This single flag is
overloaded to mean both "light passes through" and "does not occlude the
neighbor's face". That produces three distinct defects:

1. **Leaves are `is_solid=true, is_transparent=true`** ([`block.rs:184`](src/block.rs:184)).
   Because they're transparent, **every face of every leaf block adjacent to
   another leaf block is emitted** (interior faces are *not* culled). For a tree
   canopy this multiplies leaf geometry and, combined with the alpha-cutout
   discard, yields heavy overdraw and visible interior quads "inside" the
   canopy. Same for `Cactus`? No — Cactus is `is_transparent=false`
   ([`block.rs:268`](src/block.rs:268)) so cactus is fine.

2. **Water vs. water double-draw**: Water is `is_transparent=true`
   ([`block.rs:160`](src/block.rs:160)). Two adjacent water voxels each emit the
   shared face → **coplanar double geometry** → z-fighting / flicker on water
   surfaces, and with `ALPHA_BLENDING` the overlapping translucent quads stack
   to a darker/!inconsistent color.

3. **A solid opaque block adjacent to leaves/water correctly draws its face**
   (desired) — this part works.

The standard fix is two predicates: `occludes(self_type, neighbor_type)` that
returns true only when the neighbor fully hides this face. The rule should be:
emit the face if the neighbor is air, OR the neighbor is transparent **and**
the neighbor is not the same type as the current block (so same-type translucent
runs — water/water, leaves/leaves — cull their shared interior faces). Glass-like
"cull against any opaque, never against same translucent type" is the Minecraft
rule.

**Recommended fix (conceptual)**:
- Add a helper (e.g. `fn neighbor_occludes(self_type, neighbor_type) -> bool`)
  used by [`is_face_visible()`](src/chunk.rs:438):
  - face hidden if `neighbor` is opaque (`!is_transparent`);
  - face hidden if `neighbor.is_transparent` **and** `neighbor == self_type`
    (interior of a same-type translucent volume);
  - otherwise face visible.
- Re-evaluate whether `Leaves` should be `is_transparent=true` for *culling*.
  Minecraft renders leaves as opaque-cutout (they DO occlude neighboring solid
  faces and DO cull leaf-vs-leaf interior). Consider a separate
  `is_render_opaque`/`occludes` flag so leaves keep `is_transparent=true` for
  light but `occludes=true` for meshing.

---

### Issue 3 — Greedy merge key omits per-corner light vector ordering; can split/keep faces but cannot drop them
**Severity: LOW (correctness OK) / MEDIUM (perf)**
**File/Fn**: greedy merge loop [`chunk.rs:235`](src/chunk.rs:235)–[`chunk.rs:278`](src/chunk.rs:278), [`lights_similar()`](src/chunk.rs:470).

**Analysis**:
- The merge key is `(VoxelType, tex_idx, light[4])` with `lights_similar`
  threshold 0.05. This is conservative: it never merges across a type or
  texture boundary, and it never produces a zero-area quad (width/height start
  at 1 and only grow while `visited` is false and the key matches). **No face
  can be dropped** by the greedy pass — every unvisited masked cell starts a new
  quad. Confirmed safe.
- It also cannot merge across a transparency boundary incorrectly, because the
  mask only contains cells that already passed `is_face_visible`, and the key
  includes `VoxelType`.
- **Minor**: `lights_similar` compares the 4 corner lights **positionally**.
  Because the AO corner ordering in [`get_corner_offsets()`](src/chunk.rs:122)
  is per-face-axis (not per-merged-run), two cells that should merge can fail
  the light test at run edges, slightly increasing quad count. Not a visibility
  bug — purely extra triangles.

**Recommended fix (conceptual)**: none required for correctness. If perf
matters, quantize light to the 0.05 bucket once and merge on the quantized
tuple.

---

### Issue 4 — Texture index range safety (full map)
**Severity: LOW (currently safe) — but fragile**
**File/Fn**: [`texture_index()`](src/texture.rs:47), `TEXTURE_COUNT = 32`
([`texture.rs:10`](src/texture.rs:10)), atlas upload [`texture.rs:829`](src/texture.rs:829).

**Analysis**: The GPU array has exactly 32 layers (0..31), and
`generate_texture_data()` fills layers 0..31. Every constant `TEX_*` is in
`0..=31`. I mapped every `VoxelType` through `texture_index()` for all three
face dirs:

| VoxelType | Top | Bottom | Side | Max layer | OK? |
|-----------|-----|--------|------|-----------|-----|
| Air | 3 | 3 | 3 | 3 | yes (never meshed) |
| Stone | 3 | 3 | 3 | 3 | yes |
| Dirt | 2 | 2 | 2 | 2 | yes |
| Grass | 0 | 2 | 1 | 2 | yes |
| Sand | 4 | 4 | 4 | 4 | yes |
| Water | 5 | 5 | 5 | 5 | yes |
| Wood | 7 | 7 | 6 | 7 | yes |
| Leaves | 8 | 8 | 8 | 8 | yes |
| Bedrock | 10 | 10 | 10 | 10 | yes |
| Snow | 9 | 9 | 9 | 9 | yes |
| IronOre | 11 | … | … | 11 | yes |
| GoldOre | 12 | | | 12 | yes |
| DiamondOre | 13 | | | 13 | yes |
| Gravel | 14 | | | 14 | yes |
| Cactus | 16 | 16 | 15 | 16 | yes |
| TallGrass | 17 | | | 17 | yes |
| Flower | 18 | | | 18 | yes |
| Mushroom | 19 | | | 19 | yes |
| Vine | 8 | | | 8 | yes (uses leaves) |
| CraftingTable | 20 | 27 | 21 | 27 | yes |
| Furnace | 23 | 23 | 22 | 23 | yes |
| Chest | 24 | | | 24 | yes |
| Torch | 25 | | | 25 | yes |
| Ladder | 26 | | | 26 | yes |
| Door | 31 | 31 | 31 | 31 | yes (last layer) |
| Plank | 27 | | | 27 | yes |
| Cobblestone | 28 | | | 28 | yes |
| IronIngot | 29 | | | 29 | yes |
| GoldIngot | 30 | | | 30 | yes |
| Wood tools | 27 | | | 27 | yes |
| Stone tools | 28 | | | 28 | yes |
| Iron tools | 29 | | | 29 | yes |

**No out-of-range index** today. But this is fragile: `TEXTURE_COUNT` and the
`generate_*` call list ([`texture.rs:114`](src/texture.rs:114)) are maintained
by hand. Adding a block + TEX constant without bumping `TEXTURE_COUNT` would
silently sample a black/garbage layer.

**Recommended fix (conceptual)**: add a debug-assert/test that, for every
`VoxelType` and every `FaceDir`, `texture_index(..) < TEXTURE_COUNT`; and a test
that the number of `generate_*` writes equals `TEXTURE_COUNT`.

---

### Issue 5 — Single-pass rendering of translucent geometry with alpha blending
**Severity: HIGH**
**File/Fn**: pipeline blend `ALPHA_BLENDING` + `depth_write_enabled: true`
([`renderer.rs:380`](src/renderer.rs:380), [`renderer.rs:396`](src/renderer.rs:396)); single chunk draw loop [`renderer.rs:1298`](src/renderer.rs:1298); shader cutout [`shader.wgsl:90`](src/shader.wgsl:90).

**Root cause**: All chunk geometry (opaque + water + leaves cutout) is drawn in
**one pass**, in `HashMap` iteration order (non-deterministic), with alpha
blending enabled **and** depth-write enabled.

- For the **alpha-cutout** part (`base_color.a < 0.5 → discard`), this is fine
  and is actually the correct fix for the "black patch behind grass" artifact
  the comment describes — discarded fragments don't write depth.
- For genuinely **translucent** water (texture alpha 200/255 ≈ 0.78, vertex
  color alpha 0.7 → combined ≈ 0.55, *above* the 0.5 cutoff so it is NOT
  discarded and IS alpha-blended), drawing with depth-write on and arbitrary
  order means: water drawn before the terrain behind it writes depth and can
  **occlude** that terrain (terrain fails depth test) → "missing blocks behind
  water"; water drawn after blends over already-shaded terrain inconsistently.
  Because draw order is `HashMap` iteration (unstable), the artifact flickers
  per frame / per reload.

**Recommended fix (conceptual)**:
- Split rendering into two passes: opaque/cutout first (depth write on, no
  blend or alpha-to-coverage), then translucent (water) sorted back-to-front
  with `depth_write_enabled=false`. At minimum, separate water into its own
  vertex stream per chunk and draw it after all opaque chunks with depth-write
  off.
- Alternatively raise water's combined alpha above cutoff and treat it as
  cutout-opaque (loses translucency but removes the ordering bug).

---

### Issue 6 — Water/leaves alpha interaction with the 0.5 cutout can discard legitimate texels
**Severity: MEDIUM**
**File/Fn**: [`shader.wgsl:90`](src/shader.wgsl:90); water texture alpha
[`texture.rs:339`](src/texture.rs:341) (alpha 200); water vertex color alpha 0.7
([`voxel.rs:108`](src/voxel.rs:108)); but note meshing hardcodes vertex color to
opaque white.

**Root cause / nuance**: The greedy/LOD/cross builders set vertex
`color = [1,1,1,1]` ([`chunk.rs:599`](src/chunk.rs:599),
[`chunk.rs:638`](src/chunk.rs:638), [`chunk.rs:538`](src/chunk.rs:538)) — they do
**not** use `VoxelType::face_color()`. So per-face tint (green grass top, water
blue, leaves tint) is **never applied** to terrain; only the texture's own RGBA
matters. Consequences:
- Water final alpha = textureAlpha(≈0.78) × 1.0 = 0.78 > 0.5 → not discarded
  (blended). OK-ish, but see Issue 5 ordering.
- Leaves texture writes `alpha 230` for foliage texels and `alpha 0` for the
  `h%5==0` holes ([`texture.rs:392`](src/texture.rs:392)). 230/255 ≈ 0.90 > 0.5
  so leaf texels survive; holes (alpha 0) are discarded. Correct.
- The **risk**: any texture whose authored alpha dips near/below 0.5 on
  legitimately-opaque texels would be discarded → "holes in blocks". None of the
  current procedural textures do this except by design (leaves/foliage cutouts),
  but PNG overlays for `water.png`/`leaves.png` ([`texture.rs:159`](src/texture.rs:163))
  are user-supplied and could introduce sub-0.5 alpha and produce holes.

Also note grass top/side no longer gets its green tint, so it relies entirely on
`grass_top.png`/procedural green. If a PNG is grayscale-ish it will look washed
out (not black, but "untextured-ish").

**Recommended fix (conceptual)**: either (a) feed `face_color()` into the vertex
`color` so tints apply and so transparency is intentional, or (b) document that
authored textures must keep opaque texels at alpha ≥ ~0.6. Keep the cutout but
consider lowering threshold to ~0.1 for true cutout and handling translucency
via the separate pass (Issue 5).

---

### Issue 7 — Lighting ordering is correct, but neighbor light at unloaded boundaries defaults to 15 (can over-brighten, not black)
**Severity: LOW**
**File/Fn**: [`sample_light_at()`](src/chunk.rs:145); compute ordering in
[`generate_pending_chunks()`](src/renderer.rs:911) and dirty path
[`renderer.rs:753`](src/renderer.rs:753).

**Analysis**:
- **Ordering is correct**: lighting is computed *before* meshing in both paths
  — initial generation calls `compute_lighting` inside the same parallel task as
  `generate_chunk` ([`renderer.rs:913`](src/renderer.rs:913)), and dirty chunks
  recompute lighting ([`renderer.rs:755`](src/renderer.rs:755)) in a loop that
  runs *before* the mesh-building `par_iter` ([`renderer.rs:769`](src/renderer.rs:769)).
  So there is no "mesh built before light computed → all-black" bug.
- **Floor exists**: corner lights clamp to `.max(0.1)` ([`chunk.rs:117`](src/chunk.rs:117))
  and cross-quads to `.max(0.1)` ([`chunk.rs:296`](src/chunk.rs:296)). Nothing is
  pure black from lighting alone.
- **Minor**: out-of-range light sampling returns `15` for +X/-X/+Y/+Z/-Z and
  `0` only for -Y ([`chunk.rs:177`](src/chunk.rs:177)). At an unloaded boundary
  this slightly over-brightens edge faces rather than darkening them — not a
  visibility bug, but it can mask the seam issue from Issue 1.

**Recommended fix**: none required for visibility. For polish, sample neighbor
chunk light when the neighbor is loaded (the function already does for loaded
neighbors); the `15` default is acceptable.

---

### Issue 8 — Coverage guarantee: every solid block has a real texture & every face has an index
**Severity: PASS (no defect) — note on tools**
**File/Fn**: [`texture_index()`](src/texture.rs:47), [`block.rs`](src/block.rs:98).

**Analysis**:
- The `match` in `texture_index()` is exhaustive over `VoxelType` (Rust would
  not compile otherwise), so **every block and every face direction always gets
  a defined index**. There is no face direction that ends up without a texture.
- No `is_solid=true` block falls through to a black/default layer: even `Air`
  maps to `TEX_STONE` (3), and Air is never meshed. Every solid block points at
  a layer that `generate_texture_data()` actually fills.
- **Tools** (`WoodPickaxe`…`IronSword`) are `is_solid=false, is_transparent=true`
  and map to plank/cobblestone/iron-ingot textures. They are never placed as
  world blocks (they're inventory items), so they never mesh — harmless.
- **Vine** maps to `TEX_LEAVES` and is cross-rendered ([`is_cross_render()`](src/chunk.rs:485)),
  so it shows the leaf texture on an X-billboard. Acceptable per the code's own
  note.

**Conclusion**: no coverage gap. The only black-block risk is the indirect one
from an out-of-range index if `TEXTURE_COUNT` drifts (Issue 4) — not a current
defect.

---

## Recommended diagnostic mode (proposed F6 toggle)

Add an in-game **"render debug"** mode (toggle key F6, mirroring the existing
F3/F4/F5 pattern in [`renderer.rs`](src/renderer.rs:178)) that makes the three
failure classes visually distinguishable at a glance:

1. **Disable ALL culling**: force `enable_frustum_cull=false`,
   `enable_occlusion_cull=false`, and skip the per-chunk frustum test in the
   draw loop ([`renderer.rs:1305`](src/renderer.rs:1305)). This isolates
   "geometry exists but is being culled" from "geometry was never built".
2. **Force full-bright**: pass a uniform flag into the shader that overrides
   `in.light_level` to `1.0` and sets `directional_lighting` to `1.0` and
   `fog_factor` to `0.0` in [`fs_main`](src/shader.wgsl:73). This isolates
   "block is there but black due to lighting/fog" from "block is missing".
3. **Per-face debug tint**: when the debug flag is set, replace the sampled
   texture with a flat color keyed by the face normal sign (e.g. +X red, -X
   dark-red, +Y green, -Y dark-green, +Z blue, -Z dark-blue), computed from
   `in.world_normal` in the shader. This makes:
   - **missing geometry** → background space color shows through (a hole);
   - **missing texture / wrong layer** → still shows the debug tint (so you know
     geometry + normal are fine and the problem is the texture index);
   - **back-face / winding errors** → faces that vanish only with culling on but
     reappear in F6 reveal a winding bug;
   - **culling errors** → chunks that pop in only under F6 reveal frustum/
     occlusion over-culling.

Implementation sketch (no code here, per task constraints):
- Extend `SunUniform` (or add a tiny `DebugUniform` on a spare binding) with a
  `debug_mode: u32` field.
- In [`fs_main`](src/shader.wgsl:73): `if (debug_mode == 1u) { return vec4(face_tint(normal), 1.0); }`
  before the lighting/fog math, and skip the alpha discard so even cutout
  geometry is solid in debug.
- Wire an F6 handler next to the F4/F5 handlers (input handling lives outside
  these files — search for where `enable_frustum_cull` is toggled).

A second useful sub-mode: **wireframe** via `PolygonMode::Line` (requires the
`POLYGON_MODE_LINE` feature) on a dedicated pipeline, to see degenerate/merged
quads directly.

---

## Prioritized ordered fix list (what to fix first)

1. **[CRITICAL] Issue 2 — Fix the cull predicate.** Introduce an
   `occludes(self, neighbor)` rule so opaque neighbors cull, same-type
   translucent neighbors cull their shared interior faces (water/water,
   leaves/leaves), and only true air / different-type-transparent neighbors emit
   faces. This removes leaf-canopy interior overdraw and water double-draw. Add a
   separate `occludes` flag (or reclassify `Leaves` as render-opaque-cutout).
2. **[HIGH] Issue 5 — Two-pass / depth-write-off translucency.** Render opaque +
   cutout first (depth write on), then water in a second pass with depth write
   off (sorted back-to-front, or at least after all opaque). Fixes "missing
   blocks behind water" and the per-frame flicker from `HashMap` draw order.
3. **[HIGH] Issue 1 — Re-mesh on neighbor readiness.** In
   `generate_pending_chunks()`, after a chunk becomes `generated`, mark its 6
   existing neighbors `dirty` so boundary faces are recomputed with real
   neighbor data. Eliminates the load-frontier face shell / seams.
4. **[MEDIUM] Issue 6 — Tint + alpha policy.** Decide whether terrain vertex
   color should carry `face_color()` (restores grass/water tint) and document
   that authored PNG opaque texels must stay at alpha ≥ ~0.6 to avoid accidental
   cutout holes.
5. **[LOW] Issue 4 — Add safety tests.** Assert `texture_index(t, f) <
   TEXTURE_COUNT` for all types/faces and that the `generate_*` write count
   equals `TEXTURE_COUNT`, to prevent future black-layer regressions.
6. **[LOW] Issue 3 / Issue 7 — Polish.** Optional greedy light quantization for
   fewer quads; optional neighbor-aware edge lighting. No visibility impact.
7. **[Tooling] Add the F6 diagnostic mode** early — it will make verifying fixes
   1–3 dramatically faster.
