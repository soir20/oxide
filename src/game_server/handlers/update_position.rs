use crate::game_server::packets::{
    update_position::{PlayerJump, UpdatePlayerPlatformPosition, UpdatePlayerPos},
    GamePacket, Pos,
};

pub trait UpdatePositionPacket: Copy + GamePacket {
    fn apply_jump_height_multiplier(&mut self, multiplier: f32);

    fn guid(&self) -> u64;

    fn pos(&self) -> Pos;

    fn rot(&self) -> Pos;
}

impl UpdatePositionPacket for UpdatePlayerPos {
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

impl UpdatePositionPacket for PlayerJump {
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

impl UpdatePositionPacket for UpdatePlayerPlatformPosition {
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
