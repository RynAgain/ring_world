/// Texture atlas system for the ring world voxel game
/// Uses a 2D texture array where each layer is a 24x24 procedural texture

use crate::voxel::{VoxelType, FaceDir};

/// Texture size in pixels
pub const TEXTURE_SIZE: u32 = 24;

/// Number of texture layers in the array
pub const TEXTURE_COUNT: u32 = 32;

/// Texture indices for each block face
pub const TEX_GRASS_TOP: u32 = 0;
pub const TEX_GRASS_SIDE: u32 = 1;
pub const TEX_DIRT: u32 = 2;
pub const TEX_STONE: u32 = 3;
pub const TEX_SAND: u32 = 4;
pub const TEX_WATER: u32 = 5;
pub const TEX_WOOD_SIDE: u32 = 6;
pub const TEX_WOOD_TOP: u32 = 7;
pub const TEX_LEAVES: u32 = 8;
pub const TEX_SNOW: u32 = 9;
pub const TEX_BEDROCK: u32 = 10;
pub const TEX_IRON_ORE: u32 = 11;
pub const TEX_GOLD_ORE: u32 = 12;
pub const TEX_DIAMOND_ORE: u32 = 13;
pub const TEX_GRAVEL: u32 = 14;
pub const TEX_CACTUS_SIDE: u32 = 15;
pub const TEX_CACTUS_TOP: u32 = 16;
pub const TEX_TALL_GRASS: u32 = 17;
pub const TEX_FLOWER: u32 = 18;
pub const TEX_MUSHROOM: u32 = 19;
pub const TEX_CRAFTING_TABLE_TOP: u32 = 20;
pub const TEX_CRAFTING_TABLE_SIDE: u32 = 21;
pub const TEX_FURNACE_SIDE: u32 = 22;
pub const TEX_FURNACE_TOP: u32 = 23;
pub const TEX_CHEST: u32 = 24;
pub const TEX_TORCH: u32 = 25;
pub const TEX_LADDER: u32 = 26;
pub const TEX_PLANK: u32 = 27;
pub const TEX_COBBLESTONE: u32 = 28;
pub const TEX_IRON_INGOT: u32 = 29;
pub const TEX_GOLD_INGOT: u32 = 30;
pub const TEX_DOOR: u32 = 31;

/// Get the texture index for a given voxel type and face direction
pub fn texture_index(voxel_type: VoxelType, face: FaceDir) -> u32 {
    match voxel_type {
        VoxelType::Air => TEX_STONE,
        VoxelType::Grass => match face {
            FaceDir::Top => TEX_GRASS_TOP,
            FaceDir::Bottom => TEX_DIRT,
            FaceDir::Side => TEX_GRASS_SIDE,
        },
        VoxelType::Dirt => TEX_DIRT,
        VoxelType::Stone => TEX_STONE,
        VoxelType::Sand => TEX_SAND,
        VoxelType::Water => TEX_WATER,
        VoxelType::Wood => match face {
            FaceDir::Top | FaceDir::Bottom => TEX_WOOD_TOP,
            FaceDir::Side => TEX_WOOD_SIDE,
        },
        VoxelType::Leaves => TEX_LEAVES,
        VoxelType::Snow => TEX_SNOW,
        VoxelType::Bedrock => TEX_BEDROCK,
        VoxelType::IronOre => TEX_IRON_ORE,
        VoxelType::GoldOre => TEX_GOLD_ORE,
        VoxelType::DiamondOre => TEX_DIAMOND_ORE,
        VoxelType::Gravel => TEX_GRAVEL,
        VoxelType::Cactus => match face {
            FaceDir::Top | FaceDir::Bottom => TEX_CACTUS_TOP,
            FaceDir::Side => TEX_CACTUS_SIDE,
        },
        VoxelType::TallGrass => TEX_TALL_GRASS,
        VoxelType::Flower => TEX_FLOWER,
        VoxelType::Mushroom => TEX_MUSHROOM,
        VoxelType::Vine => TEX_LEAVES,
        VoxelType::CraftingTable => match face {
            FaceDir::Top => TEX_CRAFTING_TABLE_TOP,
            FaceDir::Bottom => TEX_PLANK,
            FaceDir::Side => TEX_CRAFTING_TABLE_SIDE,
        },
        VoxelType::Furnace => match face {
            FaceDir::Top | FaceDir::Bottom => TEX_FURNACE_TOP,
            FaceDir::Side => TEX_FURNACE_SIDE,
        },
        VoxelType::Chest => TEX_CHEST,
        VoxelType::Torch => TEX_TORCH,
        VoxelType::Ladder => TEX_LADDER,
        VoxelType::Door => TEX_DOOR,
        VoxelType::Plank => TEX_PLANK,
        VoxelType::Cobblestone => TEX_COBBLESTONE,
        VoxelType::IronIngot => TEX_IRON_INGOT,
        VoxelType::GoldIngot => TEX_GOLD_INGOT,
        VoxelType::WoodPickaxe | VoxelType::WoodAxe | VoxelType::WoodShovel | VoxelType::WoodSword => TEX_PLANK,
        VoxelType::StonePickaxe | VoxelType::StoneAxe | VoxelType::StoneShovel | VoxelType::StoneSword => TEX_COBBLESTONE,
        VoxelType::IronPickaxe | VoxelType::IronAxe | VoxelType::IronShovel | VoxelType::IronSword => TEX_IRON_INGOT,
    }
}

fn hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x.wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    h
}

pub fn generate_texture_data() -> Vec<u8> {
    let layer_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
    let mut data = vec![0u8; layer_size * TEXTURE_COUNT as usize];

    generate_grass_top(&mut data[layer_size * 0..layer_size * 1]);
    generate_grass_side(&mut data[layer_size * 1..layer_size * 2]);
    generate_dirt(&mut data[layer_size * 2..layer_size * 3]);
    generate_stone(&mut data[layer_size * 3..layer_size * 4]);
    generate_sand(&mut data[layer_size * 4..layer_size * 5]);
    generate_water(&mut data[layer_size * 5..layer_size * 6]);
    generate_wood_side(&mut data[layer_size * 6..layer_size * 7]);
    generate_wood_top(&mut data[layer_size * 7..layer_size * 8]);
    generate_leaves(&mut data[layer_size * 8..layer_size * 9]);
    generate_snow(&mut data[layer_size * 9..layer_size * 10]);
    generate_bedrock(&mut data[layer_size * 10..layer_size * 11]);
    generate_iron_ore(&mut data[layer_size * 11..layer_size * 12]);
    generate_gold_ore(&mut data[layer_size * 12..layer_size * 13]);
    generate_diamond_ore(&mut data[layer_size * 13..layer_size * 14]);
    generate_gravel(&mut data[layer_size * 14..layer_size * 15]);
    generate_cactus_side(&mut data[layer_size * 15..layer_size * 16]);
    generate_cactus_top(&mut data[layer_size * 16..layer_size * 17]);
    generate_tall_grass(&mut data[layer_size * 17..layer_size * 18]);
    generate_flower(&mut data[layer_size * 18..layer_size * 19]);
    generate_mushroom(&mut data[layer_size * 19..layer_size * 20]);
    generate_crafting_table_top(&mut data[layer_size * 20..layer_size * 21]);
    generate_crafting_table_side(&mut data[layer_size * 21..layer_size * 22]);
    generate_furnace_side(&mut data[layer_size * 22..layer_size * 23]);
    generate_furnace_top(&mut data[layer_size * 23..layer_size * 24]);
    generate_chest(&mut data[layer_size * 24..layer_size * 25]);
    generate_torch(&mut data[layer_size * 25..layer_size * 26]);
    generate_ladder(&mut data[layer_size * 26..layer_size * 27]);
    generate_plank(&mut data[layer_size * 27..layer_size * 28]);
    generate_cobblestone(&mut data[layer_size * 28..layer_size * 29]);
    generate_iron_ingot(&mut data[layer_size * 29..layer_size * 30]);
    generate_gold_ingot(&mut data[layer_size * 30..layer_size * 31]);
    generate_door(&mut data[layer_size * 31..layer_size * 32]);

    // Attempt to override specific layers with PNG files from the `textures/`
    // folder. Layers without a corresponding file keep their procedural data.
    // Failures (missing/corrupt files) are logged and fall back to procedural.
    overlay_png_textures(&mut data, layer_size);

    // Belt-and-suspenders: force every OPAQUE (solid-block) layer to a fully
    // opaque alpha channel (255). Solid blocks must never carry a sub-255 alpha
    // texel, because the shader's alpha-cutout `discard` (threshold 0.5) would
    // drop that texel and make the otherwise-solid face render see-through. This
    // catches both a procedural generator that left a pixel unwritten (alpha 0
    // from the zero-init) AND an overlaid PNG that happens to carry a non-opaque
    // alpha channel. Genuinely alpha-tested layers (leaves + cross-render
    // plants) and translucent layers (water) keep their authored alpha so their
    // intended transparency / cutout holes survive.
    force_opaque_layer_alpha(&mut data, layer_size);

    data
}

/// Layers whose authored alpha must be PRESERVED (not forced to 255): the
/// alpha-cutout foliage (leaves + cross-render plants, which have transparent
/// holes) and translucent water. Every other layer is a solid block face and is
/// forced fully opaque.
pub fn layer_keeps_alpha(layer: u32) -> bool {
    matches!(
        layer,
        TEX_WATER
            | TEX_LEAVES
            | TEX_TALL_GRASS
            | TEX_FLOWER
            | TEX_MUSHROOM
            | TEX_TORCH
            | TEX_LADDER
    )
}

/// Force the alpha channel of every opaque (solid-block) layer to 255 so the
/// shader's alpha-cutout discard can never drop a solid face's texel.
fn force_opaque_layer_alpha(data: &mut [u8], layer_size: usize) {
    for layer in 0..TEXTURE_COUNT {
        if layer_keeps_alpha(layer) {
            continue;
        }
        let offset = layer as usize * layer_size;
        let end = offset + layer_size;
        // Alpha is the 4th byte of each RGBA texel.
        let mut i = offset + 3;
        while i < end {
            data[i] = 255;
            i += 4;
        }
    }
}

/// Map a texture layer index to its corresponding PNG filename in `textures/`.
/// Returns `None` for layers that have no PNG (those keep procedural textures).
fn png_filename_for_layer(layer: u32) -> Option<&'static str> {
    match layer {
        TEX_GRASS_TOP => Some("grass_top.png"),
        TEX_GRASS_SIDE => Some("grass_side.png"),
        TEX_DIRT => Some("dirt.png"),
        TEX_STONE => Some("stone.png"),
        TEX_SAND => Some("sand.png"),
        TEX_WATER => Some("water.png"),
        TEX_LEAVES => Some("leaves.png"),
        TEX_SNOW => Some("snow.png"),
        TEX_BEDROCK => Some("bedrock.png"),
        _ => None,
    }
}

/// For each layer that has a corresponding PNG file, load it, resize it to the
/// atlas tile size with nearest-neighbor sampling (to keep pixel art crisp), and
/// write it over the procedural data for that layer. If a file is missing or
/// fails to load, a warning is logged and the procedural texture is kept.
fn overlay_png_textures(data: &mut [u8], layer_size: usize) {
    for layer in 0..TEXTURE_COUNT {
        let filename = match png_filename_for_layer(layer) {
            Some(name) => name,
            None => continue,
        };
        let path = format!("textures/{}", filename);
        match image::open(&path) {
            Ok(img) => {
                let resized = img.resize_exact(
                    TEXTURE_SIZE,
                    TEXTURE_SIZE,
                    image::imageops::FilterType::Nearest,
                );
                let rgba = resized.to_rgba8();
                let raw = rgba.as_raw();
                let offset = layer as usize * layer_size;
                if raw.len() == layer_size {
                    data[offset..offset + layer_size].copy_from_slice(raw);
                } else {
                    log::warn!(
                        "texture '{}' decoded to unexpected size ({} bytes, expected {}); using procedural fallback",
                        path,
                        raw.len(),
                        layer_size
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "failed to load texture '{}' ({}); using procedural fallback",
                    path,
                    e
                );
            }
        }
    }
}

fn set_pixel(data: &mut [u8], x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
    let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
    if idx + 3 < data.len() {
        data[idx] = r;
        data[idx + 1] = g;
        data[idx + 2] = b;
        data[idx + 3] = a;
    }
}

fn generate_grass_top(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 100);
            let v = (h % 30) as i16 - 15;
            let r = (76i16 + v / 2).clamp(0, 255) as u8;
            let g = (140i16 + v).clamp(0, 255) as u8;
            let b = (50i16 + v / 3).clamp(0, 255) as u8;
            if h % 7 == 0 {
                set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(40), b.saturating_sub(20), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_grass_side(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 200);
            if y < 4 {
                let v = (h % 20) as i16 - 10;
                set_pixel(data, x, y, (76i16+v/2).clamp(0,255) as u8, (140i16+v).clamp(0,255) as u8, (50i16+v/3).clamp(0,255) as u8, 255);
            } else if y < 7 {
                let blade = hash(x, 0, 201) % 5;
                let blade_len = (hash(x, 1, 202) % 4) as u32 + 2;
                if blade == 0 && (y as u32) < 4 + blade_len {
                    let v = (h % 20) as i16 - 10;
                    set_pixel(data, x, y, (76i16+v/2).clamp(0,255) as u8, (130i16+v).clamp(0,255) as u8, (50i16+v/3).clamp(0,255) as u8, 255);
                } else {
                    let v = (h % 20) as i16 - 10;
                    set_pixel(data, x, y, (140i16+v).clamp(0,255) as u8, (100i16+v/2).clamp(0,255) as u8, (60i16+v/3).clamp(0,255) as u8, 255);
                }
            } else {
                let v = (h % 20) as i16 - 10;
                let r = (140i16+v).clamp(0,255) as u8;
                let g = (100i16+v/2).clamp(0,255) as u8;
                let b = (60i16+v/3).clamp(0,255) as u8;
                if h % 11 == 0 {
                    set_pixel(data, x, y, r.saturating_sub(25), g.saturating_sub(20), b.saturating_sub(15), 255);
                } else {
                    set_pixel(data, x, y, r, g, b, 255);
                }
            }
        }
    }
}

fn generate_dirt(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 300);
            let v = (h % 24) as i16 - 12;
            let r = (140i16+v).clamp(0,255) as u8;
            let g = (100i16+v/2).clamp(0,255) as u8;
            let b = (60i16+v/3).clamp(0,255) as u8;
            if h % 9 == 0 {
                set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(25), b.saturating_sub(15), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_stone(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 400);
            let v = (h % 30) as i16 - 15;
            let base = 128i16 + v;
            let r = base.clamp(0,255) as u8;
            let g = base.clamp(0,255) as u8;
            let b = (base+5).clamp(0,255) as u8;
            let crack1 = ((x as i16 - 12).abs() + (y as i16 - 8).abs()) < 3;
            let crack2 = ((x as i16 - 6).abs() + (y as i16 - 18).abs()) < 2;
            let crack3 = ((x as i16 - 18).abs() + (y as i16 - 15).abs()) < 3;
            if crack1 || crack2 || crack3 || h % 13 == 0 {
                set_pixel(data, x, y, r.saturating_sub(40), g.saturating_sub(40), b.saturating_sub(35), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_sand(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 500);
            let v = (h % 16) as i16 - 8;
            let r = (220i16+v).clamp(0,255) as u8;
            let g = (200i16+v).clamp(0,255) as u8;
            let b = (140i16+v/2).clamp(0,255) as u8;
            if h % 15 == 0 {
                set_pixel(data, x, y, r.saturating_sub(15), g.saturating_sub(15), b.saturating_sub(10), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_water(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 600);
            let v = (h % 20) as i16 - 10;
            let wave = ((x as f32 * 0.5 + y as f32 * 0.3).sin() * 10.0) as i16;
            let r = (50i16 + v/2 + wave/3).clamp(0,255) as u8;
            let g = (100i16 + v + wave/2).clamp(0,255) as u8;
            let b = (200i16 + v + wave).clamp(0,255) as u8;
            if (x + y*3) % 12 < 2 {
                set_pixel(data, x, y, r.saturating_add(30), g.saturating_add(30), b.saturating_add(25), 200);
            } else {
                set_pixel(data, x, y, r, g, b, 200);
            }
        }
    }
}

fn generate_wood_side(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 700);
            let v = (h % 16) as i16 - 8;
            let r = (100i16+v).clamp(0,255) as u8;
            let g = (70i16+v/2).clamp(0,255) as u8;
            let b = (35i16+v/3).clamp(0,255) as u8;
            let bark_line = (x % 4 == 0) || (x % 7 == 0 && h % 3 == 0);
            if bark_line {
                set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(20), b.saturating_sub(10), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_wood_top(data: &mut [u8]) {
    let cx = TEXTURE_SIZE as f32 / 2.0;
    let cy = TEXTURE_SIZE as f32 / 2.0;
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 800);
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx*dx + dy*dy).sqrt();
            let v = (h % 12) as i16 - 6;
            let ring = (dist * 1.5) as u32 % 4;
            let (r, g, b) = if ring < 2 {
                ((120i16+v).clamp(0,255) as u8, (85i16+v/2).clamp(0,255) as u8, (45i16+v/3).clamp(0,255) as u8)
            } else {
                ((90i16+v).clamp(0,255) as u8, (60i16+v/2).clamp(0,255) as u8, (30i16+v/3).clamp(0,255) as u8)
            };
            set_pixel(data, x, y, r, g, b, 255);
        }
    }
}

fn generate_leaves(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 900);
            let v = (h % 30) as i16 - 15;
            if h % 5 == 0 {
                set_pixel(data, x, y, 0, 0, 0, 0);
            } else {
                set_pixel(data, x, y, (40i16+v/2).clamp(0,255) as u8, (120i16+v).clamp(0,255) as u8, (30i16+v/3).clamp(0,255) as u8, 230);
            }
        }
    }
}

fn generate_snow(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1000);
            let v = (h % 10) as i16 - 5;
            set_pixel(data, x, y, (240i16+v).clamp(0,255) as u8, (242i16+v).clamp(0,255) as u8, (248i16+v).clamp(0,255) as u8, 255);
        }
    }
}

fn generate_bedrock(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1100);
            let v = (h % 20) as i16 - 10;
            let base = 40i16 + v;
            let r = base.clamp(0,255) as u8;
            let g = base.clamp(0,255) as u8;
            let b = (base+2).clamp(0,255) as u8;
            if h % 8 == 0 {
                set_pixel(data, x, y, r.saturating_sub(20), g.saturating_sub(20), b.saturating_sub(20), 255);
            } else {
                set_pixel(data, x, y, r, g, b, 255);
            }
        }
    }
}

fn generate_iron_ore(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1200);
            let v = (h % 30) as i16 - 15;
            let base = 128i16 + v;
            let ore_h = hash(x, y, 1201);
            let is_ore = ore_h % 8 == 0
                || (((x as i16-8).abs() < 3) && ((y as i16-8).abs() < 3) && ore_h % 3 == 0)
                || (((x as i16-16).abs() < 2) && ((y as i16-16).abs() < 2) && ore_h % 2 == 0)
                || (((x as i16-5).abs() < 2) && ((y as i16-18).abs() < 2));
            if is_ore {
                set_pixel(data, x, y, (180i16+(h%20) as i16-10).clamp(0,255) as u8, (120i16+(h%16) as i16-8).clamp(0,255) as u8, (70i16+(h%12) as i16-6).clamp(0,255) as u8, 255);
            } else {
                let r = base.clamp(0,255) as u8;
                let g = base.clamp(0,255) as u8;
                let b = (base+5).clamp(0,255) as u8;
                if h % 13 == 0 { set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(30), b.saturating_sub(25), 255); }
                else { set_pixel(data, x, y, r, g, b, 255); }
            }
        }
    }
}

fn generate_gold_ore(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1300);
            let v = (h % 30) as i16 - 15;
            let base = 128i16 + v;
            let ore_h = hash(x, y, 1301);
            let is_ore = ore_h % 10 == 0
                || (((x as i16-10).abs() < 2) && ((y as i16-6).abs() < 2) && ore_h % 2 == 0)
                || (((x as i16-18).abs() < 3) && ((y as i16-12).abs() < 2) && ore_h % 3 == 0)
                || (((x as i16-6).abs() < 2) && ((y as i16-20).abs() < 2));
            if is_ore {
                set_pixel(data, x, y, (230i16+(h%20) as i16-10).clamp(0,255) as u8, (200i16+(h%16) as i16-8).clamp(0,255) as u8, (50i16+(h%12) as i16-6).clamp(0,255) as u8, 255);
            } else {
                let r = base.clamp(0,255) as u8;
                let g = base.clamp(0,255) as u8;
                let b = (base+5).clamp(0,255) as u8;
                if h % 13 == 0 { set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(30), b.saturating_sub(25), 255); }
                else { set_pixel(data, x, y, r, g, b, 255); }
            }
        }
    }
}

fn generate_diamond_ore(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1400);
            let v = (h % 30) as i16 - 15;
            let base = 128i16 + v;
            let ore_h = hash(x, y, 1401);
            let is_ore = ore_h % 12 == 0
                || (((x as i16-12).abs() < 2) && ((y as i16-10).abs() < 2) && ore_h % 2 == 0)
                || (((x as i16-4).abs() < 2) && ((y as i16-14).abs() < 2) && ore_h % 3 == 0)
                || (((x as i16-18).abs() < 2) && ((y as i16-20).abs() < 2));
            if is_ore {
                set_pixel(data, x, y, (100i16+(h%20) as i16-10).clamp(0,255) as u8, (220i16+(h%16) as i16-8).clamp(0,255) as u8, (240i16+(h%12) as i16-6).clamp(0,255) as u8, 255);
            } else {
                let r = base.clamp(0,255) as u8;
                let g = base.clamp(0,255) as u8;
                let b = (base+5).clamp(0,255) as u8;
                if h % 13 == 0 { set_pixel(data, x, y, r.saturating_sub(30), g.saturating_sub(30), b.saturating_sub(25), 255); }
                else { set_pixel(data, x, y, r, g, b, 255); }
            }
        }
    }
}

fn generate_gravel(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1500);
            let v = (h % 40) as i16 - 20;
            let (r, g, b) = match h % 5 {
                0 => ((95i16+v/2).clamp(0,255) as u8, (90i16+v/2).clamp(0,255) as u8, (85i16+v/2).clamp(0,255) as u8),
                1 => ((155i16+v/2).clamp(0,255) as u8, (150i16+v/2).clamp(0,255) as u8, (145i16+v/2).clamp(0,255) as u8),
                2 => ((130i16+v/2).clamp(0,255) as u8, (110i16+v/2).clamp(0,255) as u8, (90i16+v/2).clamp(0,255) as u8),
                _ => ((120i16+v/2).clamp(0,255) as u8, (115i16+v/2).clamp(0,255) as u8, (110i16+v/2).clamp(0,255) as u8),
            };
            set_pixel(data, x, y, r, g, b, 255);
        }
    }
}

fn generate_cactus_side(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1600);
            let v = (h % 20) as i16 - 10;
            let ridge = (x % 6 < 2) || (x % 6 == 2 && h % 3 == 0);
            let (r, g, b) = if ridge {
                ((30i16+v/2).clamp(0,255) as u8, (100i16+v).clamp(0,255) as u8, (25i16+v/3).clamp(0,255) as u8)
            } else {
                ((50i16+v/2).clamp(0,255) as u8, (140i16+v).clamp(0,255) as u8, (40i16+v/3).clamp(0,255) as u8)
            };
            if h % 23 == 0 { set_pixel(data, x, y, 200, 200, 150, 255); }
            else { set_pixel(data, x, y, r, g, b, 255); }
        }
    }
}

fn generate_cactus_top(data: &mut [u8]) {
    let cx = TEXTURE_SIZE as f32 / 2.0;
    let cy = TEXTURE_SIZE as f32 / 2.0;
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 1700);
            let v = (h % 16) as i16 - 8;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx*dx + dy*dy).sqrt();
            let (r, g, b) = if dist < 4.0 {
                ((25i16+v/2).clamp(0,255) as u8, (80i16+v).clamp(0,255) as u8, (20i16+v/3).clamp(0,255) as u8)
            } else if dist < 10.0 {
                ((45i16+v/2).clamp(0,255) as u8, (130i16+v).clamp(0,255) as u8, (35i16+v/3).clamp(0,255) as u8)
            } else {
                ((55i16+v/2).clamp(0,255) as u8, (145i16+v).clamp(0,255) as u8, (42i16+v/3).clamp(0,255) as u8)
            };
            set_pixel(data, x, y, r, g, b, 255);
        }
    }
}

fn generate_tall_grass(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE { for x in 0..TEXTURE_SIZE { set_pixel(data, x, y, 0, 0, 0, 0); } }
    let blade_positions: [u32; 8] = [2, 5, 8, 10, 13, 16, 19, 22];
    for &bx in &blade_positions {
        let bh = hash(bx, 0, 1800);
        let blade_height = (TEXTURE_SIZE / 2) + (bh % (TEXTURE_SIZE / 3));
        let start_y = TEXTURE_SIZE - blade_height;
        for y in start_y..TEXTURE_SIZE {
            let h = hash(bx, y, 1801);
            let v = (h % 20) as i16 - 10;
            let sway = if y < start_y + 4 { 1i32 } else { 0 };
            let px = (bx as i32 + sway).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
            let gi = 100i16 + ((TEXTURE_SIZE - y) as i16 * 3) + v;
            set_pixel(data, px, y, (30i16+v/2).clamp(0,255) as u8, gi.clamp(0,255) as u8, (20i16+v/3).clamp(0,255) as u8, 200);
        }
    }
}

fn generate_flower(data: &mut [u8]) {
    let cx = TEXTURE_SIZE / 2;
    let cy = TEXTURE_SIZE / 2;
    for y in 0..TEXTURE_SIZE { for x in 0..TEXTURE_SIZE { set_pixel(data, x, y, 0, 0, 0, 0); } }
    for y in (cy+4)..TEXTURE_SIZE {
        let h = hash(cx, y, 1900);
        let v = (h % 10) as i16 - 5;
        set_pixel(data, cx, y, (30i16+v).clamp(0,255) as u8, (120i16+v).clamp(0,255) as u8, (25i16+v).clamp(0,255) as u8, 255);
    }
    for dy in -4i32..=4 {
        for dx in -4i32..=4 {
            let dist = ((dx*dx + dy*dy) as f32).sqrt();
            if dist < 4.5 && dist > 1.5 {
                let px = (cx as i32 + dx).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
                let py = (cy as i32 + dy).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
                let h = hash(px, py, 1901);
                let v = (h % 30) as i16 - 15;
                set_pixel(data, px, py, (220i16+v).clamp(0,255) as u8, (60i16+v/2).clamp(0,255) as u8, (80i16+v).clamp(0,255) as u8, 240);
            }
        }
    }
    for dy in -1i32..=1 { for dx in -1i32..=1 {
        set_pixel(data, (cx as i32+dx) as u32, (cy as i32+dy) as u32, 240, 220, 50, 255);
    }}
}

fn generate_mushroom(data: &mut [u8]) {
    let cx = TEXTURE_SIZE / 2;
    for y in 0..TEXTURE_SIZE { for x in 0..TEXTURE_SIZE { set_pixel(data, x, y, 0, 0, 0, 0); } }
    let stem_top = TEXTURE_SIZE / 2 + 2;
    for y in stem_top..TEXTURE_SIZE {
        for dx in -2i32..=2 {
            let px = (cx as i32 + dx).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
            let h = hash(px, y, 2000);
            let v = (h % 12) as i16 - 6;
            set_pixel(data, px, y, (220i16+v).clamp(0,255) as u8, (210i16+v).clamp(0,255) as u8, (195i16+v).clamp(0,255) as u8, 255);
        }
    }
    let cap_cy = TEXTURE_SIZE / 2;
    for dy in -5i32..=2 {
        for dx in -6i32..=6 {
            let dist = ((dx*dx + dy*dy) as f32).sqrt();
            let cap_r = 6.0 - (dy as f32 * 0.5).max(0.0);
            if dist < cap_r {
                let px = (cx as i32+dx).clamp(0, TEXTURE_SIZE as i32-1) as u32;
                let py = (cap_cy as i32+dy).clamp(0, TEXTURE_SIZE as i32-1) as u32;
                let h = hash(px, py, 2001);
                let v = (h % 20) as i16 - 10;
                if h % 11 == 0 { set_pixel(data, px, py, 240, 235, 230, 255); }
                else { set_pixel(data, px, py, (180i16+v).clamp(0,255) as u8, (50i16+v/2).clamp(0,255) as u8, (40i16+v/3).clamp(0,255) as u8, 255); }
            }
        }
    }
}

fn generate_crafting_table_top(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2100);
            let v = (h % 16) as i16 - 8;
            let on_grid = (x % 8 == 0) || (y % 8 == 0);
            if on_grid { set_pixel(data, x, y, (80i16+v).clamp(0,255) as u8, (55i16+v/2).clamp(0,255) as u8, (25i16+v/3).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (160i16+v).clamp(0,255) as u8, (120i16+v/2).clamp(0,255) as u8, (60i16+v/3).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_crafting_table_side(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2200);
            let v = (h % 16) as i16 - 8;
            let plank_line = y % 6 == 0;
            if plank_line { set_pixel(data, x, y, (70i16+v).clamp(0,255) as u8, (50i16+v/2).clamp(0,255) as u8, (25i16+v/3).clamp(0,255) as u8, 255); }
            else {
                let in_tool = x > 8 && x < 16 && y > 6 && y < 18;
                if in_tool && (x+y) % 3 == 0 { set_pixel(data, x, y, (100i16+v).clamp(0,255) as u8, (75i16+v/2).clamp(0,255) as u8, (40i16+v/3).clamp(0,255) as u8, 255); }
                else { set_pixel(data, x, y, (140i16+v).clamp(0,255) as u8, (100i16+v/2).clamp(0,255) as u8, (50i16+v/3).clamp(0,255) as u8, 255); }
            }
        }
    }
}

fn generate_furnace_side(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2300);
            let v = (h % 20) as i16 - 10;
            let in_opening = x > 7 && x < 17 && y > 10 && y < 20;
            if in_opening {
                let inner = x > 9 && x < 15 && y > 12 && y < 18;
                if inner { set_pixel(data, x, y, (20i16+v/2).clamp(0,255) as u8, (15i16+v/3).clamp(0,255) as u8, (10i16+v/4).clamp(0,255) as u8, 255); }
                else { set_pixel(data, x, y, (50i16+v/2).clamp(0,255) as u8, (45i16+v/2).clamp(0,255) as u8, (40i16+v/2).clamp(0,255) as u8, 255); }
            } else {
                set_pixel(data, x, y, (115i16+v).clamp(0,255) as u8, (115i16+v).clamp(0,255) as u8, (115i16+v).clamp(0,255) as u8, 255);
            }
        }
    }
}

fn generate_furnace_top(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2400);
            let v = (h % 20) as i16 - 10;
            set_pixel(data, x, y, (120i16+v).clamp(0,255) as u8, (120i16+v).clamp(0,255) as u8, (120i16+v).clamp(0,255) as u8, 255);
        }
    }
}

fn generate_chest(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2500);
            let v = (h % 16) as i16 - 8;
            let is_latch = x > 10 && x < 14 && y > 8 && y < 14;
            let is_band = y == 11 || y == 12;
            if is_latch { set_pixel(data, x, y, (180i16+v).clamp(0,255) as u8, (160i16+v).clamp(0,255) as u8, (40i16+v).clamp(0,255) as u8, 255); }
            else if is_band { set_pixel(data, x, y, (90i16+v).clamp(0,255) as u8, (70i16+v/2).clamp(0,255) as u8, (30i16+v/3).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (140i16+v).clamp(0,255) as u8, (95i16+v/2).clamp(0,255) as u8, (40i16+v/3).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_torch(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE { for x in 0..TEXTURE_SIZE { set_pixel(data, x, y, 0, 0, 0, 0); } }
    let cx = TEXTURE_SIZE / 2;
    for y in 8..TEXTURE_SIZE {
        for dx in -1i32..=1 {
            let px = (cx as i32+dx).clamp(0, TEXTURE_SIZE as i32-1) as u32;
            let h = hash(px, y, 2600);
            let v = (h % 10) as i16 - 5;
            set_pixel(data, px, y, (100i16+v).clamp(0,255) as u8, (70i16+v).clamp(0,255) as u8, (30i16+v).clamp(0,255) as u8, 255);
        }
    }
    for dy in 0i32..6 {
        for dx in -2i32..=2 {
            let dist = (dx.abs() as f32) + (dy as f32 * 0.5);
            if dist < 3.0 {
                let px = (cx as i32+dx).clamp(0, TEXTURE_SIZE as i32-1) as u32;
                let py = (7-dy).clamp(0, TEXTURE_SIZE as i32-1) as u32;
                let i = 1.0 - dist / 3.0;
                set_pixel(data, px, py, (255.0*i) as u8, (200.0*i) as u8, (50.0*i) as u8, (240.0*i) as u8);
            }
        }
    }
}

fn generate_ladder(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE { for x in 0..TEXTURE_SIZE { set_pixel(data, x, y, 0, 0, 0, 0); } }
    for y in 0..TEXTURE_SIZE {
        for &rx in &[5u32, 18u32] {
            let h = hash(rx, y, 2700);
            let v = (h % 10) as i16 - 5;
            set_pixel(data, rx, y, (100i16+v).clamp(0,255) as u8, (70i16+v).clamp(0,255) as u8, (35i16+v).clamp(0,255) as u8, 255);
            if rx + 1 < TEXTURE_SIZE { set_pixel(data, rx+1, y, (100i16+v).clamp(0,255) as u8, (70i16+v).clamp(0,255) as u8, (35i16+v).clamp(0,255) as u8, 255); }
        }
    }
    for &ry in &[3u32, 9u32, 15u32, 21u32] {
        for x in 6..19 {
            let h = hash(x, ry, 2701);
            let v = (h % 10) as i16 - 5;
            set_pixel(data, x, ry, (110i16+v).clamp(0,255) as u8, (80i16+v).clamp(0,255) as u8, (40i16+v).clamp(0,255) as u8, 255);
        }
    }
}

fn generate_plank(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2800);
            let v = (h % 20) as i16 - 10;
            let plank_line = y % 6 == 0;
            let nail = (x == 4 || x == 20) && (y % 6 == 3);
            if nail { set_pixel(data, x, y, 60, 60, 65, 255); }
            else if plank_line { set_pixel(data, x, y, (90i16+v).clamp(0,255) as u8, (65i16+v/2).clamp(0,255) as u8, (30i16+v/3).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (165i16+v).clamp(0,255) as u8, (125i16+v/2).clamp(0,255) as u8, (65i16+v/3).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_cobblestone(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 2900);
            let v = (h % 30) as i16 - 15;
            let mortar = ((x + (h % 3) as u32) % 8 < 1) || ((y + (h % 5) as u32) % 6 < 1);
            if mortar { set_pixel(data, x, y, (90i16+v/2).clamp(0,255) as u8, (85i16+v/2).clamp(0,255) as u8, (80i16+v/2).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (130i16+v).clamp(0,255) as u8, (128i16+v).clamp(0,255) as u8, (125i16+v).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_iron_ingot(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 3000);
            let v = (h % 16) as i16 - 8;
            let highlight = (x + y) % 12 < 2;
            if highlight { set_pixel(data, x, y, (210i16+v).clamp(0,255) as u8, (210i16+v).clamp(0,255) as u8, (215i16+v).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (180i16+v).clamp(0,255) as u8, (180i16+v).clamp(0,255) as u8, (185i16+v).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_gold_ingot(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 3100);
            let v = (h % 16) as i16 - 8;
            let highlight = (x + y) % 10 < 2;
            if highlight { set_pixel(data, x, y, (250i16+v).clamp(0,255) as u8, (220i16+v).clamp(0,255) as u8, (80i16+v).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (220i16+v).clamp(0,255) as u8, (180i16+v).clamp(0,255) as u8, (40i16+v).clamp(0,255) as u8, 255); }
        }
    }
}

fn generate_door(data: &mut [u8]) {
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let h = hash(x, y, 3200);
            let v = (h % 16) as i16 - 8;
            let border = x < 2 || x >= 22 || y < 2 || y >= 22;
            let panel_top = x > 4 && x < 20 && y > 3 && y < 10;
            let panel_bot = x > 4 && x < 20 && y > 13 && y < 20;
            let handle = x > 16 && x < 19 && y > 10 && y < 14;
            if handle { set_pixel(data, x, y, (160i16+v).clamp(0,255) as u8, (140i16+v).clamp(0,255) as u8, (30i16+v).clamp(0,255) as u8, 255); }
            else if border { set_pixel(data, x, y, (90i16+v).clamp(0,255) as u8, (60i16+v/2).clamp(0,255) as u8, (25i16+v/3).clamp(0,255) as u8, 255); }
            else if panel_top || panel_bot { set_pixel(data, x, y, (120i16+v).clamp(0,255) as u8, (80i16+v/2).clamp(0,255) as u8, (35i16+v/3).clamp(0,255) as u8, 255); }
            else { set_pixel(data, x, y, (105i16+v).clamp(0,255) as u8, (70i16+v/2).clamp(0,255) as u8, (30i16+v/3).clamp(0,255) as u8, 255); }
        }
    }
}

/// Holds the GPU texture array and sampler
pub struct TextureAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl TextureAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture_data = generate_texture_data();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture Atlas Array"),
            size: wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: TEXTURE_COUNT },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes_per_row = TEXTURE_SIZE * 4;
        let layer_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        for layer in 0..TEXTURE_COUNT {
            let offset = layer as usize * layer_size;
            queue.write_texture(
                wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::All },
                &texture_data[offset..offset + layer_size],
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(TEXTURE_SIZE) },
                wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: 1 },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Texture Atlas View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self { texture, view, sampler }
    }

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_atlas_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2Array, sample_type: wgpu::TextureSampleType::Float { filterable: true } },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    pub fn bind_group(&self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_atlas_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::VOXEL_TYPE_COUNT;

    /// Issue 4 safety net: every VoxelType / FaceDir combination must map to a
    /// texture layer within the GPU array's bounds. An out-of-range index would
    /// silently sample a black/garbage layer (the "untextured / black block"
    /// failure class).
    #[test]
    fn every_texture_index_is_in_range() {
        for raw in 0u8..(VOXEL_TYPE_COUNT as u8) {
            let vt = VoxelType::from(raw);
            for face in [FaceDir::Top, FaceDir::Bottom, FaceDir::Side] {
                let idx = texture_index(vt, face);
                assert!(
                    idx < TEXTURE_COUNT,
                    "texture_index({:?}, {:?}) = {} >= TEXTURE_COUNT ({})",
                    vt, face, idx, TEXTURE_COUNT
                );
            }
        }
    }

    /// The procedurally generated atlas data must contain exactly TEXTURE_COUNT
    /// fully-written layers (no missing / extra layers). This guards against a
    /// drift between TEXTURE_COUNT and the `generate_*` call list.
    #[test]
    fn generated_layer_count_matches_texture_count() {
        let data = generate_texture_data();
        let layer_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        assert_eq!(
            data.len(),
            layer_size * TEXTURE_COUNT as usize,
            "generated atlas byte length should equal layer_size * TEXTURE_COUNT"
        );
        // Number of complete layers actually present.
        assert_eq!(data.len() / layer_size, TEXTURE_COUNT as usize);
    }

    /// All TEX_* constants used by texture_index must address valid layers.
    /// Per-face texture mapping for multi-texture blocks. Grass must map to
    /// three DISTINCT, in-range layers: Top=grass_top, Side=grass_side,
    /// Bottom=dirt. This is the direct regression guard for the "grass sides
    /// show the wrong / a blank layer" failure.
    #[test]
    fn grass_per_face_textures_are_correct_and_distinct() {
        let top = texture_index(VoxelType::Grass, FaceDir::Top);
        let side = texture_index(VoxelType::Grass, FaceDir::Side);
        let bottom = texture_index(VoxelType::Grass, FaceDir::Bottom);

        assert_eq!(top, TEX_GRASS_TOP, "grass top must be grass_top layer");
        assert_eq!(side, TEX_GRASS_SIDE, "grass side must be grass_side layer");
        assert_eq!(bottom, TEX_DIRT, "grass bottom must be dirt layer");

        // Three distinct layers.
        assert_ne!(top, side);
        assert_ne!(top, bottom);
        assert_ne!(side, bottom);

        // All in range.
        for idx in [top, side, bottom] {
            assert!(idx < TEXTURE_COUNT);
        }
    }

    /// Wood: top/bottom share the rings (wood_top) layer; the four sides use the
    /// bark (wood_side) layer.
    #[test]
    fn wood_per_face_textures_are_correct() {
        let top = texture_index(VoxelType::Wood, FaceDir::Top);
        let side = texture_index(VoxelType::Wood, FaceDir::Side);
        let bottom = texture_index(VoxelType::Wood, FaceDir::Bottom);

        assert_eq!(top, TEX_WOOD_TOP, "wood top must be wood_top (rings) layer");
        assert_eq!(bottom, TEX_WOOD_TOP, "wood bottom must be wood_top (rings) layer");
        assert_eq!(side, TEX_WOOD_SIDE, "wood side must be wood_side (bark) layer");
        assert_ne!(top, side, "wood top (rings) and side (bark) must differ");

        for idx in [top, side, bottom] {
            assert!(idx < TEXTURE_COUNT);
        }
    }

    /// Snow is a single-texture block: all six faces use the snow layer.
    #[test]
    fn snow_per_face_textures_are_correct() {
        let top = texture_index(VoxelType::Snow, FaceDir::Top);
        let side = texture_index(VoxelType::Snow, FaceDir::Side);
        let bottom = texture_index(VoxelType::Snow, FaceDir::Bottom);

        assert_eq!(top, TEX_SNOW);
        assert_eq!(side, TEX_SNOW);
        assert_eq!(bottom, TEX_SNOW);
        for idx in [top, side, bottom] {
            assert!(idx < TEXTURE_COUNT);
        }
    }

    /// Crafting table and furnace are also multi-texture; verify their side
    /// faces map to the dedicated side layer (not a blank/wrong layer).
    #[test]
    fn other_multitexture_blocks_have_correct_side_layers() {
        assert_eq!(texture_index(VoxelType::CraftingTable, FaceDir::Top), TEX_CRAFTING_TABLE_TOP);
        assert_eq!(texture_index(VoxelType::CraftingTable, FaceDir::Side), TEX_CRAFTING_TABLE_SIDE);
        assert_eq!(texture_index(VoxelType::CraftingTable, FaceDir::Bottom), TEX_PLANK);
        assert_eq!(texture_index(VoxelType::Furnace, FaceDir::Top), TEX_FURNACE_TOP);
        assert_eq!(texture_index(VoxelType::Furnace, FaceDir::Side), TEX_FURNACE_SIDE);
    }

    /// CORE FIX GUARD: every OPAQUE (non-water, non-foliage/cutout) texture layer
    /// must be fully opaque (every texel alpha == 255) after
    /// `generate_texture_data()`. If any solid-block layer carried a sub-255
    /// alpha texel, the shader's alpha-cutout `discard` would drop it and the
    /// solid face would render see-through (the exact grass-side bug).
    #[test]
    fn opaque_layers_are_fully_opaque() {
        let data = generate_texture_data();
        let layer_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        for layer in 0..TEXTURE_COUNT {
            if layer_keeps_alpha(layer) {
                continue;
            }
            let offset = layer as usize * layer_size;
            for px in 0..(TEXTURE_SIZE * TEXTURE_SIZE) as usize {
                let a = data[offset + px * 4 + 3];
                assert_eq!(
                    a, 255,
                    "opaque layer {} texel {} has alpha {} (must be 255 so the cutout never discards a solid face)",
                    layer, px, a
                );
            }
        }
    }

    /// The alpha-tested foliage / translucent layers must STILL contain
    /// transparency (we must not have accidentally forced them opaque, which
    /// would turn cross-plant billboards into solid boxes).
    #[test]
    fn foliage_and_water_layers_keep_transparency() {
        let data = generate_texture_data();
        let layer_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        // These foliage/cutout layers have no PNG override, so their procedural
        // transparency must survive (they are excluded from the opaque-alpha
        // force). They must still contain transparent texels (holes).
        for layer in [TEX_TALL_GRASS, TEX_FLOWER, TEX_MUSHROOM, TEX_TORCH, TEX_LADDER] {
            let offset = layer as usize * layer_size;
            let mut min_a = 255u8;
            for px in 0..(TEXTURE_SIZE * TEXTURE_SIZE) as usize {
                min_a = min_a.min(data[offset + px * 4 + 3]);
            }
            assert!(min_a < 255, "alpha-tested layer {} should retain transparent texels", layer);
        }
    }

    #[test]
    fn all_tex_constants_in_range() {
        let constants = [
            TEX_GRASS_TOP, TEX_GRASS_SIDE, TEX_DIRT, TEX_STONE, TEX_SAND, TEX_WATER,
            TEX_WOOD_SIDE, TEX_WOOD_TOP, TEX_LEAVES, TEX_SNOW, TEX_BEDROCK, TEX_IRON_ORE,
            TEX_GOLD_ORE, TEX_DIAMOND_ORE, TEX_GRAVEL, TEX_CACTUS_SIDE, TEX_CACTUS_TOP,
            TEX_TALL_GRASS, TEX_FLOWER, TEX_MUSHROOM, TEX_CRAFTING_TABLE_TOP,
            TEX_CRAFTING_TABLE_SIDE, TEX_FURNACE_SIDE, TEX_FURNACE_TOP, TEX_CHEST,
            TEX_TORCH, TEX_LADDER, TEX_PLANK, TEX_COBBLESTONE, TEX_IRON_INGOT,
            TEX_GOLD_INGOT, TEX_DOOR,
        ];
        for c in constants {
            assert!(c < TEXTURE_COUNT, "TEX constant {} >= TEXTURE_COUNT ({})", c, TEXTURE_COUNT);
        }
    }
}
