use bitfield_struct::bitfield;

use crate::{Decode, Encode, Packet, VarInt};

#[derive(Clone, Debug, Encode, Decode, Packet)]
pub struct UpdateMobEffectS2c {
    pub entity_id: VarInt,
    pub effect_id: VarInt, // TODO: effect ID registry.
    pub amplifier: u8,
    pub duration: VarInt,
    pub flags: Flags,
}

#[bitfield(u8)]
#[derive(PartialEq, Eq, Encode, Decode)]
pub struct Flags {
    pub is_ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
    #[bits(5)]
    _pad: u8,
}
