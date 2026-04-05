use std::fmt::Display;

use crate::error::Error;
use crate::token::Token;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{
    self, Deserialize, DeserializeSeed, EnumAccess, Error as _, IntoDeserializer, MapAccess,
    SeqAccess, VariantAccess, Visitor,
};

#[derive(Debug)]
pub(crate) struct Deserializer<'de> {
    tokens: &'de [Token],
}

fn assert_next_token(de: &mut Deserializer, expected: Token) -> Result<(), Error> {
    match de.next_token_opt() {
        Some(token) if token == expected => Ok(()),
        Some(other) => Err(de::Error::custom(format!(
            "expected Token::{} but deserialization wants Token::{}",
            other, expected,
        ))),
        None => Err(de::Error::custom(format!(
            "end of tokens but deserialization wants Token::{}",
            expected,
        ))),
    }
}

fn unexpected(token: Token) -> Error {
    de::Error::custom(format!(
        "deserialization did not expect this token: {}",
        token,
    ))
}

fn assert_name_eq(expected: impl Display, actual: impl Display) -> Result<(), Error> {
    let expected = expected.to_string();
    let actual = actual.to_string();

    if expected == actual {
        Ok(())
    } else {
        Err(de::Error::custom(format!(
            "expected name `{}` but got `{}`",
            expected, actual
        )))
    }
}

fn assert_len_eq(expected: usize, actual: usize) -> Result<(), Error> {
    if expected == actual {
        Ok(())
    } else {
        Err(de::Error::custom(format!(
            "expected length {} but got {}",
            expected, actual
        )))
    }
}

fn assert_contains(expected: &[impl Display], actual: impl Display) -> Result<(), Error> {
    let expected = expected.iter().map(ToString::to_string).collect::<Vec<_>>();
    let actual = actual.to_string();

    if expected.contains(&actual) {
        Ok(())
    } else {
        Err(de::Error::custom(format!(
            "expected one of [{}] but got `{}`",
            expected.join(", "),
            actual
        )))
    }
}

fn end_of_tokens() -> Error {
    de::Error::custom("ran out of tokens to deserialize")
}

impl<'de> Deserializer<'de> {
    pub(crate) fn new(tokens: &'de [Token]) -> Self {
        Deserializer { tokens }
    }

    fn peek_token_opt(&self) -> Option<Token> {
        self.tokens.first().copied()
    }

    fn peek_token(&self) -> Result<Token, Error> {
        self.peek_token_opt().ok_or_else(end_of_tokens)
    }

    pub(crate) fn next_token_opt(&mut self) -> Option<Token> {
        match self.tokens.split_first() {
            Some((&first, rest)) => {
                self.tokens = rest;
                Some(first)
            }
            None => None,
        }
    }

    fn next_token(&mut self) -> Result<Token, Error> {
        let (&first, rest) = self.tokens.split_first().ok_or_else(end_of_tokens)?;
        self.tokens = rest;
        Ok(first)
    }

    pub(crate) fn remaining(&self) -> usize {
        self.tokens.len()
    }

    fn visit_seq<V>(
        &mut self,
        len: Option<usize>,
        end: Token,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let value = visitor.visit_seq(DeserializerSeqVisitor { de: self, len, end })?;
        assert_next_token(self, end)?;
        Ok(value)
    }

    fn visit_map<V>(
        &mut self,
        len: Option<usize>,
        end: Token,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let value = visitor.visit_map(DeserializerMapVisitor { de: self, len, end })?;
        assert_next_token(self, end)?;
        Ok(value)
    }
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let token = self.next_token()?;
        match token {
            Token::Bool(v) => visitor.visit_bool(v),
            Token::I8(v) => visitor.visit_i8(v),
            Token::I16(v) => visitor.visit_i16(v),
            Token::I32(v) => visitor.visit_i32(v),
            Token::I64(v) => visitor.visit_i64(v),
            Token::I128(v) => visitor.visit_i128(v),
            Token::U8(v) => visitor.visit_u8(v),
            Token::U16(v) => visitor.visit_u16(v),
            Token::U32(v) => visitor.visit_u32(v),
            Token::U64(v) => visitor.visit_u64(v),
            Token::U128(v) => visitor.visit_u128(v),
            Token::F32(v) => visitor.visit_f32(v),
            Token::F64(v) => visitor.visit_f64(v),
            Token::Char(v) => visitor.visit_char(v),
            Token::Str(v) => visitor.visit_str(v),
            Token::BorrowedStr(v) => visitor.visit_borrowed_str(v),
            Token::String(v) => visitor.visit_string(v.to_owned()),
            Token::Bytes(v) => visitor.visit_bytes(v),
            Token::BorrowedBytes(v) => visitor.visit_borrowed_bytes(v),
            Token::ByteBuf(v) => visitor.visit_byte_buf(v.to_vec()),
            Token::None => visitor.visit_none(),
            Token::Some => visitor.visit_some(self),
            Token::Unit | Token::UnitStruct { .. } => visitor.visit_unit(),
            Token::NewtypeStruct { .. } => visitor.visit_newtype_struct(self),
            Token::Seq { len } => self.visit_seq(len, Token::SeqEnd, visitor),
            Token::Tuple { len } => self.visit_seq(Some(len), Token::TupleEnd, visitor),
            Token::TupleStruct { len, .. } => {
                self.visit_seq(Some(len), Token::TupleStructEnd, visitor)
            }
            Token::Map { len } => self.visit_map(len, Token::MapEnd, visitor),
            Token::Struct { len, .. } => self.visit_map(Some(len), Token::StructEnd, visitor),
            Token::Enum { .. } => {
                let variant = self.next_token()?;
                let next = self.peek_token()?;
                match (variant, next) {
                    (Token::Str(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_str(variant)
                    }
                    (Token::BorrowedStr(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_borrowed_str(variant)
                    }
                    (Token::String(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_string(variant.to_string())
                    }
                    (Token::Bytes(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_bytes(variant)
                    }
                    (Token::BorrowedBytes(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_borrowed_bytes(variant)
                    }
                    (Token::ByteBuf(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_byte_buf(variant.to_vec())
                    }
                    (Token::U8(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_u8(variant)
                    }
                    (Token::U16(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_u16(variant)
                    }
                    (Token::U32(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_u32(variant)
                    }
                    (Token::U64(variant), Token::Unit) => {
                        self.next_token()?;
                        visitor.visit_u64(variant)
                    }
                    (variant, Token::Unit) => Err(unexpected(variant)),
                    (variant, _) => {
                        visitor.visit_map(EnumMapVisitor::new(self, variant, EnumFormat::Any))
                    }
                }
            }
            Token::UnitVariant { variant, .. } => visitor.visit_str(variant),
            Token::NewtypeVariant { variant, .. } => visitor.visit_map(EnumMapVisitor::new(
                self,
                Token::Str(variant),
                EnumFormat::Any,
            )),
            Token::TupleVariant { variant, .. } => visitor.visit_map(EnumMapVisitor::new(
                self,
                Token::Str(variant),
                EnumFormat::Seq,
            )),
            Token::StructVariant { variant, .. } => visitor.visit_map(EnumMapVisitor::new(
                self,
                Token::Str(variant),
                EnumFormat::Map,
            )),
            Token::SeqEnd
            | Token::TupleEnd
            | Token::TupleStructEnd
            | Token::MapEnd
            | Token::StructEnd
            | Token::TupleVariantEnd
            | Token::StructVariantEnd => Err(unexpected(token)),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Unit | Token::None => visitor.visit_none(),
            Token::Some => visitor.visit_some(self),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.peek_token()? {
            Token::Enum { name: n } => {
                assert_name_eq(name, n)?;
                visitor.visit_enum(DeserializerEnumVisitor { de: self })
            }
            Token::UnitVariant {
                name: n,
                variant_index: _,
                variant,
            }
            | Token::NewtypeVariant { name: n, variant }
            | Token::TupleVariant {
                name: n,
                variant,
                len: _,
            }
            | Token::StructVariant {
                name: n,
                variant,
                len: _,
            } => {
                assert_name_eq(name, n)?;
                assert_contains(variants, variant)?;
                visitor.visit_enum(DeserializerEnumVisitor { de: self })
            }
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_unit_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::UnitStruct { name: n } => {
                assert_name_eq(name, n)?;
                visitor.visit_unit()
            }
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::NewtypeStruct { name: n } => {
                assert_name_eq(name, n)?;
                visitor.visit_newtype_struct(self)
            }
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Unit | Token::UnitStruct { .. } => visitor.visit_unit(),
            Token::Seq { .. } => self.visit_seq(Some(len), Token::SeqEnd, visitor),
            Token::Tuple { .. } => self.visit_seq(Some(len), Token::TupleEnd, visitor),
            Token::TupleStruct { .. } => self.visit_seq(Some(len), Token::TupleStructEnd, visitor),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Unit => visitor.visit_unit(),
            Token::UnitStruct { name: n } => {
                assert_name_eq(name, n)?;
                visitor.visit_unit()
            }
            Token::Seq { len: l } => {
                if let Some(enum_len) = l {
                    assert_len_eq(len, enum_len)?;
                }
                self.visit_seq(Some(len), Token::SeqEnd, visitor)
            }
            Token::Tuple { len: l } => {
                assert_len_eq(len, l)?;
                self.visit_seq(Some(len), Token::TupleEnd, visitor)
            }
            Token::TupleStruct { name: n, len: l } => {
                assert_name_eq(name, n)?;
                assert_len_eq(len, l)?;
                self.visit_seq(Some(len), Token::TupleStructEnd, visitor)
            }
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Struct { name: n, len } => {
                assert_name_eq(name, n)?;
                assert_len_eq(fields.len(), len)?;
                self.visit_map(Some(fields.len()), Token::StructEnd, visitor)
            }
            Token::Map { len } => {
                if let Some(enum_len) = len {
                    assert_len_eq(fields.len(), enum_len)?;
                }
                self.visit_map(Some(fields.len()), Token::MapEnd, visitor)
            }
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Bool(v) => visitor.visit_bool(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::I8(v) => visitor.visit_i8(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::I16(v) => visitor.visit_i16(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::I32(v) => visitor.visit_i32(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::I64(v) => visitor.visit_i64(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::I128(v) => visitor.visit_i128(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::U8(v) => visitor.visit_u8(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::U16(v) => visitor.visit_u16(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::U32(v) => visitor.visit_u32(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::U64(v) => visitor.visit_u64(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::U128(v) => visitor.visit_u128(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::F32(v) => visitor.visit_f32(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::F64(v) => visitor.visit_f64(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Char(v) => visitor.visit_char(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Str(v) => visitor.visit_str(v),
            Token::BorrowedStr(v) => visitor.visit_borrowed_str(v),
            Token::String(v) => visitor.visit_string(v.to_owned()),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Str(v) => visitor.visit_str(v),
            Token::BorrowedStr(v) => visitor.visit_borrowed_str(v),
            Token::String(v) => visitor.visit_string(v.to_owned()),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Bytes(v) => visitor.visit_bytes(v),
            Token::BorrowedBytes(v) => visitor.visit_borrowed_bytes(v),
            Token::ByteBuf(v) => visitor.visit_byte_buf(v.to_vec()),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Bytes(v) => visitor.visit_bytes(v),
            Token::BorrowedBytes(v) => visitor.visit_borrowed_bytes(v),
            Token::ByteBuf(v) => visitor.visit_byte_buf(v.to_vec()),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Unit => visitor.visit_unit(),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Seq { len } => self.visit_seq(len, Token::SeqEnd, visitor),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Map { len } => self.visit_map(len, Token::MapEnd, visitor),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.next_token()? {
            Token::Str(v) => visitor.visit_str(v),
            Token::BorrowedStr(v) => visitor.visit_borrowed_str(v),
            Token::String(v) => visitor.visit_string(v.to_owned()),
            Token::U8(v) => visitor.visit_u8(v),
            Token::U16(v) => visitor.visit_u16(v),
            Token::U32(v) => visitor.visit_u32(v),
            Token::U64(v) => visitor.visit_u64(v),
            token => Err(de::Error::invalid_type(token.into_unexpected(), &visitor)),
        }
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn is_human_readable(&self) -> bool {
        panic!(
            "Types which have different human-readable and compact representations \
             must explicitly mark their test cases with `serde_test2::Configure`"
        );
    }
}

//////////////////////////////////////////////////////////////////////////

struct DeserializerSeqVisitor<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    len: Option<usize>,
    end: Token,
}

impl<'de> SeqAccess<'de> for DeserializerSeqVisitor<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.de.peek_token_opt() == Some(self.end) {
            return Ok(None);
        }
        self.len = self.len.map(|len| len.saturating_sub(1));
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        self.len
    }
}

//////////////////////////////////////////////////////////////////////////

struct DeserializerMapVisitor<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    len: Option<usize>,
    end: Token,
}

impl<'de> MapAccess<'de> for DeserializerMapVisitor<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: DeserializeSeed<'de>,
    {
        if self.de.peek_token_opt() == Some(self.end) {
            return Ok(None);
        }
        self.len = self.len.map(|len| len.saturating_sub(1));
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.de)
    }

    fn size_hint(&self) -> Option<usize> {
        self.len
    }
}

//////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
struct DeserializerEnumVisitor<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
}

impl<'de> EnumAccess<'de> for DeserializerEnumVisitor<'_, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self), Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.de.peek_token()? {
            Token::UnitVariant { variant: v, .. }
            | Token::NewtypeVariant { variant: v, .. }
            | Token::TupleVariant { variant: v, .. }
            | Token::StructVariant { variant: v, .. } => {
                let de = v.into_deserializer();
                let value = seed.deserialize(de)?;
                Ok((value, self))
            }
            Token::Enum { .. } => {
                // Consume the `Enum` header and deserialize the following variant identifier token
                // as the variant seed expects.
                self.de.next_token()?; // consume Token::Enum
                match self.de.next_token()? {
                    Token::Str(v) | Token::BorrowedStr(v) | Token::String(v) => {
                        let value = seed.deserialize(v.into_deserializer())?;
                        Ok((value, self))
                    }
                    Token::Bytes(v) | Token::BorrowedBytes(v) | Token::ByteBuf(v) => {
                        let value = seed.deserialize(BytesDeserializer { value: v })?;
                        Ok((value, self))
                    }
                    Token::U8(v) => {
                        let value = seed.deserialize(v.into_deserializer())?;
                        Ok((value, self))
                    }
                    Token::U16(v) => {
                        let value = seed.deserialize(v.into_deserializer())?;
                        Ok((value, self))
                    }
                    Token::U32(v) => {
                        let value = seed.deserialize(v.into_deserializer())?;
                        Ok((value, self))
                    }
                    Token::U64(v) => {
                        let value = seed.deserialize(v.into_deserializer())?;
                        Ok((value, self))
                    }
                    other => Err(unexpected(other)),
                }
            }
            _ => {
                let value = seed.deserialize(&mut *self.de)?;
                Ok((value, self))
            }
        }
    }
}

impl<'de> VariantAccess<'de> for DeserializerEnumVisitor<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.de.peek_token()? {
            Token::UnitVariant { .. } => {
                self.de.next_token()?;
                Ok(())
            }
            _ => Deserialize::deserialize(self.de),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.de.peek_token()? {
            Token::NewtypeVariant { .. } => {
                self.de.next_token()?;
                seed.deserialize(self.de)
            }
            _ => seed.deserialize(self.de),
        }
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.de.next_token()? {
            Token::TupleVariant { len: enum_len, .. } => {
                assert_len_eq(len, enum_len)?;
                self.de
                    .visit_seq(Some(len), Token::TupleVariantEnd, visitor)
            }
            Token::Seq {
                len: Some(enum_len),
            } => {
                assert_len_eq(len, enum_len)?;
                self.de.visit_seq(Some(len), Token::SeqEnd, visitor)
            }
            token => Err(unexpected(token)),
        }
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.de.next_token()? {
            Token::StructVariant { len: enum_len, .. } => {
                assert_len_eq(fields.len(), enum_len)?;
                self.de
                    .visit_map(Some(fields.len()), Token::StructVariantEnd, visitor)
            }
            Token::Map {
                len: Some(enum_len),
            } => {
                assert_len_eq(fields.len(), enum_len)?;
                self.de
                    .visit_map(Some(fields.len()), Token::MapEnd, visitor)
            }
            token => Err(unexpected(token)),
        }
    }
}

//////////////////////////////////////////////////////////////////////////

struct EnumMapVisitor<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    variant: Option<Token>,
    format: EnumFormat,
}

enum EnumFormat {
    Seq,
    Map,
    Any,
}

impl<'a, 'de> EnumMapVisitor<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, variant: Token, format: EnumFormat) -> Self {
        EnumMapVisitor {
            de,
            variant: Some(variant),
            format,
        }
    }
}

impl<'de> MapAccess<'de> for EnumMapVisitor<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: DeserializeSeed<'de>,
    {
        match self.variant.take() {
            Some(Token::Str(variant)) => seed.deserialize(variant.into_deserializer()).map(Some),
            Some(Token::Bytes(variant)) => seed
                .deserialize(BytesDeserializer { value: variant })
                .map(Some),
            Some(Token::U32(variant)) => seed.deserialize(variant.into_deserializer()).map(Some),
            Some(other) => Err(unexpected(other)),
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.format {
            EnumFormat::Seq => {
                let value = {
                    let visitor = DeserializerSeqVisitor {
                        de: self.de,
                        len: None,
                        end: Token::TupleVariantEnd,
                    };
                    seed.deserialize(SeqAccessDeserializer::new(visitor))?
                };
                assert_next_token(self.de, Token::TupleVariantEnd)?;
                Ok(value)
            }
            EnumFormat::Map => {
                let value = {
                    let visitor = DeserializerMapVisitor {
                        de: self.de,
                        len: None,
                        end: Token::StructVariantEnd,
                    };
                    seed.deserialize(MapAccessDeserializer::new(visitor))?
                };
                assert_next_token(self.de, Token::StructVariantEnd)?;
                Ok(value)
            }
            EnumFormat::Any => seed.deserialize(&mut *self.de),
        }
    }
}

struct BytesDeserializer {
    value: &'static [u8],
}

impl<'de> de::Deserializer<'de> for BytesDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_bytes(self.value)
    }

    fn deserialize_bool<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found bool"))
    }

    fn deserialize_i8<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found i8"))
    }

    fn deserialize_i16<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found i16"))
    }

    fn deserialize_i32<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found i32"))
    }

    fn deserialize_i64<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found i64"))
    }

    fn deserialize_u8<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found u8"))
    }

    fn deserialize_u16<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found u16"))
    }

    fn deserialize_u32<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found u32"))
    }

    fn deserialize_u64<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found u64"))
    }

    fn deserialize_f32<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found f32"))
    }

    fn deserialize_f64<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found f64"))
    }

    fn deserialize_char<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found char"))
    }

    fn deserialize_str<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found str"))
    }

    fn deserialize_string<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found string"))
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bytes(self.value)
    }

    fn deserialize_byte_buf<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found byte_buf"))
    }

    fn deserialize_option<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found option"))
    }

    fn deserialize_unit<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found unit"))
    }

    fn deserialize_unit_struct<V>(self, _: &'static str, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found unit_struct"))
    }

    fn deserialize_newtype_struct<V>(self, _: &'static str, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found newtype_struct"))
    }

    fn deserialize_seq<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found seq"))
    }

    fn deserialize_tuple<V>(self, _: usize, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found tuple"))
    }

    fn deserialize_tuple_struct<V>(
        self,
        _: &'static str,
        _: usize,
        _: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found tuple_struct"))
    }

    fn deserialize_map<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found map"))
    }

    fn deserialize_struct<V>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found struct"))
    }

    fn deserialize_enum<V>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found enum"))
    }

    fn deserialize_identifier<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found identifier"))
    }

    fn deserialize_ignored_any<V>(self, _: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::custom("expected bytes but found ignored_any"))
    }
}
