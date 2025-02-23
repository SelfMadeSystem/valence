use crate::{Decode, Encode, Packet};

#[derive(Copy, Clone, Debug, Encode, Decode, Packet)]
pub struct ContainerButtonClickC2s {
    pub window_id: i8,
    pub button_id: i8,
}
