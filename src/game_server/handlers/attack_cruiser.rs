use std::{
    io::{Cursor, Read},
    time::Instant,
};

use packet_serialize::DeserializePacket;
use serde::Deserialize;

use crate::game_server::{
    handlers::{
        character::{MinigameMatchmakingGroup, MinigameStatus},
        minigame::{
            handle_minigame_packet_write, MinigameRemovePlayerResult, SharedMinigameTypeData,
        },
    },
    packets::{attack_cruiser::AttackCruiserOpCode, minigame::MinigameHeader},
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
enum AttackCruiserPlayerReadiness {
    Ready,
    #[default]
    Unready,
}

#[derive(Clone, Debug, Default)]
struct AttackCruiserPlayerState {
    readiness: AttackCruiserPlayerReadiness,
    pub score: i32,
    pub score_multiplier_tier_progress: u16,
    pub score_multiplier_tier: u16,
    pub lives: u8,
    pub health: u16,
}

impl AttackCruiserPlayerState {
    pub fn new(lives: u8, health: u16) -> Self {
        AttackCruiserPlayerState {
            readiness: AttackCruiserPlayerReadiness::default(),
            score: 0,
            score_multiplier_tier_progress: 0,
            score_multiplier_tier: 1,
            lives,
            health,
        }
    }
}

#[derive(Clone, Debug)]
enum AttackCruiserGameState {
    WaitingForPlayersReady,
    WaveActive,
    GameOver,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserConfig {
    max_health: u16,
}

pub fn process_attack_cruiser_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let header = MinigameHeader::deserialize(cursor)?;
    handle_minigame_packet_write(
        sender,
        game_server,
        &header,
        |_, _, _, _, shared_minigame_data, _| {
            let SharedMinigameTypeData::AttackCruiser { game } = &mut shared_minigame_data.data
            else {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!(
                        "Received Attack Cruiser packet from unexpected game: {}, {buffer:x?}",
                        header.sub_op_code
                    ),
                ));
            };

            match AttackCruiserOpCode::try_from(header.sub_op_code) {
                Ok(op_code) => match op_code {
                    _ => {
                        let mut buffer = Vec::new();
                        cursor.read_to_end(&mut buffer)?;
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::UnknownOpCode,
                            format!(
                                "Unimplemented Attack Cruiser op code: {op_code:?} {buffer:x?}"
                            ),
                        ))
                    }
                },
                Err(_) => {
                    let mut buffer = Vec::new();
                    cursor.read_to_end(&mut buffer)?;
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::UnknownOpCode,
                        format!(
                            "Unknown Attack Cruiser packet: {}, {buffer:x?}",
                            header.sub_op_code
                        ),
                    ))
                }
            }
        },
    )
}

#[derive(Clone, Debug)]
pub struct AttackCruiserGame {
    config: AttackCruiserConfig,
    player1: u32,
    player2: Option<u32>,
    player_states: [AttackCruiserPlayerState; 2],
    state: AttackCruiserGameState,
    recipients: Vec<u32>,
    group: MinigameMatchmakingGroup,
}

impl AttackCruiserGame {
    pub fn new(
        config: AttackCruiserConfig,
        player1: u32,
        player2: Option<u32>,
        group: MinigameMatchmakingGroup,
    ) -> Self {
        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }
        AttackCruiserGame {
            config,
            player1,
            player2,
            player_states: Default::default(),
            state: AttackCruiserGameState::WaitingForPlayersReady,
            recipients,
            group,
        }
    }

    pub fn start(&self, sender: u32) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        //

        Ok(Vec::new())
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        Vec::new()
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        Ok(Vec::new())
    }

    pub fn remove_player(
        &self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<MinigameRemovePlayerResult, ProcessPacketError> {
        Ok(MinigameRemovePlayerResult {
            broadcasts: Vec::new(),
            characters_to_remove: Vec::new(),
            end_game_for_all: false,
        })
    }

    fn is_singleplayer(&self) -> bool {
        self.player2.is_none()
    }

    fn player_index(&self, sender: u32) -> Result<u8, ProcessPacketError> {
        if sender == self.player1 {
            Ok(0)
        } else if Some(sender) == self.player2 {
            Ok(1)
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {sender} sent a packet for Attack Cruiser, but they aren't one of the game's players ({self:?})")
            ))
        }
    }
}
