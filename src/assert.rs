use core::fmt::Debug;

use serde_core::{Deserialize, Serialize};

use crate::de::Deserializer;
use crate::ser::Serializer;
use crate::token::Token;

/// Runs both `assert_ser_tokens` and `assert_de_tokens`.
///
/// ```
/// # use serde_derive::{Deserialize, Serialize};
/// # use serde_test2::{assert_tokens, Token};
/// #
/// #[derive(Serialize, Deserialize, PartialEq, Debug)]
/// struct S {
///     a: u8,
///     b: u8,
/// }
///
/// let s = S { a: 0, b: 0 };
/// assert_tokens(
///     &s,
///     &[
///         Token::Struct { name: "S", len: 2 },
///         Token::Str("a"),
///         Token::U8(0),
///         Token::Str("b"),
///         Token::U8(0),
///         Token::StructEnd,
///     ],
/// );
/// ```
#[track_caller]
pub fn assert_tokens<'de, T>(value: &T, tokens: &'de [Token])
where
    T: Serialize + Deserialize<'de> + PartialEq + Debug,
{
    assert_ser_tokens(value, tokens);
    assert_de_tokens(value, tokens);
}

/// Asserts that `value` serializes to the given `tokens`.
///
/// ```
/// # use serde_derive::{Deserialize, Serialize};
/// # use serde_test2::{assert_ser_tokens, Token};
/// #
/// #[derive(Serialize, Deserialize, PartialEq, Debug)]
/// struct S {
///     a: u8,
///     b: u8,
/// }
///
/// let s = S { a: 0, b: 0 };
/// assert_ser_tokens(
///     &s,
///     &[
///         Token::Struct { name: "S", len: 2 },
///         Token::Str("a"),
///         Token::U8(0),
///         Token::Str("b"),
///         Token::U8(0),
///         Token::StructEnd,
///     ],
/// );
/// ```
#[track_caller]
pub fn assert_ser_tokens<T>(value: &T, tokens: &[Token])
where
    T: ?Sized + Serialize,
{
    let mut ser = Serializer::new(tokens);
    match value.serialize(&mut ser) {
        Ok(()) => {}
        Err(err) => panic!("value failed to serialize: {err}"),
    }

    if ser.remaining() > 0 {
        panic!("{} remaining tokens", ser.remaining());
    }
}

/// Asserts that `value` serializes to the given `tokens`, and then yields
/// `error`.
///
/// ```
/// # use serde_test2::{assert_ser_tokens_error, Token};
/// # use serde::ser::SerializeStruct;
/// # use serde_derive::Serialize;
/// #
/// #[derive(Serialize)]
/// struct Example {
///     inner: FailsToSerialize,
/// }
/// struct FailsToSerialize;
///
/// impl serde::ser::Serialize for FailsToSerialize {
///     fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
///     where
///         S: serde::Serializer,
///     {
///         Err(serde::ser::Error::custom("uh oh!"))
///     }
/// }
///
/// let example = Example {
///     inner: FailsToSerialize,
/// };
/// let expected = &[
///     Token::Struct {
///         name: "Example",
///         len: 1,
///     },
///     Token::Str("inner"),
/// ];
/// assert_ser_tokens_error(&example, expected, "uh oh!");
/// ```
#[track_caller]
pub fn assert_ser_tokens_error<T>(value: &T, tokens: &[Token], error: &str)
where
    T: ?Sized + Serialize,
{
    let mut ser = Serializer::new(tokens);
    match value.serialize(&mut ser) {
        Ok(()) => panic!("value serialized successfully"),
        Err(e) => assert_eq!(e, *error),
    }

    if ser.remaining() > 0 {
        panic!("{} remaining tokens", ser.remaining());
    }
}

/// Asserts that the given `tokens` deserialize into `value`.
///
/// ```
/// # use serde_derive::{Deserialize, Serialize};
/// # use serde_test2::{assert_de_tokens, Token};
/// #
/// #[derive(Serialize, Deserialize, PartialEq, Debug)]
/// struct S {
///     a: u8,
///     b: u8,
/// }
///
/// let s = S { a: 0, b: 0 };
/// assert_de_tokens(
///     &s,
///     &[
///         Token::Struct { name: "S", len: 2 },
///         Token::Str("a"),
///         Token::U8(0),
///         Token::Str("b"),
///         Token::U8(0),
///         Token::StructEnd,
///     ],
/// );
/// ```
#[track_caller]
pub fn assert_de_tokens<'de, T>(value: &T, tokens: &'de [Token])
where
    T: Deserialize<'de> + PartialEq + Debug,
{
    let mut de = Deserializer::new(tokens);
    let mut deserialized_val = match T::deserialize(&mut de) {
        Ok(v) => {
            assert_eq!(v, *value);
            v
        }
        Err(e) => panic!("tokens failed to deserialize: {e}"),
    };
    if de.remaining() > 0 {
        panic!("{} remaining tokens", de.remaining());
    }

    // Do the same thing for deserialize_in_place. This isn't *great* because a
    // no-op impl of deserialize_in_place can technically succeed here. Still,
    // this should catch a lot of junk.
    let mut de = Deserializer::new(tokens);
    match T::deserialize_in_place(&mut de, &mut deserialized_val) {
        Ok(()) => {
            assert_eq!(deserialized_val, *value);
        }
        Err(e) => panic!("tokens failed to deserialize_in_place: {e}"),
    }
    if de.remaining() > 0 {
        panic!("{} remaining tokens", de.remaining());
    }
}

/// Asserts that the given `tokens` yield `error` when deserializing.
///
/// ```
/// # use serde_derive::{Deserialize, Serialize};
/// # use serde_test2::{assert_de_tokens_error, Token};
/// #
/// #[derive(Serialize, Deserialize, PartialEq, Debug)]
/// #[serde(deny_unknown_fields)]
/// struct S {
///     a: u8,
///     b: u8,
/// }
///
/// assert_de_tokens_error::<S>(
///     &[Token::Struct { name: "S", len: 2 }, Token::Str("x")],
///     "unknown field `x`, expected `a` or `b`",
/// );
/// ```
#[track_caller]
pub fn assert_de_tokens_error<'de, T>(tokens: &'de [Token], error: &str)
where
    T: Deserialize<'de>,
{
    let mut de = Deserializer::new(tokens);
    match T::deserialize(&mut de) {
        Ok(_) => panic!("tokens deserialized successfully"),
        Err(e) => assert_eq!(e, *error),
    }

    // There may be one token left if a peek caused the error
    de.next_token_opt();

    if de.remaining() > 0 {
        panic!("{} remaining tokens", de.remaining());
    }
}
