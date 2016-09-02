use std::collections::BTreeMap;
use std::io::{Error as IOError, Read, Write};
use std::iter::FromIterator;

use toml::{encode, Parser, ParserError, Value};


#[derive(Debug)]
pub enum StoreError {
    IO(IOError),
    Parser(Vec<ParserError>)
}

pub type StoreResult<T> = Result<T, StoreError>;

impl From<IOError> for StoreError {
    fn from(err: IOError) -> Self {
        StoreError::IO(err)
    }
}

impl From<Vec<ParserError>> for StoreError {
    fn from(err: Vec<ParserError>) -> Self {
        StoreError::Parser(err)
    }
}

pub fn load<F>(reader: &mut F) -> StoreResult<BTreeMap<String, Value>>
    where F : Read {
    let mut s = String::new();
    try!(reader.read_to_string(&mut s));
    let mut p = Parser::new(&s);
    p.parse().map(|x| BTreeMap::from_iter(x.into_iter()))
             .ok_or_else(|| StoreError::from(p.errors.clone())
    )
}

pub fn save<F>(btreemap: BTreeMap<String, Value>, writer: &mut F) -> Result<(), IOError>
    where F : Write {
    write!(writer, "{}", encode(&btreemap))
}


#[test]
fn test() {
    let mut input = r#"key = "value""#.as_bytes();
    load(&mut input).unwrap();
}
