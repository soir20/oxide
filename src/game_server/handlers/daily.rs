use chrono::{DateTime, Datelike, FixedOffset};
use rand::{seq::SliceRandom, thread_rng, Rng};
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::Deserialize;

use crate::game_server::{
    handlers::{
        character::MinigameWinStatus,
        minigame::{
            award_credits, DailyGamePlayability, DailyResetOffset, MinigameStageConfig,
            PlayerMinigameStats,
        },
    },
    packets::{
        minigame::{FlashPayload, MinigameHeader, ScoreEntry, ScoreType},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Clone, Debug, Deserialize)]
pub struct DailySpinRewardBucket {
    start: u16,
    end: u16,
    weight: u32,
}

#[derive(Clone, Debug)]
enum DailySpinGameState {
    WaitingForPlayersReady,
    WaitingForSpin,
    Spinning { reward: u16 },
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
        if !matches!(self.state, DailySpinGameState::WaitingForPlayersReady) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} tried to connect to Daily Spin, but the game has already started ({self:?})"
                ),
            ));
        }

        let total_spins = match self.daily_game_playability {
            DailyGamePlayability::NotYetPlayed { boost, .. } => {
                minigame_stats.boosts_remaining(boost).saturating_add(1)
            }
            DailyGamePlayability::OnlyWithBoosts { boost, .. } => {
                minigame_stats.boosts_remaining(boost)
            }
            DailyGamePlayability::Unplayable { .. } => 0,
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
                        payload: "OnWheelDataMsg\t0\t0\t\t0\t0".to_string(),
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
                        payload: format!("OnWheelUpdateMsg\t0\t{total_spins}\t0\t0"),
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
                        payload: "OnServerReadyMsg".to_string(),
                    },
                }),
            ],
        )])
    }

    pub fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if !matches!(self.state, DailySpinGameState::WaitingForPlayersReady) {
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
        game_score: &mut i32,
        win_status: &mut MinigameWinStatus,
        score_entries: &mut Vec<ScoreEntry>,
        minigame_stats: &mut PlayerMinigameStats,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if !matches!(self.state, DailySpinGameState::WaitingForSpin) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a spin request for Daily Spin, but the game isn't waiting to spin ({self:?})"
                ),
            ));
        }

        match self.daily_game_playability {
            DailyGamePlayability::NotYetPlayed { boost, timestamp } => {
                if minigame_stats.boosts_remaining(boost) == 0 {
                    self.daily_game_playability = DailyGamePlayability::Unplayable { timestamp };
                } else {
                    self.daily_game_playability = DailyGamePlayability::OnlyWithBoosts { boost, timestamp };
                }
            },
            DailyGamePlayability::OnlyWithBoosts { boost, timestamp } => {
                if minigame_stats.use_boost(boost)? == 0 {
                    self.daily_game_playability = DailyGamePlayability::Unplayable { timestamp };
                } else {
                    self.daily_game_playability = DailyGamePlayability::OnlyWithBoosts { boost, timestamp };
                }
            },
            DailyGamePlayability::Unplayable { .. } => return Err(ProcessPacketError::new(
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

        *game_score = reward as i32;
        win_status.set_win_time(self.daily_game_playability.time());
        score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: reward as i32,
            score_max: 0,
            score_points: 0,
        });
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

    pub fn stop_spin(
        &mut self,
        sender: u32,
        player_credits: &mut u32,
        game_awarded_credits: &mut u32,
        stage_config: &MinigameStageConfig,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let DailySpinGameState::Spinning { reward } = self.state else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a stop spin request for Daily Spin, but the game isn't spinning ({self:?})"
                ),
            ));
        };

        let mut broadcasts = award_credits(
            sender,
            player_credits,
            game_awarded_credits,
            stage_config,
            reward as i32,
        )?
        .0;

        broadcasts.push(Broadcast::Single(
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
        ));

        self.state = DailySpinGameState::WaitingForSpin;

        Ok(broadcasts)
    }
}

const HOLOCRON_DAILY_BONUS: u16 = 50;
const HOLOCRON_REWARDS: [u16; 6] = [100, 150, 200, 250, 300, 600];

#[derive(Clone, Debug)]
enum DailyHolocronGameState {
    WaitingForConnection,
    WaitingForSelection { completions_this_week: [u8; 7] },
    OpeningHolocron { reward: u16 },
    GameOver,
}

#[derive(Clone, Debug)]
pub struct DailyHolocronGame {
    state: DailyHolocronGameState,
    timestamp: DateTime<FixedOffset>,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl DailyHolocronGame {
    pub fn new(
        daily_game_playability: DailyGamePlayability,
        stage_guid: i32,
        stage_group_guid: i32,
    ) -> Self {
        DailyHolocronGame {
            state: DailyHolocronGameState::WaitingForConnection,
            timestamp: daily_game_playability.time(),
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn connect(
        &mut self,
        sender: u32,
        minigame_stats: &PlayerMinigameStats,
        daily_reset_offset: &DailyResetOffset,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if !matches!(self.state, DailyHolocronGameState::WaitingForConnection) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} tried to connect to Daily Holocron, but the game has already started ({self:?})"
                ),
            ));
        }

        let completions_this_week = minigame_stats.completions_this_week(
            self.stage_guid,
            self.timestamp,
            daily_reset_offset,
        );
        self.state = DailyHolocronGameState::WaitingForSelection {
            completions_this_week,
        };

        let current_day = self.timestamp.weekday().num_days_from_sunday();

        let mut packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!(
                        "OnDailyBonusInfo\t{}",
                        [HOLOCRON_DAILY_BONUS; 7]
                            .map(|bonus| bonus.to_string())
                            .join(" ")
                    ),
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
                    payload: format!(
                        "OnHolocronRewardInfo\t{}",
                        HOLOCRON_REWARDS.map(|bonus| bonus.to_string()).join(" ")
                    ),
                },
            }),
        ];
        for holocron in 1..=4 {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!(
                        "OnCurrentWeekActivityInfo\t{}\t{current_day}",
                        completions_this_week
                            .map(|completions| if completions >= holocron {
                                holocron.to_string()
                            } else {
                                "0".to_string()
                            })
                            .join(" ")
                    ),
                },
            }));
        }

        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: FlashPayload {
                header: MinigameHeader {
                    stage_guid: self.stage_guid,
                    sub_op_code: -1,
                    stage_group_guid: self.stage_group_guid,
                },
                payload: "OnServerReadyMsg".to_string(),
            },
        }));

        Ok(vec![Broadcast::Single(sender, packets)])
    }

    pub fn select_holocron(
        &mut self,
        sender: u32,
        game_score: &mut i32,
        win_status: &mut MinigameWinStatus,
        score_entries: &mut Vec<ScoreEntry>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let DailyHolocronGameState::WaitingForSelection {
            completions_this_week,
        } = self.state
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a holocron request for Daily Holocron, but the game isn't waiting to holocron ({self:?})"
                ),
            ));
        };

        let rng = &mut rand::thread_rng();
        let is_double = rng.gen_bool(0.25);
        let side = rng.gen_range(0..HOLOCRON_REWARDS.len());

        let day_factor = if is_double { 2 } else { 1 };
        let reward = HOLOCRON_REWARDS[side] * day_factor
            + completions_this_week
                .into_iter()
                .enumerate()
                .take_while(|(index, _)| {
                    *index < self.timestamp.weekday().num_days_from_sunday() as usize
                })
                .map(|(_, completions)| completions.min(4) as u16 * HOLOCRON_DAILY_BONUS)
                .sum::<u16>();

        *game_score = reward as i32;
        win_status.set_win_time(self.timestamp);
        score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: reward as i32,
            score_max: 0,
            score_points: 0,
        });
        self.state = DailyHolocronGameState::OpeningHolocron { reward };

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
                    payload: format!("OnHolocronSideToLandOnMsg\t{side}\t{}", is_double as u8),
                },
            })],
        )])
    }

    pub fn display_reward(
        &mut self,
        sender: u32,
        player_credits: &mut u32,
        game_awarded_credits: &mut u32,
        stage_config: &MinigameStageConfig,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let DailyHolocronGameState::OpeningHolocron { reward } = self.state else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} sent a display reward for Daily Holocron, but they haven't picked a holocron ({self:?})"
                ),
            ));
        };

        let broadcasts = award_credits(
            sender,
            player_credits,
            game_awarded_credits,
            stage_config,
            reward as i32,
        )?
        .0;

        self.state = DailyHolocronGameState::GameOver;

        Ok(broadcasts)
    }
}

#[derive(Clone, Deserialize)]
pub struct DailyTriviaQuestionConfig {
    pub question_id: u32,
    pub correct_answer_id: u32,
    pub incorrect_answer_ids: [u32; 3],
    pub sound_id: Option<u32>,
}

#[derive(Clone, Debug)]
struct DailyTriviaQuestion {
    answers: [u32; 4],
    correct_answer: u8,
}

impl From<&DailyTriviaQuestionConfig> for DailyTriviaQuestion {
    fn from(value: &DailyTriviaQuestionConfig) -> Self {
        let mut answers = [
            value.correct_answer_id,
            value.incorrect_answer_ids[0],
            value.incorrect_answer_ids[1],
            value.incorrect_answer_ids[2],
        ];

        answers.shuffle(&mut thread_rng());

        DailyTriviaQuestion {
            answers,
            correct_answer: answers
                .iter()
                .position(|answer| *answer == value.correct_answer_id)
                .expect("Correct answer disappeared from answers array")
                as u8,
        }
    }
}

#[derive(Clone, Debug)]
enum DailyTriviaGameState {
    WaitingForConnection,
    AnsweringQuestion { question_index: u8 },
    ReadyForNextQuestion { next_question_index: u8 },
    GameOver,
}

#[derive(Clone, Debug)]
pub struct DailyTriviaGame {
    daily_double: bool,
    consecutive_days_for_daily_double: u32,
    seconds_per_question: u16,
    score_per_second_remaining: i32,
    questions: Vec<DailyTriviaQuestion>,
    state: DailyTriviaGameState,
    timestamp: DateTime<FixedOffset>,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl DailyTriviaGame {
    pub fn new(
        question_bank: &[DailyTriviaQuestionConfig],
        questions_per_game: u8,
        consecutive_days_for_daily_double: u32,
        seconds_per_question: u16,
        score_per_second_remaining: i32,
        daily_game_playability: DailyGamePlayability,
        stage_guid: i32,
        stage_group_guid: i32,
    ) -> Self {
        let questions = question_bank
            .choose_multiple(&mut thread_rng(), questions_per_game as usize)
            .map(|question| question.into())
            .collect();

        DailyTriviaGame {
            daily_double: false,
            consecutive_days_for_daily_double,
            seconds_per_question,
            score_per_second_remaining,
            questions,
            state: DailyTriviaGameState::WaitingForConnection,
            timestamp: daily_game_playability.time(),
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn connect(
        &mut self,
        sender: u32,
        minigame_stats: &PlayerMinigameStats,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if !matches!(self.state, DailyTriviaGameState::WaitingForConnection) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} tried to connect to Daily Trivia, but the game has already started ({self:?})"
                ),
            ));
        }

        let consecutive_completions = minigame_stats.consecutive_days_completed(self.stage_guid);
        self.daily_double = consecutive_completions > 0 && consecutive_completions % 4 == 0;
        self.state = DailyTriviaGameState::ReadyForNextQuestion {
            next_question_index: 0,
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
                        payload: format!(
                            "OnDailyTriviaGameData\t{}\t{}\t{}\t{}",
                            self.seconds_per_question,
                            self.questions.len(),
                            self.score_per_second_remaining,
                            self.daily_double,
                        ),
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
                        payload: "OnServerReadyMsg".to_string(),
                    },
                }),
            ],
        )])
    }
}
