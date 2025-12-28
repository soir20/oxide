use crate::game_server::packets::{
    update_position::{PlayerJump, UpdatePlayerPlatformPos, UpdatePlayerPos},
    GamePacket, Pos,
};

pub trait UpdatePosPacket: Copy + GamePacket {
    fn apply_jump_height_multiplier(&mut self, multiplier: f32);

    fn guid(&self) -> u64;

    fn pos(&self) -> Pos;

    fn rot(&self) -> Pos;
}

impl UpdatePosPacket for UpdatePlayerPos {
    fn apply_jump_height_multiplier(&mut self, _: f32) {}

    fn guid(&self) -> u64 {
        self.guid
    }

    fn pos(&self) -> Pos {
        Pos {
            x: self.pos_x,
            y: self.pos_y,
            z: self.pos_z,
            w: 1.0,
        }
    }

    fn rot(&self) -> Pos {
        Pos {
            x: self.rot_x,
            y: self.rot_y,
            z: self.rot_z,
            w: 1.0,
        }
    }
}

impl UpdatePosPacket for PlayerJump {
    fn apply_jump_height_multiplier(&mut self, multiplier: f32) {
        self.vertical_speed *= multiplier;
    }

    fn guid(&self) -> u64 {
        self.pos_update.guid()
    }

    fn pos(&self) -> Pos {
        self.pos_update.pos()
    }

    fn rot(&self) -> Pos {
        self.pos_update.rot()
    }
}

impl UpdatePosPacket for UpdatePlayerPlatformPos {
    fn apply_jump_height_multiplier(&mut self, multiplier: f32) {
        self.pos_update.apply_jump_height_multiplier(multiplier);
    }

    fn guid(&self) -> u64 {
        self.pos_update.guid()
    }

    fn pos(&self) -> Pos {
        self.pos_update.pos()
    }

    fn rot(&self) -> Pos {
        self.pos_update.rot()
    }
}

pub struct UpdatePosProgress<T> {
    pub new_pos: Pos,
    pub new_rot: Pos,
    pub packet: T,
}

impl From<UpdatePlayerPos> for UpdatePosProgress<UpdatePlayerPos> {
    fn from(packet: UpdatePlayerPos) -> Self {
        UpdatePosProgress {
            new_pos: packet.pos(),
            new_rot: packet.rot(),
            packet,
        }
    }
}

impl From<PlayerJump> for UpdatePosProgress<PlayerJump> {
    fn from(packet: PlayerJump) -> Self {
        UpdatePosProgress {
            new_pos: packet.pos(),
            new_rot: packet.rot(),
            packet,
        }
    }
}

impl From<UpdatePlayerPlatformPos> for UpdatePosProgress<UpdatePlayerPlatformPos> {
    fn from(packet: UpdatePlayerPlatformPos) -> Self {
        UpdatePosProgress {
            new_pos: packet.pos(),
            new_rot: packet.rot(),
            packet,
        }
    }
}
