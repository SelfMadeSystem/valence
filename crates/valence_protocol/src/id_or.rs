use std::{fmt::Debug, io::Write};

use anyhow::Error;
use valence_generated::registry_id::RegistryId;

use crate::{Decode, Encode, VarInt};

#[derive(Clone, Debug, PartialEq)]
pub enum IdOr<'a, T: Decode<'a> + Encode + Clone + Debug + PartialEq> {
    Id(RegistryId),
    Inline(T),
    Phantom(std::marker::PhantomData<&'a T>),
}

impl<'a, T: Decode<'a> + Encode + Clone + Debug + PartialEq> IdOr<'a, T> {
    pub fn id(id: impl Into<RegistryId>) -> Self {
        Self::Id(id.into())
    }

    pub fn inline(value: T) -> Self {
        Self::Inline(value)
    }
}

impl<'a, T: Decode<'a> + Encode + Clone + Debug + PartialEq> Encode for IdOr<'a, T> {
    fn encode(&self, mut buf: impl Write) -> anyhow::Result<()> {
        match self {
            Self::Id(id) => (id.id() + 1).encode(buf),
            Self::Inline(value) => {
                0.encode(&mut buf).unwrap();
                value.encode(&mut buf)
            }
            _ => Ok(()),
        }
    }
}

impl<'a, T: Decode<'a> + Encode + Clone + Debug + PartialEq> Decode<'a> for IdOr<'a, T> {
    fn decode(buf: &mut &'a [u8]) -> Result<Self, Error> {
        let id = VarInt::decode(buf)?;
        if id == VarInt(0) {
            let value = T::decode(buf)?;
            Ok(Self::Inline(value))
        } else {
            let registry_id = RegistryId::new(id.0 - 1);
            Ok(Self::Id(registry_id))
        }
    }
}
