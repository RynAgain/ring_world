/// Crafting system for the ring world
/// Implements recipes, crafting table, furnace smelting, and tool creation

use crate::block::ToolType;
use crate::inventory::Inventory;
use crate::voxel::VoxelType;

/// Type of recipe
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeType {
    /// Basic crafting (no table needed, 2x2 grid equivalent)
    Basic,
    /// Requires a crafting table nearby (3x3 grid equivalent)
    CraftingTable,
    /// Furnace smelting (1 input + fuel -> 1 output)
    Smelting,
}

/// A crafting recipe
#[derive(Clone, Debug)]
pub struct Recipe {
    pub recipe_type: RecipeType,
    pub ingredients: Vec<(VoxelType, u32)>,
    pub result: VoxelType,
    pub result_count: u32,
}

/// Manages all recipes and crafting operations
pub struct CraftingManager {
    pub recipes: Vec<Recipe>,
}

impl CraftingManager {
    /// Create a new CraftingManager with all recipes registered
    pub fn new() -> Self {
        let mut recipes = Vec::new();

        // === Basic Crafting (no table needed) ===
        recipes.push(Recipe {
            recipe_type: RecipeType::Basic,
            ingredients: vec![(VoxelType::Wood, 1)],
            result: VoxelType::Plank,
            result_count: 4,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::Basic,
            ingredients: vec![(VoxelType::Plank, 4)],
            result: VoxelType::CraftingTable,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::Basic,
            ingredients: vec![(VoxelType::Wood, 1), (VoxelType::Stone, 1)],
            result: VoxelType::Torch,
            result_count: 4,
        });

        // === Crafting Table Recipes ===
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Cobblestone, 8)],
            result: VoxelType::Furnace,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 8)],
            result: VoxelType::Chest,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 7)],
            result: VoxelType::Ladder,
            result_count: 3,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 6)],
            result: VoxelType::Door,
            result_count: 1,
        });

        // Wood tools
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 3), (VoxelType::Wood, 2)],
            result: VoxelType::WoodPickaxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 3), (VoxelType::Wood, 2)],
            result: VoxelType::WoodAxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 1), (VoxelType::Wood, 2)],
            result: VoxelType::WoodShovel,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Plank, 2), (VoxelType::Wood, 1)],
            result: VoxelType::WoodSword,
            result_count: 1,
        });

        // Stone tools
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Cobblestone, 3), (VoxelType::Wood, 2)],
            result: VoxelType::StonePickaxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Cobblestone, 3), (VoxelType::Wood, 2)],
            result: VoxelType::StoneAxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Cobblestone, 1), (VoxelType::Wood, 2)],
            result: VoxelType::StoneShovel,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::Cobblestone, 2), (VoxelType::Wood, 1)],
            result: VoxelType::StoneSword,
            result_count: 1,
        });

        // Iron tools
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::IronIngot, 3), (VoxelType::Wood, 2)],
            result: VoxelType::IronPickaxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::IronIngot, 3), (VoxelType::Wood, 2)],
            result: VoxelType::IronAxe,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::IronIngot, 1), (VoxelType::Wood, 2)],
            result: VoxelType::IronShovel,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::CraftingTable,
            ingredients: vec![(VoxelType::IronIngot, 2), (VoxelType::Wood, 1)],
            result: VoxelType::IronSword,
            result_count: 1,
        });

        // === Smelting Recipes ===
        recipes.push(Recipe {
            recipe_type: RecipeType::Smelting,
            ingredients: vec![(VoxelType::IronOre, 1)],
            result: VoxelType::IronIngot,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::Smelting,
            ingredients: vec![(VoxelType::GoldOre, 1)],
            result: VoxelType::GoldIngot,
            result_count: 1,
        });
        recipes.push(Recipe {
            recipe_type: RecipeType::Smelting,
            ingredients: vec![(VoxelType::Stone, 1)],
            result: VoxelType::Cobblestone,
            result_count: 1,
        });

        Self { recipes }
    }

    /// Check if a recipe can be crafted with the current inventory
    pub fn can_craft(&self, recipe: &Recipe, inventory: &Inventory) -> bool {
        for (item_type, count) in &recipe.ingredients {
            if inventory.count_item(*item_type) < *count {
                return false;
            }
        }
        true
    }

    /// Craft a recipe by index. Consumes ingredients and adds result to inventory.
    /// Returns true if crafting was successful.
    pub fn craft(&self, recipe_index: usize, inventory: &mut Inventory) -> bool {
        if recipe_index >= self.recipes.len() {
            return false;
        }

        let recipe = &self.recipes[recipe_index];

        // Check if we can craft
        if !self.can_craft(recipe, inventory) {
            return false;
        }

        // Consume ingredients
        for (item_type, count) in &recipe.ingredients {
            if !inventory.remove_items(*item_type, *count) {
                // This shouldn't happen since we checked can_craft, but be safe
                return false;
            }
        }

        // Add result to inventory
        let leftover = inventory.add_item(recipe.result, recipe.result_count);
        if leftover > 0 {
            // Inventory full - items are lost (could be improved later)
            // In a real game you'd drop them on the ground
        }

        true
    }

    /// Attempt to smelt using the furnace. Requires the input item and fuel (Wood or Plank).
    /// Returns true if smelting was successful.
    pub fn smelt(&self, input_type: VoxelType, inventory: &mut Inventory) -> bool {
        // Find matching smelting recipe
        let recipe_idx = self.recipes.iter().position(|r| {
            r.recipe_type == RecipeType::Smelting
                && r.ingredients.len() == 1
                && r.ingredients[0].0 == input_type
        });

        let recipe_idx = match recipe_idx {
            Some(idx) => idx,
            None => return false,
        };

        let recipe = &self.recipes[recipe_idx];

        // Check if player has the input
        if inventory.count_item(input_type) < 1 {
            return false;
        }

        // Check if player has fuel (Wood or Plank)
        let has_wood_fuel = inventory.count_item(VoxelType::Wood) >= 1;
        let has_plank_fuel = inventory.count_item(VoxelType::Plank) >= 1;

        if !has_wood_fuel && !has_plank_fuel {
            return false;
        }

        // Consume input
        if !inventory.remove_items(input_type, 1) {
            return false;
        }

        // Consume fuel (prefer plank over wood logs)
        if has_plank_fuel {
            inventory.remove_items(VoxelType::Plank, 1);
        } else {
            inventory.remove_items(VoxelType::Wood, 1);
        }

        // Add result
        inventory.add_item(recipe.result, recipe.result_count);

        true
    }

    /// Get indices of all recipes that can currently be crafted
    /// `has_crafting_table` - whether a crafting table is nearby
    pub fn get_available_recipes(&self, inventory: &Inventory, has_crafting_table: bool) -> Vec<usize> {
        self.recipes.iter().enumerate().filter_map(|(idx, recipe)| {
            // Filter by recipe type availability
            match recipe.recipe_type {
                RecipeType::Basic => {},
                RecipeType::CraftingTable => {
                    if !has_crafting_table {
                        return None;
                    }
                },
                RecipeType::Smelting => {
                    // Smelting recipes are handled separately via smelt()
                    return None;
                },
            }

            // Check if we have the ingredients
            if self.can_craft(recipe, inventory) {
                Some(idx)
            } else {
                None
            }
        }).collect()
    }

    /// Get the tool type that a VoxelType item provides when held
    pub fn get_tool_type(item: VoxelType) -> Option<ToolType> {
        match item {
            VoxelType::WoodPickaxe | VoxelType::StonePickaxe | VoxelType::IronPickaxe => Some(ToolType::Pickaxe),
            VoxelType::WoodAxe | VoxelType::StoneAxe | VoxelType::IronAxe => Some(ToolType::Axe),
            VoxelType::WoodShovel | VoxelType::StoneShovel | VoxelType::IronShovel => Some(ToolType::Shovel),
            VoxelType::WoodSword | VoxelType::StoneSword | VoxelType::IronSword => Some(ToolType::Sword),
            _ => None,
        }
    }

    /// Get the mining speed multiplier for a tool item
    /// Wood = 2x, Stone = 4x, Iron = 6x
    pub fn get_tool_multiplier(item: VoxelType) -> f32 {
        match item {
            VoxelType::WoodPickaxe | VoxelType::WoodAxe | VoxelType::WoodShovel | VoxelType::WoodSword => 2.0,
            VoxelType::StonePickaxe | VoxelType::StoneAxe | VoxelType::StoneShovel | VoxelType::StoneSword => 4.0,
            VoxelType::IronPickaxe | VoxelType::IronAxe | VoxelType::IronShovel | VoxelType::IronSword => 6.0,
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Find the index of the recipe producing the given result (first match).
    fn recipe_index_for(mgr: &CraftingManager, result: VoxelType) -> usize {
        mgr.recipes.iter().position(|r| r.result == result).expect("recipe exists")
    }

    #[test]
    fn new_creates_recipes() {
        let mgr = CraftingManager::new();
        assert!(!mgr.recipes.is_empty());
        // Should contain the basic Wood -> Plank recipe
        assert!(mgr.recipes.iter().any(|r| r.result == VoxelType::Plank));
    }

    #[test]
    fn can_craft_true_when_ingredients_present() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Wood, 1);
        let idx = recipe_index_for(&mgr, VoxelType::Plank);
        assert!(mgr.can_craft(&mgr.recipes[idx], &inv));
    }

    #[test]
    fn can_craft_false_when_ingredients_absent() {
        let mgr = CraftingManager::new();
        let inv = Inventory::new();
        let idx = recipe_index_for(&mgr, VoxelType::Plank);
        assert!(!mgr.can_craft(&mgr.recipes[idx], &inv));
    }

    #[test]
    fn craft_wood_to_planks_end_to_end() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Wood, 1);
        let idx = recipe_index_for(&mgr, VoxelType::Plank);

        assert!(mgr.craft(idx, &mut inv));
        // Wood consumed
        assert_eq!(inv.count_item(VoxelType::Wood), 0);
        // 4 planks produced
        assert_eq!(inv.count_item(VoxelType::Plank), 4);
    }

    #[test]
    fn craft_fails_without_ingredients() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        let idx = recipe_index_for(&mgr, VoxelType::Plank);
        assert!(!mgr.craft(idx, &mut inv));
    }

    #[test]
    fn craft_out_of_range_index_fails() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        assert!(!mgr.craft(usize::MAX, &mut inv));
    }

    #[test]
    fn craft_multi_ingredient_recipe() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Wood, 1);
        inv.add_item(VoxelType::Stone, 1);
        let idx = recipe_index_for(&mgr, VoxelType::Torch);
        assert!(mgr.craft(idx, &mut inv));
        assert_eq!(inv.count_item(VoxelType::Torch), 4);
        assert_eq!(inv.count_item(VoxelType::Wood), 0);
        assert_eq!(inv.count_item(VoxelType::Stone), 0);
    }

    #[test]
    fn smelt_iron_ore_to_ingot() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::IronOre, 1);
        inv.add_item(VoxelType::Wood, 1); // fuel
        assert!(mgr.smelt(VoxelType::IronOre, &mut inv));
        assert_eq!(inv.count_item(VoxelType::IronIngot), 1);
        assert_eq!(inv.count_item(VoxelType::IronOre), 0);
        assert_eq!(inv.count_item(VoxelType::Wood), 0);
    }

    #[test]
    fn smelt_fails_without_fuel() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::IronOre, 1);
        assert!(!mgr.smelt(VoxelType::IronOre, &mut inv));
    }

    #[test]
    fn get_available_recipes_respects_crafting_table() {
        let mgr = CraftingManager::new();
        let mut inv = Inventory::new();
        // 8 planks can make a chest (requires table) and other basic recipes
        inv.add_item(VoxelType::Plank, 8);

        let without_table = mgr.get_available_recipes(&inv, false);
        let with_table = mgr.get_available_recipes(&inv, true);
        // Table recipes are only available when a crafting table is present
        assert!(with_table.len() >= without_table.len());
        // Chest is a table recipe and should not appear without a table
        let chest_idx = recipe_index_for(&mgr, VoxelType::Chest);
        assert!(!without_table.contains(&chest_idx));
        assert!(with_table.contains(&chest_idx));
    }

    #[test]
    fn get_tool_type_correct() {
        assert_eq!(CraftingManager::get_tool_type(VoxelType::WoodPickaxe), Some(ToolType::Pickaxe));
        assert_eq!(CraftingManager::get_tool_type(VoxelType::StoneAxe), Some(ToolType::Axe));
        assert_eq!(CraftingManager::get_tool_type(VoxelType::IronShovel), Some(ToolType::Shovel));
        assert_eq!(CraftingManager::get_tool_type(VoxelType::WoodSword), Some(ToolType::Sword));
        assert_eq!(CraftingManager::get_tool_type(VoxelType::Stone), None);
    }

    #[test]
    fn get_tool_multiplier_correct() {
        assert_eq!(CraftingManager::get_tool_multiplier(VoxelType::WoodPickaxe), 2.0);
        assert_eq!(CraftingManager::get_tool_multiplier(VoxelType::StonePickaxe), 4.0);
        assert_eq!(CraftingManager::get_tool_multiplier(VoxelType::IronPickaxe), 6.0);
        assert_eq!(CraftingManager::get_tool_multiplier(VoxelType::Stone), 1.0);
    }
}
