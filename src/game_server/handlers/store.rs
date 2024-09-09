use std::{
    collections::BTreeMap,
    fs::File,
    io::{Error, ErrorKind},
    path::Path,
};

use evalexpr::{context_map, eval_with_context, Value};
use serde::Deserialize;

use crate::game_server::packets::{
    item::ItemDefinition,
    reference_data::ItemGroupDefinition,
    store::{StoreItem, StoreItemList},
};

const DEFAULT_COST_EXPRESSION: &str = "x";

#[derive(Deserialize)]
struct Sale {
    item_group_guid: i32,
    base_cost_expression: Option<String>,
    members_cost_expression: Option<String>,
}

pub struct CostEntry {
    pub base: u32,
    pub members: u32,
}

pub fn load_cost_map(
    config_dir: &Path,
    items: &BTreeMap<u32, ItemDefinition>,
    item_groups: &[ItemGroupDefinition],
) -> Result<BTreeMap<u32, CostEntry>, Error> {
    let mut file = File::open(config_dir.join("sales.json"))?;
    let sales: Vec<Sale> = serde_json::from_reader(&mut file)?;
    cost_map_from_sales(items, item_groups, sales)
}

impl From<&BTreeMap<u32, CostEntry>> for StoreItemList {
    fn from(cost_map: &BTreeMap<u32, CostEntry>) -> Self {
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

fn cost_map_from_sales(
    items: &BTreeMap<u32, ItemDefinition>,
    item_groups: &[ItemGroupDefinition],
    sales: Vec<Sale>,
) -> Result<BTreeMap<u32, CostEntry>, Error> {
    let items_by_group = items_by_group(item_groups);
    let mut costs = BTreeMap::new();

    for sale in sales {
        if let Some(items_in_group) = items_by_group.get(&sale.item_group_guid) {
            for item_guid in items_in_group {
                let cost_entry = costs.entry(*item_guid).or_insert_with(|| {
                    if let Some(item_definition) = items.get(item_guid) {
                        CostEntry {
                            base: item_definition.cost,
                            members: item_definition.cost,
                        }
                    } else {
                        println!("Defaulting to 0 cost for unknown item {}", item_guid);
                        CostEntry {
                            base: 0,
                            members: 0,
                        }
                    }
                });

                cost_entry.base = evaluate_cost_expression(
                    sale.base_cost_expression
                        .as_deref()
                        .unwrap_or(DEFAULT_COST_EXPRESSION),
                    cost_entry.base,
                )?;
                cost_entry.members = evaluate_cost_expression(
                    sale.members_cost_expression
                        .as_deref()
                        .unwrap_or(DEFAULT_COST_EXPRESSION),
                    cost_entry.members,
                )?;
            }
        } else {
            println!(
                "Skipping sale for unknown item group {}",
                sale.item_group_guid
            )
        }
    }

    Ok(costs)
}

fn items_by_group(item_groups: &[ItemGroupDefinition]) -> BTreeMap<i32, Vec<u32>> {
    let mut items_by_group = BTreeMap::new();

    for definition in item_groups {
        for item in &definition.items {
            items_by_group
                .entry(definition.guid)
                .or_insert_with(Vec::new)
                .push(item.guid);
        }
    }

    items_by_group
}

fn evaluate_cost_expression(cost_expression: &str, cost: u32) -> Result<u32, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(cost as f64),
    }
    .expect("Couldn't build expression evaluation context");

    let result = eval_with_context(cost_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Unable to evaluate cost expression: {}", err),
        )
    })?;

    if let Value::Float(new_cost) = result {
        u32::try_from(new_cost.round() as i64).map_err(|err| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Cost expression returned float that could not be converted to an integer: {}",
                    err
                ),
            )
        })
    } else {
        panic!("Cost expression did not return an integer")
    }
}
