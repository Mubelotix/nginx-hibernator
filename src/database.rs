use anyhow::{Result as AnyResult, anyhow};
use heed::{
    byteorder::BigEndian,
    types::{SerdeBincode as Bincoded, Str, U64},
    Database as HeedDatabase, EnvOpenOptions,
};
use std::{path::Path, sync::LazyLock, time::Duration};

use crate::server::ConnectionMetadata;

pub static DATABASE: LazyLock<Database> = LazyLock::new(|| Database::open("data.mdb"));

const LATEST_DB_VERSION: u64 = 0;

pub struct Database {
    env: heed::Env,
    connections: HeedDatabase<U64<BigEndian>, Bincoded<Vec<ConnectionMetadata>>>,
    start_durations: HeedDatabase<Str, Bincoded<Vec<Duration>>>,
}

impl Database {
    fn open(path: impl AsRef<Path>) -> Self {
        std::fs::create_dir_all(&path).expect("couldn't create database directory");

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(16 * 4096) // 16 pages
                .max_dbs(16)
                .open(path)
                .expect("couldn't open database")
        };

        let mut wtxn = env.write_txn().expect("couldn't create write txn");

        // Upgrade database

        let version_db: HeedDatabase<Str, U64<BigEndian>> = env
            .create_database(&mut wtxn, None)
            .expect("couldn't create version database");

        let version = version_db
            .get(&wtxn, "version")
            .expect("couldn't read database version");

        match version {
            Some(LATEST_DB_VERSION) | None => {}
            Some(unsupported_version) => {
                panic!("cannot upgrade from unsupported database version {unsupported_version}")
            }
        }

        if version != Some(LATEST_DB_VERSION) {
            version_db
                .put(&mut wtxn, "version", &LATEST_DB_VERSION)
                .expect("couldn't update database version");
        }

        // Open main databases

        let connections = env
            .create_database(&mut wtxn, Some("connections"))
            .expect("couldn't create tokens database");

        let start_durations = env
            .create_database(&mut wtxn, Some("connections"))
            .expect("couldn't create tokens database");

        wtxn.commit().expect("couldn't commit transaction");

        Database { env, connections, start_durations }
    }

    pub fn put_connection_metadata(&self, at: u64, metadata: ConnectionMetadata) -> AnyResult<()> {
        let mut wtxn = self.env.write_txn()?;

        let mut list = self.connections.get(&wtxn, &at)?.unwrap_or_default();
        list.push(metadata);
        self.connections.put(&mut wtxn, &at, &list)?;

        wtxn.commit()?;
        
        Ok(())
    }

    pub fn get_start_duration_estimate(&self, name: &str, percentile: usize) -> AnyResult<Duration> {
        let rtxn = self.env.read_txn()?;

        let values = self.start_durations.get(&rtxn, name)?.ok_or(anyhow!("No durations stored"))?;
        let idx = (values.len() * percentile) / 100;

        Ok(values[idx])
    }

    pub fn put_start_duration(&self, name: &str, value: Duration, sample_count: usize) -> AnyResult<()> {
        let mut wtxn = self.env.write_txn()?;

        if sample_count == 0 {
            self.start_durations.delete(&mut wtxn, name)?;
            return Ok(())
        }

        let mut values = self.start_durations.get(&wtxn, name)?.unwrap_or_default();
        values.push(value);
        while values.len() > sample_count {
            values.remove(0);
        }

        self.start_durations.put(&mut wtxn, name, &values)?;
        wtxn.commit()?;

        Ok(())
    }
}
