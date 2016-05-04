use std::collections::{BTreeMap, HashMap};
use std::io::{Error as IOError, Read, Write};
use std::iter::FromIterator;

use rustc_serialize::Encodable;
use toml::{encode, Parser, ParserError, Value};


#[derive(Debug)]
enum StoreError {
    IO(IOError),
    Parser(Vec<ParserError>)
}

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

trait Store where Self : Encodable + Sized {
    fn load<T>(reader: &mut Read) -> Result<T, StoreError>
        where T : FromIterator<(String, Value)> {
        let mut s = String::new();
        try!(reader.read_to_string(&mut s));
        let mut p = Parser::new(&s);
        p.parse().map(|x| T::from_iter(x.into_iter()))
                 .ok_or_else(|| StoreError::from(p.errors.clone())
        )
    }

    fn save(self, writer: &mut Write) -> Result<(), IOError> {
        write!(writer, "{}", encode(&self))
    }
}

impl Store for BTreeMap<String, Value> {}
impl Store for HashMap<String, Value> {}


#[test]
fn test() {
    let input = r#"key = "value""#;
    BTreeMap::load::<BTreeMap<String, Value>>(&mut input.as_bytes()).unwrap();
    HashMap::load::<HashMap<String, Value>>(&mut input.as_bytes()).unwrap();
}
