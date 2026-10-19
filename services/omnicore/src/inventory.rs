//! Item stacks, data-component patches and container tracking.

use crate::data::GAME;
use crate::protocol::{Nbt, Reader, Writer};
use anyhow::{bail, Result};
use serde::Serialize;

/// Data component types in 26.3 registry order (index == network id).
pub const COMPONENTS: [&str; 122] = [
    "custom_data", "max_stack_size", "max_damage", "damage", "unbreakable", "use_effects", "custom_name",
    "minimum_attack_charge", "damage_type", "item_name", "item_model", "lore", "rarity", "enchantments",
    "can_place_on", "can_break", "attribute_modifiers", "custom_model_data", "tooltip_display", "repair_cost",
    "creative_slot_lock", "enchantment_glint_override", "intangible_projectile", "food", "consumable",
    "use_remainder", "use_cooldown", "damage_resistant", "tool", "weapon", "attack_range", "enchantable",
    "equippable", "repairable", "glider", "tooltip_style", "death_protection", "blocks_attacks", "piercing_weapon",
    "kinetic_weapon", "attack_animation", "interact_animation", "additional_trade_cost", "block_transformer",
    "villager_food", "stored_enchantments", "dye", "dyed_color", "map_id", "map_decorations", "map_post_processing",
    "charged_projectiles", "bundle_contents", "potion_contents", "potion_duration_scale", "suspicious_stew_effects",
    "writable_book_content", "written_book_content", "trim", "debug_stick_state", "entity_data",
    "bucket_entity_data", "block_entity_data", "instrument", "provides_trim_material", "ominous_bottle_amplifier",
    "jukebox_playable", "provides_banner_patterns", "recipes", "lodestone_tracker", "firework_explosion",
    "fireworks", "profile", "note_block_sound", "banner_patterns", "base_color", "pot_decorations", "container",
    "block_state", "bees", "sulfur_cube_content", "lock", "container_loot", "break_sound", "compostable",
    "cooking_fuel", "brewing_fuel", "mob_visibility", "villager/variant", "wolf/variant", "wolf/sound_variant",
    "wolf/collar", "fox/variant", "salmon/size", "parrot/variant", "tropical_fish/pattern",
    "tropical_fish/base_color", "tropical_fish/pattern_color", "mooshroom/variant", "rabbit/variant", "pig/variant",
    "pig/sound_variant", "cow/variant", "cow/sound_variant", "chicken/variant", "chicken/sound_variant",
    "zombie_nautilus/variant", "frog/variant", "horse/variant", "painting/variant", "llama/variant",
    "axolotl/variant", "cat/variant", "cat/sound_variant", "cat/collar", "sheep/color", "shulker/color",
    "provides_pottery_pattern", "sign_text_front", "sign_text_back", "waxed", "cushion/color",
];

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct ItemStack {
    pub id: u32,
    pub count: i32,
    pub damage: i32,
    pub enchantments: Vec<(i32, i32)>,
    pub custom_name: Option<String>,
    /// True when the patch contained a component OmniCore cannot decode yet;
    /// the item identity/count are still correct.
    pub partial: bool,
}

impl ItemStack {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.count <= 0 || self.id == 0
    }
    pub fn name(&self) -> &'static str {
        if self.is_empty() {
            "air"
        } else {
            GAME.item_name(self.id)
        }
    }
    pub fn max_stack(&self) -> i32 {
        GAME.item(self.id).map(|i| i.stack as i32).unwrap_or(64)
    }
}

fn skip_nbt(r: &mut Reader) -> Result<()> {
    Nbt::read_network(r).map(|_| ())
}

/// Decodes a single component value. Returns Err for component types whose
/// layout is not implemented (the caller marks the stack partial).
fn read_component(r: &mut Reader, name: &str, st: &mut ItemStack) -> Result<()> {
    match name {
        "custom_data" | "bucket_entity_data" => skip_nbt(r)?,
        "max_stack_size" | "max_damage" | "repair_cost" | "enchantable" | "map_id" | "ominous_bottle_amplifier"
        | "additional_trade_cost" | "rarity" | "dye" | "base_color" | "wolf/collar" | "cat/collar"
        | "sheep/color" | "shulker/color" | "tropical_fish/base_color" | "tropical_fish/pattern_color"
        | "cushion/color" | "map_post_processing" | "fox/variant" | "salmon/size" | "parrot/variant"
        | "tropical_fish/pattern" | "mooshroom/variant" | "rabbit/variant" | "pig/variant" | "cow/variant"
        | "chicken/variant" | "frog/variant" | "horse/variant" | "llama/variant" | "axolotl/variant"
        | "cat/variant" | "wolf/variant" | "villager/variant" | "zombie_nautilus/variant" => {
            r.varint()?;
        }
        "damage" => st.damage = r.varint()?,
        "unbreakable" | "creative_slot_lock" | "glider" | "intangible_projectile" | "waxed" => {}
        "custom_name" => st.custom_name = Some(crate::protocol::text::nbt_to_plain(&Nbt::read_network(r)?)),
        "item_name" => skip_nbt(r)?,
        "item_model" | "tooltip_style" | "note_block_sound" => {
            r.string()?;
        }
        "minimum_attack_charge" | "potion_duration_scale" => {
            r.f32()?;
        }
        "lore" => {
            let n = r.varint()?;
            for _ in 0..n {
                skip_nbt(r)?;
            }
        }
        "enchantments" | "stored_enchantments" => {
            let n = r.varint()?;
            for _ in 0..n {
                let id = r.varint()?;
                let lvl = r.varint()?;
                if name == "enchantments" {
                    st.enchantments.push((id, lvl));
                }
            }
        }
        "enchantment_glint_override" => {
            r.bool()?;
        }
        "dyed_color" => {
            r.i32()?;
        }
        "recipes" => {
            let n = r.varint()?;
            for _ in 0..n {
                r.string()?;
            }
        }
        "charged_projectiles" | "bundle_contents" | "container" => {
            let n = r.varint()?;
            for _ in 0..n {
                read_template(r)?;
            }
        }
        "use_remainder" | "sulfur_cube_content" => {
            read_template(r)?;
        }
        other => bail!("component {other} not decodable"),
    }
    Ok(())
}

/// ItemStackTemplate: item id, count, component patch.
fn read_template(r: &mut Reader) -> Result<ItemStack> {
    let id = r.varint()? as u32;
    let count = r.varint()?;
    let mut st = ItemStack { id, count, ..Default::default() };
    read_patch(r, &mut st)?;
    Ok(st)
}

fn read_patch(r: &mut Reader, st: &mut ItemStack) -> Result<()> {
    let added = r.varint()?;
    let removed = r.varint()?;
    for _ in 0..added {
        let ty = r.varint()? as usize;
        let name = COMPONENTS.get(ty).copied().unwrap_or("?");
        read_component(r, name, st)?;
    }
    for _ in 0..removed {
        r.varint()?;
    }
    Ok(())
}

/// Reads a network ItemStack (count, id, component patch).
pub fn read_slot(r: &mut Reader) -> Result<ItemStack> {
    let count = r.varint()?;
    if count <= 0 {
        return Ok(ItemStack::empty());
    }
    let id = r.varint()? as u32;
    let mut st = ItemStack { id, count, ..Default::default() };
    read_patch(r, &mut st)?;
    Ok(st)
}

/// Writes a "hashed stack" for container clicks. OmniCore sends the item and
/// count without component hashes; the server treats mismatches as a resync,
/// which is always safe because the server remains authoritative.
pub fn write_hashed(w: &mut Writer, st: Option<&ItemStack>) {
    match st {
        Some(s) if !s.is_empty() => {
            w.bool(true).varint(s.id as i32).varint(s.count).varint(0).varint(0);
        }
        _ => {
            w.bool(false);
        }
    }
}

/// Player inventory window (id 0) layout.
pub mod slots {
    pub const CRAFT_RESULT: usize = 0;
    pub const CRAFT_GRID: [usize; 4] = [1, 2, 3, 4];
    pub const ARMOR: [usize; 4] = [5, 6, 7, 8];
    pub const MAIN_START: usize = 9;
    pub const HOTBAR_START: usize = 36;
    pub const OFFHAND: usize = 45;
    pub const TOTAL: usize = 46;
}

#[derive(Clone, Debug, Serialize)]
pub struct Window {
    pub id: i32,
    pub kind: i32,
    pub title: String,
    pub slots: Vec<ItemStack>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Inventory {
    /// Window 0 slots (see `slots`).
    pub slots: Vec<ItemStack>,
    pub selected_hotbar: u8,
    pub cursor: ItemStack,
    pub state_id: i32,
    pub open: Option<Window>,
    pub desynced: bool,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: vec![ItemStack::empty(); slots::TOTAL],
            selected_hotbar: 0,
            cursor: ItemStack::empty(),
            state_id: 0,
            open: None,
            desynced: false,
        }
    }
}

/// Maps the "player inventory index" used by Set Player Inventory Slot to window-0 slots.
pub fn inventory_index_to_slot(idx: usize) -> Option<usize> {
    Some(match idx {
        0..=8 => slots::HOTBAR_START + idx,
        9..=35 => idx,
        36..=39 => 8 - (idx - 36),
        40 => slots::OFFHAND,
        _ => return None,
    })
}

impl Inventory {
    pub fn held(&self) -> &ItemStack {
        &self.slots[slots::HOTBAR_START + self.selected_hotbar as usize]
    }

    pub fn count(&self, name: &str) -> i32 {
        self.slots[slots::MAIN_START..slots::OFFHAND].iter().filter(|s| s.name() == name).map(|s| s.count).sum()
    }

    pub fn count_where(&self, f: impl Fn(&str) -> bool) -> i32 {
        self.slots[slots::MAIN_START..slots::OFFHAND].iter().filter(|s| !s.is_empty() && f(s.name())).map(|s| s.count).sum()
    }

    /// Window-0 slot of the first stack matching the predicate (hotbar first).
    pub fn find(&self, f: impl Fn(&ItemStack) -> bool) -> Option<usize> {
        (slots::HOTBAR_START..slots::OFFHAND)
            .chain(slots::MAIN_START..slots::HOTBAR_START)
            .find(|i| !self.slots[*i].is_empty() && f(&self.slots[*i]))
    }

    pub fn first_empty_hotbar(&self) -> Option<usize> {
        (0..9).find(|i| self.slots[slots::HOTBAR_START + i].is_empty())
    }

    pub fn summary(&self) -> Vec<(String, i32)> {
        let mut m: std::collections::BTreeMap<String, i32> = Default::default();
        for s in &self.slots[slots::ARMOR[0]..] {
            if !s.is_empty() {
                *m.entry(s.name().to_string()).or_default() += s.count;
            }
        }
        m.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_damaged_enchanted_pickaxe() {
        let pick = GAME.item_by_name("iron_pickaxe").unwrap().id;
        let mut w = Writer::new();
        w.varint(1).varint(pick as i32);
        w.varint(2).varint(0); // two added, zero removed
        w.varint(3).varint(17); // damage = 17
        w.varint(13).varint(1).varint(5).varint(3); // enchantments {5: 3}
        let st = read_slot(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(st.name(), "iron_pickaxe");
        assert_eq!(st.damage, 17);
        assert_eq!(st.enchantments, vec![(5, 3)]);
    }

    #[test]
    fn empty_slot() {
        assert!(read_slot(&mut Reader::new(&[0])).unwrap().is_empty());
    }

    #[test]
    fn index_mapping() {
        assert_eq!(inventory_index_to_slot(0), Some(36));
        assert_eq!(inventory_index_to_slot(39), Some(5));
        assert_eq!(inventory_index_to_slot(40), Some(45));
    }
}
