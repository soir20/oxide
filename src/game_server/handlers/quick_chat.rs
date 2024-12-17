use std::{collections::BTreeMap, fs::File, io::Error, path::Path};

use serde::Deserialize;

use crate::game_server::packets::quick_chat::{Data, SendData};

#[derive(Deserialize)]
pub struct QuickChatConfig {
    id: i32,
    parent_id: i32,
    menu_text: i32,
    menu_icon_id: i32,
    animation_id: i32,
    item_id: i32,
}

pub fn load_quick_chats(config_dir: &Path) -> Result<BTreeMap<i32, QuickChatConfig>, Error> {
    let mut file = File::open(config_dir.join("quick_chats.json"))?;
    let quick_chats: Vec<QuickChatConfig> = serde_json::from_reader(&mut file)?;

    let mut quick_chat_table = BTreeMap::new();
    for quick_chat in quick_chats {
        let id = quick_chat.id;
        let previous = quick_chat_table.insert(id, quick_chat);

        if previous.is_some() {
            panic!("Two quick chats have ID {}", id);
        }
    }

    Ok(quick_chat_table)
}

pub fn make_test_quick_chats(quick_chats: &BTreeMap<i32, QuickChatConfig>) -> SendData {
    let mut owned_quick_chats = Vec::new();
    for data in quick_chats.values() {
        owned_quick_chats.push(Data {
            id1: data.id,
            id2: data.id,
            menu_text: data.menu_text,
            chat_text: 0,
            animation_id: data.animation_id,
            unknown1: 0,
            admin_only: 0,
            menu_icon_id: data.menu_icon_id,
            item_id: data.item_id,
            parent_id: data.parent_id,
            unknown2: 0,
        })
    }
    SendData {
        data: owned_quick_chats,
    }
}
