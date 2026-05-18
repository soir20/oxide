use std::{
    collections::BTreeMap,
    io::{Error, ErrorKind},
};

use evalexpr::{context_map, eval_with_context, Value};

use crate::{
    game_server::{
        handlers::item::ItemConfig,
        packets::store::{StoreItem, StoreItemList},
    },
    ConfigError,
};

pub struct CostEntry {
    pub base: u32,
    pub members: u32,
}

pub type ItemCostMap = BTreeMap<u32, CostEntry>;

pub fn compute_costs(items: &[ItemConfig]) -> Result<BTreeMap<u32, CostEntry>, ConfigError> {
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

fn evaluate_cost_expression(
    cost_expression: &str,
    cost: u32,
    item_guid: u32,
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
