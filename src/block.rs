/// Block properties system for the ring world
/// Defines comprehensive properties for each block type

use crate::voxel::{VoxelType, VOXEL_TYPE_COUNT};

/// Tool speed multiplier when using the correct tool
const CORRECT_TOOL_MULTIPLIER: f32 = 0.5;
/// Speed multiplier when using no tool (bare hands)
const NO_TOOL_MULTIPLIER: f32 = 1.5;
/// Base speed multiplier (default, used when tool system isn't active yet)
const BASE_MULTIPLIER: f32 = 1.0;

/// What tool type is most effective for breaking this block
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolType {
    None,
    Pickaxe,
    Axe,
    Shovel,
    Sword,
}

/// Comprehensive properties for a block type
#[derive(Clone, Copy, Debug)]
pub struct BlockProperties {
    /// Time to break in seconds (0.0 = instant, -1.0 = unbreakable)
    pub hardness: f32,
    /// Whether this block blocks movement
    pub is_solid: bool,
    /// Whether light/rendering passes through
    pub is_transparent: bool,
    /// Whether this block has liquid behavior
    pub is_liquid: bool,
    /// What tool is most effective
    pub tool_type: ToolType,
    /// What this block drops when broken (None = drops itself)
    pub drop: Option<VoxelType>,
    /// Light emission level (0-15, for future lighting)
    pub light_level: u8,
    /// Whether this block is affected by gravity (falls like sand)
    pub gravity_affected: bool,
    /// Whether this block can catch fire
    pub flammable: bool,
}

impl BlockProperties {
    /// Get the properties for a given voxel type using a const lookup
    pub fn get(voxel_type: VoxelType) -> &'static BlockProperties {
        let idx = voxel_type as usize;
        if idx < VOXEL_TYPE_COUNT {
            &BLOCK_PROPERTIES[idx]
        } else {
            &BLOCK_PROPERTIES[0] // fallback to Air
        }
    }
}

/// Calculate the time to break a block given its hardness and tool status.
///
/// # Arguments
/// * `block_hardness` - The hardness value of the block (from BlockProperties)
/// * `has_correct_tool` - Whether the player is holding the correct tool type
///
/// # Returns
/// The time in seconds to break the block. Returns f32::INFINITY for unbreakable blocks.
pub fn get_break_time(block_hardness: f32, has_correct_tool: bool) -> f32 {
    if block_hardness < 0.0 {
        // Unbreakable
        return f32::INFINITY;
    }
    if block_hardness == 0.0 {
        // Instant break
        return 0.0;
    }

    let multiplier = if has_correct_tool {
        CORRECT_TOOL_MULTIPLIER
    } else {
        NO_TOOL_MULTIPLIER
    };

    block_hardness * multiplier
}

/// Get the break time using the base multiplier (no tool system active yet).
/// This is the default used until tools are implemented.
pub fn get_base_break_time(block_hardness: f32) -> f32 {
    if block_hardness < 0.0 {
        return f32::INFINITY;
    }
    if block_hardness == 0.0 {
        return 0.0;
    }
    block_hardness * BASE_MULTIPLIER
}

/// Static lookup table for block properties indexed by VoxelType discriminant
static BLOCK_PROPERTIES: [BlockProperties; VOXEL_TYPE_COUNT] = [
    // Air (0)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Stone (1)
    BlockProperties {
        hardness: 1.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: Some(VoxelType::Cobblestone),
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Dirt (2)
    BlockProperties {
        hardness: 0.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Shovel,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Grass (3)
    BlockProperties {
        hardness: 0.6,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Shovel,
        drop: Some(VoxelType::Dirt),
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Sand (4)
    BlockProperties {
        hardness: 0.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Shovel,
        drop: None,
        light_level: 0,
        gravity_affected: true,
        flammable: false,
    },
    // Water (5)
    BlockProperties {
        hardness: -1.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: true,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Wood (6)
    BlockProperties {
        hardness: 2.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Leaves (7)
    BlockProperties {
        hardness: 0.2,
        is_solid: true,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Bedrock (8)
    BlockProperties {
        hardness: -1.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Snow (9)
    BlockProperties {
        hardness: 0.1,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Shovel,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronOre (10)
    BlockProperties {
        hardness: 3.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // GoldOre (11)
    BlockProperties {
        hardness: 3.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // DiamondOre (12)
    BlockProperties {
        hardness: 5.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Gravel (13)
    BlockProperties {
        hardness: 0.6,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Shovel,
        drop: None,
        light_level: 0,
        gravity_affected: true,
        flammable: false,
    },
    // Cactus (14)
    BlockProperties {
        hardness: 0.4,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // TallGrass (15)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Flower (16)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Mushroom (17)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Vine (18)
    BlockProperties {
        hardness: 0.2,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // CraftingTable (19)
    BlockProperties {
        hardness: 2.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Furnace (20)
    BlockProperties {
        hardness: 3.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // Chest (21)
    BlockProperties {
        hardness: 2.5,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Torch (22)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 14,
        gravity_affected: false,
        flammable: false,
    },
    // Ladder (23)
    BlockProperties {
        hardness: 0.4,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Door (24)
    BlockProperties {
        hardness: 3.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Plank (25)
    BlockProperties {
        hardness: 2.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Axe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // Cobblestone (26)
    BlockProperties {
        hardness: 2.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronIngot (27) - stored as a block
    BlockProperties {
        hardness: 5.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // GoldIngot (28) - stored as a block
    BlockProperties {
        hardness: 3.0,
        is_solid: true,
        is_transparent: false,
        is_liquid: false,
        tool_type: ToolType::Pickaxe,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // WoodPickaxe (29) - tool item, not a real block
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // WoodAxe (30)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // WoodShovel (31)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // WoodSword (32)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: true,
    },
    // StonePickaxe (33)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // StoneAxe (34)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // StoneShovel (35)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // StoneSword (36)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronPickaxe (37)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronAxe (38)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronShovel (39)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
    // IronSword (40)
    BlockProperties {
        hardness: 0.0,
        is_solid: false,
        is_transparent: true,
        is_liquid: false,
        tool_type: ToolType::None,
        drop: None,
        light_level: 0,
        gravity_affected: false,
        flammable: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_lookup_air() {
        let props = BlockProperties::get(VoxelType::Air);
        assert!(!props.is_solid);
        assert!(props.is_transparent);
        assert_eq!(props.tool_type, ToolType::None);
    }

    #[test]
    fn stone_is_solid_and_pickaxe() {
        let props = BlockProperties::get(VoxelType::Stone);
        assert!(props.is_solid);
        assert!(!props.is_transparent);
        assert_eq!(props.tool_type, ToolType::Pickaxe);
        assert_eq!(props.drop, Some(VoxelType::Cobblestone));
    }

    #[test]
    fn bedrock_and_water_unbreakable() {
        let bedrock = BlockProperties::get(VoxelType::Bedrock);
        let water = BlockProperties::get(VoxelType::Water);
        assert!(bedrock.hardness < 0.0);
        assert!(water.hardness < 0.0);
    }

    #[test]
    fn water_is_not_solid_but_liquid() {
        let props = BlockProperties::get(VoxelType::Water);
        assert!(!props.is_solid);
        assert!(props.is_liquid);
        assert!(props.is_transparent);
    }

    #[test]
    fn torch_emits_light() {
        let props = BlockProperties::get(VoxelType::Torch);
        assert!(props.light_level > 0);
    }

    #[test]
    fn sand_and_gravel_gravity_affected() {
        assert!(BlockProperties::get(VoxelType::Sand).gravity_affected);
        assert!(BlockProperties::get(VoxelType::Gravel).gravity_affected);
        assert!(!BlockProperties::get(VoxelType::Stone).gravity_affected);
    }

    #[test]
    fn properties_for_every_voxel_type_no_panic() {
        // Iterate over every discriminant and make sure get() returns without
        // panicking and stays within the array.
        for raw in 0u8..(VOXEL_TYPE_COUNT as u8) {
            let vt = VoxelType::from(raw);
            let _props = BlockProperties::get(vt);
        }
    }

    #[test]
    fn block_properties_array_has_full_length() {
        assert_eq!(BLOCK_PROPERTIES.len(), VOXEL_TYPE_COUNT);
    }

    #[test]
    fn get_break_time_unbreakable_is_infinite() {
        assert_eq!(get_break_time(-1.0, false), f32::INFINITY);
        assert_eq!(get_break_time(-1.0, true), f32::INFINITY);
    }

    #[test]
    fn get_break_time_instant_is_zero() {
        assert_eq!(get_break_time(0.0, false), 0.0);
        assert_eq!(get_break_time(0.0, true), 0.0);
    }

    #[test]
    fn get_break_time_correct_tool_is_faster() {
        let with_tool = get_break_time(2.0, true);
        let without_tool = get_break_time(2.0, false);
        assert!(with_tool < without_tool);
        assert_eq!(with_tool, 2.0 * CORRECT_TOOL_MULTIPLIER);
        assert_eq!(without_tool, 2.0 * NO_TOOL_MULTIPLIER);
    }

    #[test]
    fn get_base_break_time_values() {
        assert_eq!(get_base_break_time(-1.0), f32::INFINITY);
        assert_eq!(get_base_break_time(0.0), 0.0);
        assert_eq!(get_base_break_time(1.5), 1.5 * BASE_MULTIPLIER);
    }
}
