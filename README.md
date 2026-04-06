# serde_test2

This crate provides a convenient concise way to write unit tests for implementations of `Serialize`
and `Deserialize`.

The `Serialize` impl for a value can be characterized by the sequence of `Serializer` calls that are
made in the course of serializing the value, so `serde_test` provides a [`Token`] abstraction which
corresponds roughly to `Serializer` method calls. There is an `assert_ser_tokens` function to test
that a value serializes to a particular sequence of method calls, an `assert_de_tokens` function to
test that a value can be deserialized from a particular sequence of method calls, and an
`assert_tokens` function to test both directions. There are also functions to test expected failure
conditions.

Here is an example from the `linked-hash-map` crate.

```rust
use linked_hash_map::LinkedHashMap;
use serde_test2::{assert_tokens, Token};

#[test]
fn test_ser_de_empty() {
    let map = LinkedHashMap::<char, u32>::new();

    assert_tokens(
        &map,
        &[
            Token::Map { len: Some(0) },
            Token::MapEnd,
        ],
    );
}

#[test]
fn test_ser_de() {
    let mut map = LinkedHashMap::new();
    map.insert('b', 20);
    map.insert('a', 10);
    map.insert('c', 30);

    assert_tokens(
        &map,
        &[
            Token::Map { len: Some(3) },
            Token::Char('b'),
            Token::I32(20),
            Token::Char('a'),
            Token::I32(10),
            Token::Char('c'),
            Token::I32(30),
            Token::MapEnd,
        ],
    );
}
```

<br>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version 2.0</a> or <a
href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
</sub>
