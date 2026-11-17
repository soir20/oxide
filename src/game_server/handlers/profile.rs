use std::collections::{BTreeMap, BTreeSet};

use crate::game_server::packets::{
    item::EquipmentSlot,
    player_data::{EquippedItem, PlayerData},
};

use super::character::Player;

/// A player's savable data, as plain values.  This is all the persistence
/// layer ever sees; equipment slots are stored as their numeric ids.
#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub guid: u32,
    pub first_name: String,
    pub last_name: String,
    pub credits: u32,
    pub active_battle_class: u32,
    pub owned_items: BTreeSet<i32>,
    /// battle class guid -> (slot id -> item guid)
    pub equipped: BTreeMap<u32, BTreeMap<u32, i32>>,
}

impl PlayerSnapshot {
    pub fn from_player(guid: u32, player: &Player) -> Self {
        PlayerSnapshot {
            guid,
            first_name: player.name.first_name.clone(),
            last_name: player.name.last_name.clone(),
            credits: player.credits,
            active_battle_class: player.inventory.active_battle_class,
            owned_items: player.inventory.owned_items().clone(),
            equipped: player
                .inventory
                .battle_classes()
                .iter()
                .map(|(battle_class_guid, battle_class)| {
                    (
                        *battle_class_guid,
                        battle_class
                            .items
                            .iter()
                            .map(|(slot, item_guid)| (u32::from(*slot), *item_guid))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    /// Applies the saved data to the Player packet before it's sent at login.
    pub fn apply_to(&self, data: &mut PlayerData) {
        data.name.first_name = self.first_name.clone();
        data.name.last_name = self.last_name.clone();
        data.credits = self.credits;
        data.inventory
            .retain(|item_guid, _| self.owned_items.contains(item_guid));
        if data.battle_classes.contains_key(&self.active_battle_class) {
            data.active_battle_class = self.active_battle_class;
        }
        for (battle_class_guid, battle_class) in data.battle_classes.iter_mut() {
            if let Some(items) = self.equipped.get(battle_class_guid) {
                battle_class.items = items
                    .iter()
                    .filter(|(_, item_guid)| self.owned_items.contains(item_guid))
                    .filter_map(|(slot, item_guid)| {
                        let slot = EquipmentSlot::try_from(*slot).ok()?;
                        Some((
                            slot,
                            EquippedItem {
                                slot,
                                guid: *item_guid,
                                category: 0,
                            },
                        ))
                    })
                    .collect();
            }
        }
    }
}

/// First login (nothing saved yet): own only the default gear the test player
/// wears, instead of every item.
pub fn start_new_player(data: &mut PlayerData) {
    let starter: BTreeSet<i32> = data
        .battle_classes
        .values()
        .flat_map(|battle_class| battle_class.items.values().map(|item| item.guid))
        .collect();
    data.inventory
        .retain(|item_guid, _| starter.contains(item_guid));
}
