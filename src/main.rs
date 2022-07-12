extern crate leveldb;
extern crate tempdir;

use leveldb::database::Database;
use leveldb::iterator::Iterable;
use leveldb::kv::KV;
use leveldb::options::{Options, ReadOptions, WriteOptions};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use tempdir::TempDir;

// db-key 0.0.5 only impl Key for i32, wrap for orphan rule
#[derive(Debug, PartialEq)]
struct EncodedKey<T: 'static> {
    pub inner: Vec<u8>,
    phantom: PhantomData<T>,
}

impl<T> db_key::Key for EncodedKey<T> {
    fn from_u8(key: &[u8]) -> Self {
        EncodedKey {
            inner: key.into(),
            phantom: PhantomData,
        }
    }
    fn as_slice<S, F: Fn(&[u8]) -> S>(&self, f: F) -> S {
        f(&self.inner)
    }
}

impl<T> From<Vec<u8>> for EncodedKey<T> {
    fn from(inner: Vec<u8>) -> Self {
        EncodedKey {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, T> From<&[u8]> for EncodedKey<T> {
    fn from(v: &[u8]) -> Self {
        EncodedKey {
            inner: v.into(),
            phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Command {
    pub executable: u8,
    pub args: Vec<String>,
    pub current_dir: Option<String>,
}

trait KeyOrm<'a>: Sized {
    type KeyType;
    type KeyTypeRef: Serialize + 'a;

    fn encode_key(
        key: &Self::KeyTypeRef,
    ) -> std::result::Result<EncodedKey<Self>, Box<dyn std::error::Error>>;
    fn decode_key(
        data: &EncodedKey<Self>,
    ) -> std::result::Result<Self::KeyType, Box<dyn std::error::Error>>;
    fn key(&self) -> std::result::Result<EncodedKey<Self>, Box<dyn std::error::Error>>;
}

impl<'a> KeyOrm<'a> for Command {
    type KeyType = (u8, Vec<String>);
    type KeyTypeRef = (&'a u8, &'a Vec<String>);

    fn encode_key(
        key: &Self::KeyTypeRef,
    ) -> std::result::Result<EncodedKey<Self>, Box<dyn std::error::Error>> {
        bincode::serialize(key)
            .map(EncodedKey::from)
            .map_err(|e| e.into())
    }

    fn decode_key(
        data: &EncodedKey<Self>,
    ) -> std::result::Result<Self::KeyType, Box<dyn std::error::Error>> {
        bincode::deserialize(&data.inner).map_err(|e| e.into())
    }

    fn key(&self) -> std::result::Result<EncodedKey<Self>, Box<dyn std::error::Error>> {
        Self::encode_key(&(&self.executable, &self.args))
    }
}

trait KVOrm<'a>: KeyOrm<'a> {
    fn encode(&self) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn decode(data: &[u8]) -> std::result::Result<Self, Box<dyn std::error::Error>>;
    fn put_sync(
        &self,
        db: &Database<EncodedKey<Self>>,
        sync: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
    fn put(
        &self,
        db: &Database<EncodedKey<Self>>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.put_sync(db, false)
    }
    fn get_with_option(
        db: &Database<EncodedKey<Self>>,
        options: ReadOptions<'a, EncodedKey<Self>>,
        key: &EncodedKey<Self>,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>>;
    fn get(
        db: &Database<EncodedKey<Self>>,
        key: &EncodedKey<Self>,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        Self::get_with_option(db, ReadOptions::new(), key)
    }
    fn delete(
        db: &Database<EncodedKey<Self>>,
        sync: bool,
        key: &EncodedKey<Self>,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

impl<'a> KVOrm<'a> for Command {
    fn encode(&self) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
        bincode::serialize(self).map_err(|e| e.into())
    }

    fn decode(data: &[u8]) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        bincode::deserialize(data).map_err(|e| e.into())
    }

    fn put_sync(
        &self,
        db: &Database<EncodedKey<Self>>,
        sync: bool,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let key = self.key()?;
        let value = self.encode()?;
        db.put(WriteOptions { sync }, key, &value)
            .map_err(|e| e.into())
    }

    fn get_with_option(
        db: &Database<EncodedKey<Self>>,
        options: ReadOptions<'a, EncodedKey<Self>>,
        key: &EncodedKey<Self>,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if let Some(data) = db.get(options, key)? {
            Ok(Some(bincode::deserialize(&data)?))
        } else {
            Ok(None)
        }
    }

    fn delete(
        db: &Database<EncodedKey<Self>>,
        sync: bool,
        key: &EncodedKey<Self>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        db.delete(WriteOptions { sync }, key).map_err(|e| e.into())
    }
}

fn main() {
    let cmd = Command {
        executable: 1,
        args: vec!["arg1".into(), "arg2".into(), "arg3".into()],
        current_dir: Some("\\dir".into()),
    };
    let key = cmd.key().unwrap();

    let tempdir = TempDir::new("demo").unwrap();
    let path = tempdir.path();

    let mut options = Options::new();
    options.create_if_missing = true;
    dbg!(&path);
    let database = match Database::open(path, options) {
        Ok(db) => db,
        Err(e) => {
            panic!("failed to open database: {:?}", e)
        }
    };

    match cmd.put(&database) {
        Ok(_) => (),
        Err(e) => {
            panic!("failed to write to database: {:?}", e)
        }
    };

    let res = Command::get(&database, &key).unwrap();
    // dbg!(&res);
    assert_eq!(res, Some(cmd.clone()));

    let read_opts = ReadOptions::new();
    let mut iter = database.iter(read_opts);
    let entry = iter
        .next()
        .map(|(k, v)| {
            (
                Command::decode_key(&k).unwrap(),
                Command::decode(&v).unwrap(),
            )
        })
        .unwrap();
    dbg!(&entry);
    assert_eq!(entry, ((cmd.executable, cmd.args.clone()), cmd));

    Command::delete(&database, false, &key).unwrap();

    assert!(Command::get(&database, &key).unwrap().is_none());
}
