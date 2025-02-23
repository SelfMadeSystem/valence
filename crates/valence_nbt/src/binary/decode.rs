use std::borrow::Cow;
use std::hash::Hash;
use std::{fmt, mem};

use byteorder::{BigEndian, ReadBytesExt};

use crate::tag::Tag;
use crate::{Compound, Error, List, Result, Value};

/// Decodes uncompressed NBT binary data from the provided slice.
///
/// The string returned in the tuple is the name of the root compound
/// (typically the empty string).
pub fn from_binary<'de, S>(slice: &mut &'de [u8]) -> Result<(Compound<S>, Option<S>)>
where
    S: FromModifiedUtf8<'de> + Hash + Ord,
{
    let mut state = DecodeState { slice, depth: 0 };

    let root_tag = state.read_tag()?;

    if root_tag != Tag::Compound {
        return Err(Error::new_owned(format!(
            "expected root tag for compound (got {})",
            root_tag.name(),
        )));
    }

    let root_name = {
        let mut slice = *state.slice;
        let mut peek_state = DecodeState {
            slice: &mut slice,
            depth: 0,
        };

        match peek_state.read_string::<S>() {
            Ok(_) => Some(state.read_string().unwrap()),
            Err(_) => None,
        }
    };
    let root = state.read_compound()?;

    debug_assert_eq!(state.depth, 0);

    Ok((root, root_name))
}

/// Maximum recursion depth to prevent overflowing the call stack.
const MAX_DEPTH: usize = 512;

struct DecodeState<'a, 'de> {
    slice: &'a mut &'de [u8],
    /// Current recursion depth.
    depth: usize,
}

impl<'de> DecodeState<'_, 'de> {
    #[inline]
    fn check_depth<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::new_static("reached maximum recursion depth"));
        }

        self.depth += 1;
        let res = f(self);
        self.depth -= 1;
        res
    }

    fn read_tag(&mut self) -> Result<Tag> {
        match self.slice.read_u8()? {
            0 => Ok(Tag::End),
            1 => Ok(Tag::Byte),
            2 => Ok(Tag::Short),
            3 => Ok(Tag::Int),
            4 => Ok(Tag::Long),
            5 => Ok(Tag::Float),
            6 => Ok(Tag::Double),
            7 => Ok(Tag::ByteArray),
            8 => Ok(Tag::String),
            9 => Ok(Tag::List),
            10 => Ok(Tag::Compound),
            11 => Ok(Tag::IntArray),
            12 => Ok(Tag::LongArray),
            byte => Err(Error::new_owned(format!("invalid tag byte of {byte:#x}"))),
        }
    }

    fn read_value<S>(&mut self, tag: Tag) -> Result<Value<S>>
    where
        S: FromModifiedUtf8<'de> + Hash + Ord,
    {
        match tag {
            Tag::End => unreachable!("illegal TAG_End argument"),
            Tag::Byte => Ok(self.read_byte()?.into()),
            Tag::Short => Ok(self.read_short()?.into()),
            Tag::Int => Ok(self.read_int()?.into()),
            Tag::Long => Ok(self.read_long()?.into()),
            Tag::Float => Ok(self.read_float()?.into()),
            Tag::Double => Ok(self.read_double()?.into()),
            Tag::ByteArray => Ok(self.read_byte_array()?.into()),
            Tag::String => Ok(Value::String(self.read_string::<S>()?)),
            Tag::List => self.check_depth(|st| Ok(st.read_any_list::<S>()?.into())),
            Tag::Compound => self.check_depth(|st| Ok(st.read_compound::<S>()?.into())),
            Tag::IntArray => Ok(self.read_int_array()?.into()),
            Tag::LongArray => Ok(self.read_long_array()?.into()),
        }
    }

    fn read_byte(&mut self) -> Result<i8> {
        Ok(self.slice.read_i8()?)
    }

    fn read_short(&mut self) -> Result<i16> {
        Ok(self.slice.read_i16::<BigEndian>()?)
    }

    fn read_int(&mut self) -> Result<i32> {
        Ok(self.slice.read_i32::<BigEndian>()?)
    }

    fn read_long(&mut self) -> Result<i64> {
        Ok(self.slice.read_i64::<BigEndian>()?)
    }

    fn read_float(&mut self) -> Result<f32> {
        Ok(self.slice.read_f32::<BigEndian>()?)
    }

    fn read_double(&mut self) -> Result<f64> {
        Ok(self.slice.read_f64::<BigEndian>()?)
    }

    fn read_byte_array(&mut self) -> Result<Vec<i8>> {
        let len = self.slice.read_i32::<BigEndian>()?;

        if len.is_negative() {
            return Err(Error::new_owned(format!(
                "negative byte array length of {len}"
            )));
        }

        if len as usize > self.slice.len() {
            return Err(Error::new_owned(format!(
                "byte array length of {len} exceeds remainder of input"
            )));
        }

        let (left, right) = self.slice.split_at(len as usize);

        let array = left.iter().map(|b| *b as i8).collect();
        *self.slice = right;

        Ok(array)
    }

    fn read_string<S>(&mut self) -> Result<S>
    where
        S: FromModifiedUtf8<'de>,
    {
        let len = self.slice.read_u16::<BigEndian>()?.into();

        if len > self.slice.len() {
            return Err(Error::new_owned(format!(
                "string of length {len} exceeds remainder of input"
            )));
        }

        let (left, right) = self.slice.split_at(len);

        match S::from_modified_utf8(left) {
            Ok(str) => {
                *self.slice = right;
                Ok(str)
            }
            Err(_) => Err(Error::new_static("could not decode modified UTF-8 data")),
        }
    }

    fn read_any_list<S>(&mut self) -> Result<List<S>>
    where
        S: FromModifiedUtf8<'de> + Hash + Ord,
    {
        match self.read_tag()? {
            Tag::End => match self.read_int()? {
                0 => Ok(List::End),
                len => Err(Error::new_owned(format!(
                    "TAG_End list with nonzero length of {len}"
                ))),
            },
            Tag::Byte => Ok(self.read_list(Tag::Byte, 1, |st| st.read_byte())?.into()),
            Tag::Short => Ok(self.read_list(Tag::Short, 2, |st| st.read_short())?.into()),
            Tag::Int => Ok(self.read_list(Tag::Int, 4, |st| st.read_int())?.into()),
            Tag::Long => Ok(self.read_list(Tag::Long, 8, |st| st.read_long())?.into()),
            Tag::Float => Ok(self.read_list(Tag::Float, 4, |st| st.read_float())?.into()),
            Tag::Double => Ok(self
                .read_list(Tag::Double, 8, |st| st.read_double())?
                .into()),
            Tag::ByteArray => Ok(self
                .read_list(Tag::ByteArray, 0, |st| st.read_byte_array())?
                .into()),
            Tag::String => Ok(List::String(
                self.read_list(Tag::String, 0, |st| st.read_string::<S>())?,
            )),
            Tag::List => self.check_depth(|st| {
                Ok(st
                    .read_list(Tag::List, 0, |st| st.read_any_list::<S>())?
                    .into())
            }),
            Tag::Compound => self.check_depth(|st| {
                Ok(st
                    .read_list(Tag::Compound, 0, |st| st.read_compound::<S>())?
                    .into())
            }),
            Tag::IntArray => Ok(self
                .read_list(Tag::IntArray, 0, |st| st.read_int_array())?
                .into()),
            Tag::LongArray => Ok(self
                .read_list(Tag::LongArray, 0, |st| st.read_long_array())?
                .into()),
        }
    }

    /// Assumes the element tag has already been read.
    ///
    /// `min_elem_size` is the minimum size of the list element when encoded.
    #[inline]
    fn read_list<T, F>(
        &mut self,
        elem_type: Tag,
        elem_size: usize,
        mut read_elem: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>,
    {
        let len = self.read_int()?;

        if len.is_negative() {
            return Err(Error::new_owned(format!(
                "negative {} list length of {len}",
                elem_type.name()
            )));
        }

        // Ensure we don't reserve more than the maximum amount of memory required given
        // the size of the remaining input.
        if len as u64 * elem_size as u64 > self.slice.len() as u64 {
            return Err(Error::new_owned(format!(
                "{} list of length {len} exceeds remainder of input",
                elem_type.name()
            )));
        }

        let mut list = Vec::with_capacity(if elem_size == 0 { 0 } else { len as usize });

        for _ in 0..len {
            list.push(read_elem(self)?);
        }

        Ok(list)
    }

    fn read_compound<S>(&mut self) -> Result<Compound<S>>
    where
        S: FromModifiedUtf8<'de> + Hash + Ord,
    {
        let mut compound = Compound::new();

        loop {
            let tag = self.read_tag()?;
            if tag == Tag::End {
                return Ok(compound);
            }

            compound.insert(self.read_string::<S>()?, self.read_value::<S>(tag)?);
        }
    }

    fn read_int_array(&mut self) -> Result<Vec<i32>> {
        let len = self.read_int()?;

        if len.is_negative() {
            return Err(Error::new_owned(format!(
                "negative int array length of {len}",
            )));
        }

        if len as u64 * mem::size_of::<i32>() as u64 > self.slice.len() as u64 {
            return Err(Error::new_owned(format!(
                "int array of length {len} exceeds remainder of input"
            )));
        }

        let mut array = Vec::with_capacity(len as usize);
        for _ in 0..len {
            array.push(self.read_int()?);
        }

        Ok(array)
    }

    fn read_long_array(&mut self) -> Result<Vec<i64>> {
        let len = self.read_int()?;

        if len.is_negative() {
            return Err(Error::new_owned(format!(
                "negative long array length of {len}",
            )));
        }

        if len as u64 * mem::size_of::<i64>() as u64 > self.slice.len() as u64 {
            return Err(Error::new_owned(format!(
                "long array of length {len} exceeds remainder of input"
            )));
        }

        let mut array = Vec::with_capacity(len as usize);
        for _ in 0..len {
            array.push(self.read_long()?);
        }

        Ok(array)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FromModifiedUtf8Error;

impl fmt::Display for FromModifiedUtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not decode modified UTF-8 data")
    }
}

impl std::error::Error for FromModifiedUtf8Error {}

/// A string type which can be decoded from Java's [modified UTF-8](https://docs.oracle.com/javase/8/docs/api/java/io/DataInput.html#modified-utf-8).
pub trait FromModifiedUtf8<'de>: Sized {
    fn from_modified_utf8(
        modified_utf8: &'de [u8],
    ) -> std::result::Result<Self, FromModifiedUtf8Error>;
}

impl<'de> FromModifiedUtf8<'de> for Cow<'de, str> {
    fn from_modified_utf8(
        modified_utf8: &'de [u8],
    ) -> std::result::Result<Self, FromModifiedUtf8Error> {
        cesu8::from_java_cesu8(modified_utf8).map_err(move |_| FromModifiedUtf8Error)
    }
}

impl<'de> FromModifiedUtf8<'de> for String {
    fn from_modified_utf8(
        modified_utf8: &'de [u8],
    ) -> std::result::Result<Self, FromModifiedUtf8Error> {
        match cesu8::from_java_cesu8(modified_utf8) {
            Ok(str) => Ok(str.into_owned()),
            Err(_) => Err(FromModifiedUtf8Error),
        }
    }
}

#[cfg(feature = "java_string")]
impl<'de> FromModifiedUtf8<'de> for Cow<'de, java_string::JavaStr> {
    fn from_modified_utf8(
        modified_utf8: &'de [u8],
    ) -> std::result::Result<Self, FromModifiedUtf8Error> {
        java_string::JavaStr::from_modified_utf8(modified_utf8).map_err(|_| FromModifiedUtf8Error)
    }
}

#[cfg(feature = "java_string")]
impl<'de> FromModifiedUtf8<'de> for java_string::JavaString {
    fn from_modified_utf8(
        modified_utf8: &'de [u8],
    ) -> std::result::Result<Self, FromModifiedUtf8Error> {
        match java_string::JavaStr::from_modified_utf8(modified_utf8) {
            Ok(str) => Ok(str.into_owned()),
            Err(_) => Err(FromModifiedUtf8Error),
        }
    }
}
