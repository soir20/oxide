use rand::Rng;
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::Deserialize;

use crate::game_server::{
    handlers::minigame::{DailyGamePlayability, PlayerMinigameStats},
    packets::{
        minigame::{FlashPayload, MinigameHeader},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Clone, Debug, Deserialize)]
pub struct DailySpinRewardBucket {
    start: u32,
    end: u32,
    weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DailySpinGameState {
    WaitingForPlayersReady,
    WaitingForSpin,
    Spinning { reward: u32 },
}

#[derive(Clone, Debug)]
pub struct DailySpinGame {
    buckets: Vec<DailySpinRewardBucket>,
    distribution: WeightedAliasIndex<u32>,
    state: DailySpinGameState,
    daily_game_playability: DailyGamePlayability,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl DailySpinGame {
    pub fn new(
        buckets: &[DailySpinRewardBucket],
        daily_game_playability: DailyGamePlayability,
        stage_guid: i32,
        stage_group_guid: i32,
    ) -> Self {
        let distribution =
            WeightedAliasIndex::new(buckets.iter().map(|bucket| bucket.weight).collect())
                .expect("Couldn't create weighted alias index for Daily Spin");
        DailySpinGame {
            buckets: buckets.to_vec(),
            distribution,
            state: DailySpinGameState::WaitingForPlayersReady,
            daily_game_playability,
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn connect(
        &self,
        sender: u32,
        minigame_stats: &PlayerMinigameStats,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.state != DailySpinGameState::WaitingForPlayersReady {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} tried to connect to Daily Spin, but the game has already started ({self:?})"
                ),
            ));
        }

        let total_spins = match self.daily_game_playability {
            DailyGamePlayability::NotYetPlayed { boost } => {
                minigame_stats.boosts_remaining(boost).saturating_add(1)
            }
            DailyGamePlayability::OnlyWithBoosts { boost } => {
                minigame_stats.boosts_remaining(boost)
            }
            DailyGamePlayability::Unplayable => 0,
        };

        Ok(vec![Broadcast::Single(
            sender,
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: "OnWheelDataMsg\t1\t1\t\t0\t0".to_string(),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnWheelUpdateMsg\t1\t{total_spins}\t0\t0"),
                    },
                }),
            ],
        )])
    }

    pub fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.state != DailySpinGameState::WaitingForPlayersReady {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a ready payload for Daily Spin, but the game has already started ({self:?})"
                ),
            ));
        }

        self.state = DailySpinGameState::WaitingForSpin;

        Ok(Vec::new())
    }

    pub fn spin(
        &mut self,
        sender: u32,
        awarded_credits: &mut u32,
        game_won: &mut bool,
        minigame_stats: &mut PlayerMinigameStats,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.state != DailySpinGameState::WaitingForSpin {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a spin request for Daily Spin, but the game isn't waiting to spin ({self:?})"
                ),
            ));
        }

        match self.daily_game_playability {
            DailyGamePlayability::NotYetPlayed { boost } => {
                if minigame_stats.boosts_remaining(boost) == 0 {
                    self.daily_game_playability = DailyGamePlayability::Unplayable;
                } else {
                    self.daily_game_playability = DailyGamePlayability::OnlyWithBoosts { boost };
                }
            },
            DailyGamePlayability::OnlyWithBoosts { boost } => {
                if minigame_stats.use_boost(boost)? == 0 {
                    self.daily_game_playability = DailyGamePlayability::Unplayable;
                } else {
                    self.daily_game_playability = DailyGamePlayability::OnlyWithBoosts { boost };
                }
            },
            DailyGamePlayability::Unplayable => return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a spin request for Daily Spin, but it isn't playable as a daily game ({self:?})"
                ),
            )),
        };

        let rng = &mut rand::thread_rng();
        let bucket_index = self.distribution.sample(rng);
        let bucket = &self.buckets[bucket_index];
        let reward = rng.gen_range(bucket.start..bucket.end);

        *awarded_credits = awarded_credits.saturating_add(reward);
        *game_won = true;
        self.state = DailySpinGameState::Spinning { reward };

        Ok(vec![Broadcast::Single(
            sender,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnSpinInfoMsg\t1\t{reward}"),
                },
            })],
        )])
    }

    pub fn stop_spin(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let DailySpinGameState::Spinning { reward } = self.state else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a stop spin request for Daily Spin, but the game isn't spinning ({self:?})"
                ),
            ));
        };

        self.state = DailySpinGameState::WaitingForSpin;

        Ok(vec![Broadcast::Single(
            sender,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnRewardInfoMsg\t0\t0\t{reward}\t0\t0\t0"),
                },
            })],
        )])
    }
}
