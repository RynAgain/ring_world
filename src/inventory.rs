/// Inventory & Items system for the ring world
/// Manages item stacks, hotbar, and inventory operations

use crate::voxel::VoxelType;

/// Maximum number of items in a single stack (default for most items)
pub const MAX_STACK_SIZE: u32 = 64;

/// Total number of inventory slots (slots 0-8 are the hotbar)
pub const INVENTORY_SIZE: usize = 36;

/// Number of hotbar slots
pub const HOTBAR_SIZE: usize = 9;

/// A stack of items of the same type
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemStack {
    pub item_type: VoxelType,
    pub count: u32,
}

impl ItemStack {
    /// Create a new item stack
    pub fn new(item_type: VoxelType, count: u32) -> Self {
        let max = item_type.max_stack_size();
        Self {
            item_type,
            count: count.min(max),
        }
    }

    /// Check if this stack is full
    pub fn is_full(&self) -> bool {
        self.count >= self.item_type.max_stack_size()
    }

    /// How many more items can fit in this stack
    pub fn space_remaining(&self) -> u32 {
        self.item_type.max_stack_size() - self.count
    }
}

/// Player inventory with 36 slots (first 9 are the hotbar)
pub struct Inventory {
    pub slots: [Option<ItemStack>; INVENTORY_SIZE],
}

impl Inventory {
    /// Create a new empty inventory
    pub fn new() -> Self {
        Self {
            slots: [None; INVENTORY_SIZE],
        }
    }

    /// Add items to the inventory. Returns the leftover count that didn't fit.
    /// First tries to stack with existing items of the same type, then fills empty slots.
    pub fn add_item(&mut self, item_type: VoxelType, mut count: u32) -> u32 {
        let max_stack = item_type.max_stack_size();

        // First pass: try to stack with existing items of the same type
        if max_stack > 1 {
            for slot in self.slots.iter_mut() {
                if count == 0 {
                    break;
                }
                if let Some(ref mut stack) = slot {
                    if stack.item_type == item_type && !stack.is_full() {
                        let can_add = stack.space_remaining().min(count);
                        stack.count += can_add;
                        count -= can_add;
                    }
                }
            }
        }

        // Second pass: fill empty slots
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let to_place = count.min(max_stack);
                *slot = Some(ItemStack::new(item_type, to_place));
                count -= to_place;
            }
        }

        // Return leftover that didn't fit
        count
    }

    /// Remove items from a specific slot. Returns the removed ItemStack if successful.
    pub fn remove_item(&mut self, slot: usize, count: u32) -> Option<ItemStack> {
        if slot >= INVENTORY_SIZE {
            return None;
        }

        if let Some(ref mut stack) = self.slots[slot] {
            if count >= stack.count {
                // Remove entire stack
                let removed = *stack;
                self.slots[slot] = None;
                Some(removed)
            } else {
                // Remove partial stack
                stack.count -= count;
                Some(ItemStack::new(stack.item_type, count))
            }
        } else {
            None
        }
    }

    /// Get a reference to the item stack in a slot
    pub fn get_slot(&self, slot: usize) -> Option<&ItemStack> {
        if slot >= INVENTORY_SIZE {
            return None;
        }
        self.slots[slot].as_ref()
    }

    /// Get a reference to a hotbar slot (index 0-8)
    pub fn get_hotbar_slot(&self, index: usize) -> Option<&ItemStack> {
        if index >= HOTBAR_SIZE {
            return None;
        }
        self.slots[index].as_ref()
    }

    /// Check if the inventory contains at least one item of the given type
    pub fn has_item(&self, item_type: VoxelType) -> bool {
        self.slots.iter().any(|slot| {
            slot.as_ref()
                .map(|stack| stack.item_type == item_type && stack.count > 0)
                .unwrap_or(false)
        })
    }

    /// Count total items of a given type across all slots
    pub fn count_item(&self, item_type: VoxelType) -> u32 {
        self.slots.iter().filter_map(|slot| {
            slot.as_ref().and_then(|stack| {
                if stack.item_type == item_type {
                    Some(stack.count)
                } else {
                    None
                }
            })
        }).sum()
    }

    /// Remove a specific number of items of a given type from anywhere in the inventory.
    /// Returns true if successful (all items were removed), false if not enough items.
    pub fn remove_items(&mut self, item_type: VoxelType, mut count: u32) -> bool {
        // First check if we have enough
        if self.count_item(item_type) < count {
            return false;
        }

        // Remove from slots (prefer non-hotbar first to preserve hotbar)
        for i in (0..INVENTORY_SIZE).rev() {
            if count == 0 {
                break;
            }
            if let Some(ref mut stack) = self.slots[i] {
                if stack.item_type == item_type {
                    let to_remove = stack.count.min(count);
                    stack.count -= to_remove;
                    count -= to_remove;
                    if stack.count == 0 {
                        self.slots[i] = None;
                    }
                }
            }
        }

        count == 0
    }

    /// Consume one item from a hotbar slot. Returns true if successful.
    pub fn consume_from_hotbar(&mut self, index: usize) -> bool {
        if index >= HOTBAR_SIZE {
            return false;
        }

        if let Some(ref mut stack) = self.slots[index] {
            if stack.count > 0 {
                stack.count -= 1;
                if stack.count == 0 {
                    self.slots[index] = None;
                }
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_inventory_is_empty() {
        let inv = Inventory::new();
        assert!(inv.slots.iter().all(|s| s.is_none()));
        assert_eq!(inv.count_item(VoxelType::Stone), 0);
    }

    #[test]
    fn add_item_adds_into_empty_slot() {
        let mut inv = Inventory::new();
        let leftover = inv.add_item(VoxelType::Stone, 10);
        assert_eq!(leftover, 0);
        assert_eq!(inv.count_item(VoxelType::Stone), 10);
    }

    #[test]
    fn add_item_stacks_existing() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Stone, 30);
        inv.add_item(VoxelType::Stone, 20);
        // Both should stack into a single slot of 50 (max 64)
        assert_eq!(inv.count_item(VoxelType::Stone), 50);
        let used_slots = inv.slots.iter().filter(|s| s.is_some()).count();
        assert_eq!(used_slots, 1);
    }

    #[test]
    fn add_item_spills_into_multiple_slots() {
        let mut inv = Inventory::new();
        // 100 stone -> a full stack of 64 plus 36 in another slot
        let leftover = inv.add_item(VoxelType::Stone, 100);
        assert_eq!(leftover, 0);
        assert_eq!(inv.count_item(VoxelType::Stone), 100);
        let used_slots = inv.slots.iter().filter(|s| s.is_some()).count();
        assert_eq!(used_slots, 2);
    }

    #[test]
    fn add_item_returns_leftover_when_full() {
        let mut inv = Inventory::new();
        // Fill the entire inventory with Stone: 36 slots * 64 = 2304
        let total_capacity = (INVENTORY_SIZE as u32) * MAX_STACK_SIZE;
        let leftover = inv.add_item(VoxelType::Stone, total_capacity + 50);
        assert_eq!(leftover, 50);
        assert_eq!(inv.count_item(VoxelType::Stone), total_capacity);
    }

    #[test]
    fn add_item_tools_use_separate_slots() {
        let mut inv = Inventory::new();
        // Tools are not stackable (max 1), so adding 3 picks uses 3 slots
        inv.add_item(VoxelType::WoodPickaxe, 3);
        assert_eq!(inv.count_item(VoxelType::WoodPickaxe), 3);
        let used_slots = inv.slots.iter().filter(|s| {
            s.as_ref().map(|st| st.item_type == VoxelType::WoodPickaxe).unwrap_or(false)
        }).count();
        assert_eq!(used_slots, 3);
    }

    #[test]
    fn remove_item_removes_partial() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Dirt, 20);
        let removed = inv.remove_item(0, 5).expect("should remove");
        assert_eq!(removed.count, 5);
        assert_eq!(inv.count_item(VoxelType::Dirt), 15);
    }

    #[test]
    fn remove_item_removes_full_stack() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Dirt, 20);
        let removed = inv.remove_item(0, 100).expect("should remove entire stack");
        assert_eq!(removed.count, 20);
        assert!(inv.get_slot(0).is_none());
    }

    #[test]
    fn remove_item_out_of_bounds_returns_none() {
        let mut inv = Inventory::new();
        assert!(inv.remove_item(INVENTORY_SIZE, 1).is_none());
    }

    #[test]
    fn count_item_across_slots() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Stone, 64);
        inv.add_item(VoxelType::Stone, 64);
        inv.add_item(VoxelType::Stone, 10);
        assert_eq!(inv.count_item(VoxelType::Stone), 138);
    }

    #[test]
    fn has_item_works() {
        let mut inv = Inventory::new();
        assert!(!inv.has_item(VoxelType::Wood));
        inv.add_item(VoxelType::Wood, 1);
        assert!(inv.has_item(VoxelType::Wood));
    }

    #[test]
    fn remove_items_removes_across_slots() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Stone, 128);
        assert!(inv.remove_items(VoxelType::Stone, 100));
        assert_eq!(inv.count_item(VoxelType::Stone), 28);
    }

    #[test]
    fn remove_items_fails_when_insufficient() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Stone, 5);
        assert!(!inv.remove_items(VoxelType::Stone, 10));
        // Inventory unchanged on failure
        assert_eq!(inv.count_item(VoxelType::Stone), 5);
    }

    #[test]
    fn consume_from_hotbar_decrements() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Dirt, 3);
        assert!(inv.consume_from_hotbar(0));
        assert_eq!(inv.count_item(VoxelType::Dirt), 2);
    }

    #[test]
    fn consume_from_hotbar_clears_empty_slot() {
        let mut inv = Inventory::new();
        inv.add_item(VoxelType::Dirt, 1);
        assert!(inv.consume_from_hotbar(0));
        assert!(inv.get_slot(0).is_none());
        // Consuming an empty slot fails
        assert!(!inv.consume_from_hotbar(0));
    }

    #[test]
    fn consume_from_hotbar_out_of_range_fails() {
        let mut inv = Inventory::new();
        assert!(!inv.consume_from_hotbar(HOTBAR_SIZE));
    }

    #[test]
    fn item_stack_clamps_to_max() {
        let stack = ItemStack::new(VoxelType::Stone, 1000);
        assert_eq!(stack.count, 64);
        let tool = ItemStack::new(VoxelType::WoodPickaxe, 10);
        assert_eq!(tool.count, 1);
    }
}
