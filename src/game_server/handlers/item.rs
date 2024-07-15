use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Error,
    path::Path,
};

use crate::game_server::packets::item::{EquipmentSlot, ItemDefinition};

pub fn load_item_definitions(config_dir: &Path) -> Result<BTreeMap<u32, ItemDefinition>, Error> {
    let mut file = File::open(config_dir.join("items.json"))?;
    let item_defs: Vec<ItemDefinition> = serde_json::from_reader(&mut file)?;

    let mut item_def_map = BTreeMap::new();
    for item_def in item_defs {
        if let Some(previous_item_def) = item_def_map.insert(item_def.guid, item_def) {
            panic!("Two item definitions have ID {}", previous_item_def.guid);
        }
    }
    Ok(item_def_map)
}

pub fn load_required_slots(config_dir: &Path) -> Result<BTreeSet<EquipmentSlot>, Error> {
    let mut file = File::open(config_dir.join("required_slots.json"))?;
    let slots: Vec<EquipmentSlot> = serde_json::from_reader(&mut file)?;
    Ok(BTreeSet::from_iter(slots))
}
