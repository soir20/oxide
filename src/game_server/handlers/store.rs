use std::{
    collections::BTreeMap,
    io::{Error, ErrorKind},
};

use evalexpr::{context_map, eval_with_context, Value};

use crate::{
    game_server::{
        handlers::item::ItemConfig,
        packets::{
            store::{RedirectCategories, RedirectItemDefinitions, StoreItem, StoreItemList},
            tunnel::TunneledPacket,
            GamePacket,
        },
    },
    ConfigError,
};

use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::{
        character::CharacterType, lock_enforcer::CharacterLockRequest, unique_guid::player_guid,
    },
    packets::{
        client_update::{AddItems, AddItemsData, UpdateCredits},
        item::{Item, MarketData},
        store::{BuyItemRequest, SellToClientResponse, StoreOpCode},
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

pub const REDIRECT_GUID: i32 = 0b01000000_00000000_00000000_00000000;
pub const REDIRECT_NAME_ID: u32 = 51791;
pub const REDIRECT_ICON_SET_ID: u32 = 1962;
pub const REDIRECT_MODEL_ID: u32 = 2376;
pub const REDIRECT_ANIMATION_ID: i32 = 9002;

pub struct CostEntry {
    pub base: u32,
    pub members: u32,
}

pub type ItemCostMap = BTreeMap<i32, CostEntry>;

pub fn compute_costs(items: &[ItemConfig]) -> Result<ItemCostMap, ConfigError> {
    let mut costs = BTreeMap::new();

    for item_config in items.iter() {
        let cost_entry = costs.entry(item_config.guid).or_insert_with(|| CostEntry {
            base: item_config.cost,
            members: item_config.cost,
        });

        cost_entry.base = item_config.cost;
        cost_entry.members = evaluate_cost_expression(
            &item_config.members_cost_expression,
            cost_entry.members,
            item_config.guid,
        )?;
    }

    Ok(costs)
}

impl From<&ItemCostMap> for StoreItemList {
    fn from(cost_map: &ItemCostMap) -> Self {
        StoreItemList {
            static_items: cost_map
                .iter()
                .map(|(item_guid, costs)| StoreItem {
                    guid: *item_guid,
                    unknown2: 0,
                    unknown3: 0,
                    unknown4: false,
                    unknown5: false,
                    unknown6: 0,
                    unknown7: false,
                    unknown8: false,
                    base_cost: costs.base,
                    unknown10: 0,
                    unknown11: 0,
                    unknown12: 0,
                    member_cost: costs.members,
                })
                .collect(),
            dynamic_items: vec![],
        }
    }
}

fn evaluate_cost_expression(
    cost_expression: &str,
    cost: u32,
    item_guid: i32,
) -> Result<u32, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(cost as f64),
    }
    .unwrap_or_else(|_| {
        panic!("Couldn't build expression evaluation context for item {item_guid}")
    });

    let result = eval_with_context(cost_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Unable to evaluate cost expression for item {item_guid}: {err}"),
        )
    })?;

    let Value::Float(new_cost) = result else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Cost expression did not return an integer for item {item_guid}, returned: {result}"
            ),
        ));
    };

    u32::try_from(new_cost.round() as i64).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Cost expression returned float that could not be converted to an integer for item {item_guid}: {new_cost}, {err}"
            ),
        )
    })
}

pub fn redirect_packets() -> Vec<Vec<u8>> {
    vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: RedirectCategories::new(),
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: RedirectItemDefinitions::new(REDIRECT_GUID),
        }),
    ]
}

fn next_transaction_id() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub fn process_store_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    match StoreOpCode::try_from(raw_op_code) {
        Ok(StoreOpCode::BuyItem) => {
            let request: BuyItemRequest = DeserializePacket::deserialize(cursor)?;
            buy_items(sender, &[request.item_guid], game_server)
        }
        _ => Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unimplemented store op code: {raw_op_code}"),
        )),
    }
}

/// "Buy Now" in the item-set view: buy every item in the group.
pub fn buy_item_group(
    sender: u32,
    group_guid: i32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let Some(group) = game_server
        .item_groups()
        .definitions
        .iter()
        .find(|group| group.guid == group_guid)
    else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("Player {sender} tried to buy unknown item group {group_guid}"),
        ));
    };
    let item_guids: Vec<i32> = group.items.iter().map(|item| item.guid).collect();
    buy_items(sender, &item_guids, game_server)
}

/// Charges the player for the items they don't own yet, adds them to the
/// inventory and tells the client.  Saved by the periodic profile save.
pub fn buy_items(
    sender: u32,
    item_guids: &[i32],
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: Vec::new(),
        write_guids: vec![player_guid(sender)],
        character_consumer: |_, _, mut characters_write, _| {
            let Some(character) = characters_write.get_mut(&player_guid(sender)) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Unknown player {sender} tried to buy items {item_guids:?}"),
                ));
            };
            let CharacterType::Player(player) = &mut character.stats.character_type else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Non-player {sender} tried to buy items {item_guids:?}"),
                ));
            };

            // Skip anything already owned (e.g. part of a set).
            let to_buy: Vec<i32> = item_guids
                .iter()
                .copied()
                .filter(|item_guid| !player.inventory.owns_item(*item_guid))
                .collect();
            if to_buy.is_empty() {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {sender} already owns {item_guids:?}"),
                ));
            }

            // Only items for sale (they have a cost entry) can be bought.
            let mut total_cost = 0u32;
            for item_guid in &to_buy {
                let Some(cost) = game_server.costs().get(item_guid) else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!("Player {sender} tried to buy item {item_guid}, which isn't for sale"),
                    ));
                };
                total_cost += if player.member { cost.members } else { cost.base };
            }
            if total_cost > player.credits {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} tried to buy {to_buy:?} for {total_cost} credits but only has {}",
                        player.credits
                    ),
                ));
            }

            player.credits -= total_cost;
            let new_credits = player.credits;

            let mut packets = Vec::new();
            for item_guid in &to_buy {
                player.inventory.add_item(*item_guid);

                if let Some(item_config) = game_server.items().get(item_guid) {
                    packets.push(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: AddItems {
                            data: AddItemsData {
                                item: Item {
                                    definition_id: *item_guid,
                                    tint: item_config.tint,
                                    guid: *item_guid,
                                    quantity: 1,
                                    num_consumed: 0,
                                    last_use_time: 0,
                                    market_data: MarketData::None,
                                    unknown2: false,
                                },
                                definition: item_config.to_definition(game_server.abilities()),
                            },
                        },
                    }));
                }

                packets.push(GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SellToClientResponse {
                        result: 1,
                        tid: next_transaction_id(),
                        transaction_type: 1,
                        item_guid: *item_guid,
                        quantity: 1,
                    },
                }));
            }
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateCredits { new_credits },
            }));

            Ok(vec![Broadcast::Single(sender, packets)])
        },
    })
}
