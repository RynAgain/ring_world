/// Voxel types and data for the ring world

use crate::block::BlockProperties;
use crate::texture;

/// Face direction for per-face coloring
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceDir {
    Top,    // +Y (toward sun)
    Bottom, // -Y (away from sun)
    Side,   // +X, -X, +Z, -Z
}

/// Types of blocks/voxels in the world
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VoxelType {
    Air = 0,
    Stone = 1,
    Dirt = 2,
    Grass = 3,
    Sand = 4,
    Water = 5,
    Wood = 6,
    Leaves = 7,
    Bedrock = 8,
    Snow = 9,
    IronOre = 10,
    GoldOre = 11,
    DiamondOre = 12,
    Gravel = 13,
    Cactus = 14,
    TallGrass = 15,
    Flower = 16,
    Mushroom = 17,
    Vine = 18,
    // v0.3 Crafting blocks
    CraftingTable = 19,
    Furnace = 20,
    Chest = 21,
    Torch = 22,
    Ladder = 23,
    Door = 24,
    Plank = 25,
    Cobblestone = 26,
    IronIngot = 27,
    GoldIngot = 28,
    // v0.3 Tool items (non-placeable, exist in inventory)
    WoodPickaxe = 29,
    WoodAxe = 30,
    WoodShovel = 31,
    WoodSword = 32,
    StonePickaxe = 33,
    StoneAxe = 34,
    StoneShovel = 35,
    StoneSword = 36,
    IronPickaxe = 37,
    IronAxe = 38,
    IronShovel = 39,
    IronSword = 40,
}

/// Total number of VoxelType variants
pub const VOXEL_TYPE_COUNT: usize = 41;

impl VoxelType {
    /// Whether this block is solid (blocks movement) - delegates to BlockProperties
    pub fn is_solid(&self) -> bool {
        BlockProperties::get(*self).is_solid
    }

    /// Whether this block is transparent (allows light/rendering through) - delegates to BlockProperties
    pub fn is_transparent(&self) -> bool {
        BlockProperties::get(*self).is_transparent
    }

    /// Whether this item is a tool (non-placeable)
    pub fn is_tool(&self) -> bool {
        matches!(self,
            VoxelType::WoodPickaxe | VoxelType::WoodAxe | VoxelType::WoodShovel | VoxelType::WoodSword |
            VoxelType::StonePickaxe | VoxelType::StoneAxe | VoxelType::StoneShovel | VoxelType::StoneSword |
            VoxelType::IronPickaxe | VoxelType::IronAxe | VoxelType::IronShovel | VoxelType::IronSword
        )
    }

    /// Whether this item is non-stackable (tools have max stack of 1)
    pub fn max_stack_size(&self) -> u32 {
        if self.is_tool() {
            1
        } else {
            64
        }
    }

    /// Get the color for this voxel type based on which face is being rendered
    /// Minecraft-style: grass has green top, dirt sides
    pub fn face_color(&self, face: FaceDir) -> [f32; 4] {
        match self {
            VoxelType::Air => [0.0, 0.0, 0.0, 0.0],
            VoxelType::Stone => [0.5, 0.5, 0.5, 1.0],
            VoxelType::Dirt => [0.55, 0.35, 0.18, 1.0],
            VoxelType::Grass => match face {
                FaceDir::Top => [0.3, 0.75, 0.2, 1.0],     // Green top
                FaceDir::Bottom => [0.55, 0.35, 0.18, 1.0], // Dirt bottom
                FaceDir::Side => [0.45, 0.35, 0.18, 1.0],   // Dirt sides (slightly different)
            },
            VoxelType::Sand => [0.85, 0.8, 0.55, 1.0],
            VoxelType::Water => [0.2, 0.4, 0.8, 0.7],
            VoxelType::Wood => match face {
                FaceDir::Top | FaceDir::Bottom => [0.5, 0.35, 0.15, 1.0], // Wood rings
                FaceDir::Side => [0.4, 0.25, 0.1, 1.0],                    // Bark
            },
            VoxelType::Leaves => [0.1, 0.5, 0.1, 0.9],
            VoxelType::Bedrock => [0.15, 0.15, 0.15, 1.0],
            VoxelType::Snow => [0.95, 0.95, 0.98, 1.0],
            VoxelType::IronOre => [0.55, 0.45, 0.4, 1.0],    // Gray with orange/brown tint
            VoxelType::GoldOre => [0.55, 0.52, 0.35, 1.0],   // Gray with yellow tint
            VoxelType::DiamondOre => [0.45, 0.5, 0.55, 1.0], // Gray with cyan/blue tint
            VoxelType::Gravel => [0.5, 0.47, 0.43, 1.0],     // Gray-brown speckled
            VoxelType::Cactus => [0.2, 0.6, 0.15, 0.95],     // Green, slightly transparent edges
            VoxelType::TallGrass => [0.25, 0.7, 0.2, 0.8],   // Green, non-solid decorative
            VoxelType::Flower => [0.9, 0.3, 0.4, 0.9],       // Colorful, non-solid
            VoxelType::Mushroom => [0.6, 0.3, 0.2, 0.9],     // Brown/red cap
            VoxelType::Vine => [0.15, 0.55, 0.1, 0.85],      // Green, hanging
            // v0.3 blocks
            VoxelType::CraftingTable => match face {
                FaceDir::Top => [0.6, 0.45, 0.2, 1.0],      // Grid pattern top
                FaceDir::Bottom => [0.5, 0.35, 0.15, 1.0],  // Wood bottom
                FaceDir::Side => [0.55, 0.4, 0.18, 1.0],    // Wood sides with tools
            },
            VoxelType::Furnace => match face {
                FaceDir::Top => [0.45, 0.45, 0.45, 1.0],    // Stone top
                FaceDir::Bottom => [0.45, 0.45, 0.45, 1.0], // Stone bottom
                FaceDir::Side => [0.4, 0.4, 0.4, 1.0],      // Dark opening on front
            },
            VoxelType::Chest => match face {
                FaceDir::Top => [0.55, 0.4, 0.15, 1.0],     // Wood top with latch
                FaceDir::Bottom => [0.5, 0.35, 0.12, 1.0],  // Wood bottom
                FaceDir::Side => [0.52, 0.38, 0.14, 1.0],   // Wood sides
            },
            VoxelType::Torch => [0.9, 0.7, 0.2, 0.9],       // Warm yellow glow
            VoxelType::Ladder => [0.5, 0.35, 0.15, 0.85],    // Wood ladder
            VoxelType::Door => [0.45, 0.3, 0.12, 1.0],       // Dark wood door
            VoxelType::Plank => [0.65, 0.5, 0.25, 1.0],      // Lighter brown processed wood
            VoxelType::Cobblestone => [0.42, 0.42, 0.42, 1.0], // Rough gray stone
            VoxelType::IronIngot => [0.75, 0.75, 0.78, 1.0],  // Shiny silver
            VoxelType::GoldIngot => [0.9, 0.75, 0.2, 1.0],    // Shiny gold
            // Tools - shouldn't normally be rendered as blocks, but provide a color anyway
            VoxelType::WoodPickaxe | VoxelType::WoodAxe | VoxelType::WoodShovel | VoxelType::WoodSword => {
                [0.6, 0.4, 0.15, 1.0] // Wood tool color
            }
            VoxelType::StonePickaxe | VoxelType::StoneAxe | VoxelType::StoneShovel | VoxelType::StoneSword => {
                [0.5, 0.5, 0.5, 1.0] // Stone tool color
            }
            VoxelType::IronPickaxe | VoxelType::IronAxe | VoxelType::IronShovel | VoxelType::IronSword => {
                [0.7, 0.7, 0.72, 1.0] // Iron tool color
            }
        }
    }

    /// Get the default color (for backward compat)
    pub fn color(&self) -> [f32; 4] {
        self.face_color(FaceDir::Top)
    }

    /// Get the texture index for this voxel type and face direction
    pub fn texture_index(&self, face: FaceDir) -> u32 {
        texture::texture_index(*self, face)
    }
}

impl From<u8> for VoxelType {
    fn from(val: u8) -> Self {
        match val {
            0 => VoxelType::Air,
            1 => VoxelType::Stone,
            2 => VoxelType::Dirt,
            3 => VoxelType::Grass,
            4 => VoxelType::Sand,
            5 => VoxelType::Water,
            6 => VoxelType::Wood,
            7 => VoxelType::Leaves,
            8 => VoxelType::Bedrock,
            9 => VoxelType::Snow,
            10 => VoxelType::IronOre,
            11 => VoxelType::GoldOre,
            12 => VoxelType::DiamondOre,
            13 => VoxelType::Gravel,
            14 => VoxelType::Cactus,
            15 => VoxelType::TallGrass,
            16 => VoxelType::Flower,
            17 => VoxelType::Mushroom,
            18 => VoxelType::Vine,
            19 => VoxelType::CraftingTable,
            20 => VoxelType::Furnace,
            21 => VoxelType::Chest,
            22 => VoxelType::Torch,
            23 => VoxelType::Ladder,
            24 => VoxelType::Door,
            25 => VoxelType::Plank,
            26 => VoxelType::Cobblestone,
            27 => VoxelType::IronIngot,
            28 => VoxelType::GoldIngot,
            29 => VoxelType::WoodPickaxe,
            30 => VoxelType::WoodAxe,
            31 => VoxelType::WoodShovel,
            32 => VoxelType::WoodSword,
            33 => VoxelType::StonePickaxe,
            34 => VoxelType::StoneAxe,
            35 => VoxelType::StoneShovel,
            36 => VoxelType::StoneSword,
            37 => VoxelType::IronPickaxe,
            38 => VoxelType::IronAxe,
            39 => VoxelType::IronShovel,
            40 => VoxelType::IronSword,
            _ => VoxelType::Air,
        }
    }
}

/// A single voxel with its type and metadata
#[derive(Clone, Copy, Debug)]
pub struct Voxel {
    pub voxel_type: VoxelType,
}

impl Voxel {
    pub fn new(voxel_type: VoxelType) -> Self {
        Self { voxel_type }
    }

    pub fn air() -> Self {
        Self {
            voxel_type: VoxelType::Air,
        }
    }

    pub fn is_air(&self) -> bool {
        self.voxel_type == VoxelType::Air
    }
}

impl Default for Voxel {
    fn default() -> Self {
        Self::air()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_solid_for_various_types() {
        assert!(!VoxelType::Air.is_solid());
        assert!(VoxelType::Stone.is_solid());
        assert!(VoxelType::Dirt.is_solid());
        assert!(!VoxelType::Water.is_solid());
    }

    #[test]
    fn is_transparent_for_various_types() {
        assert!(VoxelType::Air.is_transparent());
        assert!(!VoxelType::Stone.is_transparent());
        assert!(VoxelType::Water.is_transparent());
        assert!(VoxelType::Leaves.is_transparent());
    }

    #[test]
    fn from_u8_round_trips_for_all_variants() {
        for raw in 0u8..(VOXEL_TYPE_COUNT as u8) {
            let vt = VoxelType::from(raw);
            assert_eq!(vt as u8, raw, "round-trip failed for {}", raw);
        }
    }

    #[test]
    fn from_u8_out_of_range_is_air() {
        assert_eq!(VoxelType::from(255), VoxelType::Air);
        assert_eq!(VoxelType::from(VOXEL_TYPE_COUNT as u8), VoxelType::Air);
    }

    #[test]
    fn max_stack_size_tools_are_one() {
        assert_eq!(VoxelType::WoodPickaxe.max_stack_size(), 1);
        assert_eq!(VoxelType::IronSword.max_stack_size(), 1);
        assert_eq!(VoxelType::StoneShovel.max_stack_size(), 1);
    }

    #[test]
    fn max_stack_size_blocks_are_64() {
        assert_eq!(VoxelType::Stone.max_stack_size(), 64);
        assert_eq!(VoxelType::Dirt.max_stack_size(), 64);
        assert_eq!(VoxelType::Plank.max_stack_size(), 64);
    }

    #[test]
    fn is_tool_identifies_tools() {
        assert!(VoxelType::WoodPickaxe.is_tool());
        assert!(VoxelType::StoneAxe.is_tool());
        assert!(VoxelType::IronSword.is_tool());
        assert!(!VoxelType::Stone.is_tool());
        assert!(!VoxelType::Air.is_tool());
        assert!(!VoxelType::Plank.is_tool());
    }

    #[test]
    fn face_color_components_in_range() {
        for raw in 0u8..(VOXEL_TYPE_COUNT as u8) {
            let vt = VoxelType::from(raw);
            for face in [FaceDir::Top, FaceDir::Bottom, FaceDir::Side] {
                let c = vt.face_color(face);
                for comp in c.iter() {
                    assert!(*comp >= 0.0 && *comp <= 1.0, "component out of range for {:?}: {}", vt, comp);
                }
            }
        }
    }

    #[test]
    fn grass_has_green_top_and_dirt_sides() {
        let top = VoxelType::Grass.face_color(FaceDir::Top);
        let side = VoxelType::Grass.face_color(FaceDir::Side);
        // Green channel of top should be the dominant one
        assert!(top[1] > top[0] && top[1] > top[2]);
        // Sides differ from the top
        assert_ne!(top, side);
    }

    #[test]
    fn voxel_air_helpers() {
        assert!(Voxel::air().is_air());
        assert!(!Voxel::new(VoxelType::Stone).is_air());
        assert!(Voxel::default().is_air());
    }
}
