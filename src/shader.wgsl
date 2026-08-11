// Vertex shader for ring world voxel rendering

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_position: vec4<f32>,
};

struct SunUniform {
    position: vec4<f32>,
    color: vec4<f32>,    // rgb = color, a = intensity
    ambient: vec4<f32>,  // rgb = ambient color, w = F6 debug flag
    // Shadow-square eclipse (Niven Ringworld day/night):
    // x = square count (0 disables), y = orbital phase (rad),
    // z = square angular half-width (rad), w = penumbra softness (rad).
    eclipse: vec4<f32>,
};

struct ChunkTransform {
    model: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> sun: SunUniform;

@group(2) @binding(0)
var<uniform> transform: ChunkTransform;

@group(3) @binding(0)
var t_texture_array: texture_2d_array<f32>;
@group(3) @binding(1)
var s_texture_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) tex_index: u32,
    @location(5) light_level: f32,
    // 1 = this face uses alpha-cutout (leaves + cross-render plants); the
    // fragment shader is allowed to `discard` its (nearly) transparent texels.
    // 0 = a solid/opaque face (grass, dirt, stone, wood sides, ...); the shader
    // must NEVER discard it, so a solid face can never end up see-through even
    // if its texture or vertex alpha is momentarily < the cutout threshold.
    @location(6) alpha_tested: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) @interpolate(flat) tex_index: u32,
    @location(5) light_level: f32,
    @location(6) @interpolate(flat) alpha_tested: u32,
};

// Inverse of a 3x3 matrix (WGSL has no built-in mat inverse). Used to build
// the normal matrix (inverse-transpose of the model's upper-left 3x3) so the
// ring's non-uniform per-axis voxel scaling does not skew face normals.
fn inverse3(m: mat3x3<f32>) -> mat3x3<f32> {
    let a = m[0]; // column 0
    let b = m[1]; // column 1
    let c = m[2]; // column 2

    // Cofactors.
    let r0 = vec3<f32>(
        b.y * c.z - c.y * b.z,
        c.y * a.z - a.y * c.z,
        a.y * b.z - b.y * a.z,
    );
    let r1 = vec3<f32>(
        c.x * b.z - b.x * c.z,
        a.x * c.z - c.x * a.z,
        b.x * a.z - a.x * b.z,
    );
    let r2 = vec3<f32>(
        b.x * c.y - c.x * b.y,
        c.x * a.y - a.x * c.y,
        a.x * b.y - b.x * a.y,
    );

    let det = a.x * r0.x + b.x * r0.y + c.x * r0.z;
    let inv_det = 1.0 / det;

    // The inverse is (1/det) * adjugate. Assemble columns from the cofactor rows.
    return mat3x3<f32>(
        vec3<f32>(r0.x, r1.x, r2.x) * inv_det,
        vec3<f32>(r0.y, r1.y, r2.y) * inv_det,
        vec3<f32>(r0.z, r1.z, r2.z) * inv_det,
    );
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    let world_pos = transform.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;
    out.clip_position = camera.view_proj * world_pos;

    // Transform the normal by the INVERSE-TRANSPOSE of the model's upper-left
    // 3x3. The ring chunk transform uses a NON-UNIFORM scale (the tangent /
    // radial / axial voxel sizes differ), so transforming the normal with the
    // plain model matrix skews it off-axis. That skew (a) misclassified the F6
    // per-face debug tint for the four vertical side faces (making them read as
    // the wrong color / wash out) and (b) skewed the diffuse term on side
    // faces. Using the normal matrix keeps each face normal aligned with its
    // true world-space axis so the side faces tint and light correctly.
    let m3 = mat3x3<f32>(
        transform.model[0].xyz,
        transform.model[1].xyz,
        transform.model[2].xyz,
    );
    let normal_matrix = transpose(inverse3(m3));
    out.world_normal = normalize(normal_matrix * in.normal);
    
    out.color = in.color;
    out.tex_coords = in.tex_coords;
    out.tex_index = in.tex_index;
    out.light_level = in.light_level;
    out.alpha_tested = in.alpha_tested;
    
    return out;
}

// Per-face debug tint keyed by the world-space normal sign. Each of the SIX
// face directions gets a FULLY DISTINCT, saturated color (no dark/light pairs
// that are hard to tell apart) so you can name exactly which side fails to
// render in the F6 render-diagnostic mode:
//   +X = RED      (1,0,0)   = +ring / "east" tangent
//   -X = CYAN     (0,1,1)   = -ring / "west" tangent
//   +Y = GREEN    (0,1,0)   = up / toward the sun  (block TOP)
//   -Y = MAGENTA  (1,0,1)   = down / away from sun (block BOTTOM)
//   +Z = BLUE     (0,0,1)   = +width / "north" axial
//   -Z = YELLOW   (1,1,0)   = -width / "south" axial
fn debug_face_tint(normal: vec3<f32>) -> vec3<f32> {
    let ax = abs(normal.x);
    let ay = abs(normal.y);
    let az = abs(normal.z);
    if (ax >= ay && ax >= az) {
        if (normal.x >= 0.0) { return vec3<f32>(1.0, 0.0, 0.0); }   // +X RED
        return vec3<f32>(0.0, 1.0, 1.0);                            // -X CYAN
    }
    if (ay >= ax && ay >= az) {
        if (normal.y >= 0.0) { return vec3<f32>(0.0, 1.0, 0.0); }   // +Y GREEN
        return vec3<f32>(1.0, 0.0, 1.0);                            // -Y MAGENTA
    }
    if (normal.z >= 0.0) { return vec3<f32>(0.0, 0.0, 1.0); }       // +Z BLUE
    return vec3<f32>(1.0, 1.0, 0.0);                                // -Z YELLOW
}

// Fragment shader with sun lighting, texture sampling, and per-vertex light levels
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);

    // F6 render-diagnostic mode: sun.ambient.w (debug_mode) != 0 forces a
    // full-bright per-face normal tint and skips fog + alpha discard, so that
    // "missing geometry" vs "bad texture" vs "culling bug" can be told apart.
    if (sun.ambient.w != 0.0) {
        return vec4<f32>(debug_face_tint(normal), 1.0);
    }

    // Sample texture from the 2D texture array
    let tex_color = textureSample(t_texture_array, s_texture_sampler, in.tex_coords, in.tex_index);
    
    // Multiply texture color by vertex color (allows tinting)
    let base_color = tex_color * in.color;

    // Alpha cutout: discard (nearly) fully-transparent fragments so that
    // cross-billboard decorations (tall grass, flowers, mushrooms, vines) and
    // the transparent pixels of leaves do NOT write to the depth buffer. Without
    // this, the opaque blocks drawn afterward fail the depth test behind the
    // transparent texels, producing the "invisible blocks / black patches behind
    // grass and flowers" artifact. The cutoff keeps depth-writing on for the
    // solid texels (so foliage still occludes correctly) while letting whatever
    // is behind the holes show through.
    //
    // CRITICAL: the discard is now scoped to ONLY alpha-tested faces
    // (in.alpha_tested == 1u: leaves + cross-render plants). Solid block faces
    // (grass sides, dirt, stone, wood, snow, ...) carry alpha_tested == 0u and
    // are NEVER discarded. Previously this discard ran for EVERY block: any
    // solid face whose sampled/vertex alpha dipped below 0.5 was dropped, making
    // an otherwise-opaque face (e.g. a grass SIDE) render as see-through / show
    // nothing. Gating on alpha_tested guarantees solid faces always draw.
    if (in.alpha_tested != 0u && base_color.a < 0.5) {
        discard;
    }

    // Shadow-square eclipse: the sun never moves (eternal noon); night falls
    // when an orbiting shadow square passes between this FRAGMENT and the sun.
    // A radial ray from the sun to a point at ring angle theta crosses the
    // shadow-square orbit at the same angle, so occlusion is just the wrapped
    // angular distance from the fragment's theta to the nearest square center.
    // Computed per fragment so the terminator visibly sweeps the landscape and
    // the far side of the arch overhead stays lit during local night.
    // (Keep in sync with the CPU mirror ShadowSquares::daylight_at.)
    var daylight = 1.0;
    if (sun.eclipse.x > 0.5) {
        let frag_theta = atan2(in.world_position.z, in.world_position.x);
        let period = 6.28318530718 / sun.eclipse.x;
        let rel = frag_theta - sun.eclipse.y;
        let w = rel - period * floor(rel / period);
        let d = min(w, period - w);
        daylight = smoothstep(sun.eclipse.z, sun.eclipse.z + sun.eclipse.w, d);
    }

    // Direction from fragment to sun (at center of ring)
    let to_sun = normalize(sun.position.xyz - in.world_position);
    
    // Diffuse lighting (fully eclipsed by shadow squares)
    let diff = max(dot(normal, to_sun), 0.0);
    // Terminator warmth: while a shadow-square edge sweeps past (daylight in
    // mid-range) sunlight reddens like a fast dawn/dusk, so night arrives as
    // a warm amber band crossing the landscape instead of a neutral fade.
    // dusk peaks at 1.0 when daylight = 0.5 and is 0 at full day/full night.
    let dusk = 4.0 * daylight * (1.0 - daylight);
    let sun_rgb = mix(sun.color.rgb, vec3<f32>(1.0, 0.45, 0.25), dusk * 0.55);
    let diffuse = sun_rgb * sun.color.a * diff * daylight;
    
    // Ambient lighting (high on a ring world due to reflected light from the
    // opposite side). At night most of that reflected light is gone too, but
    // the lit arch overhead keeps a floor of ~18% so night is moody, not void.
    let ambient = sun.ambient.rgb * mix(0.18, 1.0, daylight)
        * mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.08, 0.92, 0.78), dusk * 0.5);
    
    // Simple specular
    let view_dir = normalize(camera.view_position.xyz - in.world_position);
    let reflect_dir = reflect(-to_sun, normal);
    let spec = pow(max(dot(view_dir, reflect_dir), 0.0), 32.0);
    let specular = sun_rgb * spec * 0.2 * daylight;
    
    // Combine directional lighting with base color
    let directional_lighting = ambient + diffuse + specular;
    let dir_lit_color = base_color.rgb * directional_lighting;
    
    // Apply per-vertex voxel light level (smooth lighting / ambient occlusion).
    // This modulates the final color based on how much light reaches this voxel.
    // Belt-and-suspenders floor: even if a vertex somehow carries a near-zero
    // baked light_level (e.g. a freshly exposed cliff side face sampled from an
    // unlit cell), clamp it so terrain faces are never darker than ~28% of the
    // texture and never render pure black. The F6 debug path returns earlier and
    // is unaffected by this clamp.
    let lvl = max(in.light_level, 0.28);
    let final_color = dir_lit_color * lvl;
    
    // Distance fog - dark space color with subtle sun glow
    let dist = length(camera.view_position.xyz - in.world_position);
    let fog_factor = clamp(dist / 2000.0, 0.0, 0.8);
    
    // Fog color: dark space with subtle warm glow toward sun direction
    let view_to_frag = normalize(in.world_position - camera.view_position.xyz);
    let sun_alignment = max(dot(view_to_frag, normalize(sun.position.xyz - camera.view_position.xyz)), 0.0);
    let sun_glow = pow(sun_alignment, 8.0) * 0.3 * daylight;
    let fog_color = vec3<f32>(0.01 + sun_glow * 0.2, 0.01 + sun_glow * 0.15, 0.03 + sun_glow * 0.05);
    
    let fogged = mix(final_color, fog_color, fog_factor);

    // Opaque/cutout geometry must NEVER write a sub-1.0 alpha: the opaque
    // pipeline has alpha-blending enabled, so any texel whose SAMPLED texture
    // alpha is < 1.0 (e.g. an overlaid PNG that carries a non-opaque alpha
    // channel) would blend the wall with whatever is behind it, making solid
    // walls look translucent / "see-through". The cutout `discard` above has
    // already removed the genuinely-empty texels (alpha < 0.5); every fragment
    // that survives an OPAQUE block is a solid surface and must be fully opaque.
    //
    // We distinguish opaque from translucent by the VERTEX alpha (in.color.a):
    // the mesher clamps opaque blocks' vertex alpha to exactly 1.0 and keeps
    // water's authored sub-1.0 alpha (~0.7). So:
    //   - vertex alpha >= 1.0  -> opaque/cutout face: force output alpha 1.0,
    //     ignoring any stray sub-1.0 texture alpha (fixes translucent walls).
    //   - vertex alpha  < 1.0  -> genuinely translucent (water): keep the
    //     blended alpha so the water pipeline alpha-blends correctly.
    let out_alpha = select(base_color.a, 1.0, in.color.a >= 1.0);
    return vec4<f32>(fogged, out_alpha);
}
