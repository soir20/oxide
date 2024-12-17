use std::io::{Error, ErrorKind};

use evalexpr::{context_map, eval_with_context, Value};

pub struct MinigameStageGroupConfig {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub stage_select_map_name: String,
    pub default_stage_instance: u32,
    pub stages: Vec<MinigameStageConfig>,
}

pub struct MinigameStageConfig {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub difficulty: u32,
    pub start_sound_id: u32,
    pub members_only: bool,
    pub link_name: String,
    pub short_name: String,
    pub score_to_credits_expression: String,
}

fn evaluate_score_to_credits_expression(
    score_to_credits_expression: &str,
    score: i32,
) -> Result<u32, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(score as f64),
    }
    .unwrap_or_else(|_| {
        panic!(
            "Couldn't build expression evaluation context for score {}",
            score
        )
    });

    let result = eval_with_context(score_to_credits_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Unable to evaluate score-to-credits expression for score {}: {}",
                score, err
            ),
        )
    })?;

    if let Value::Float(credits) = result {
        u32::try_from(credits.round() as i64).map_err(|err| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Score-to-credits expression returned float that could not be converted to an integer for score {}: {}, {}",
                    score,
                    credits,
                    err
                ),
            )
        })
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Score-to-credits expression did not return an integer for score {}, returned: {}",
                score, result
            ),
        ))
    }
}
