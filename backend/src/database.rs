use anyhow::{Result as AnyResult, anyhow};
use chrono::{DateTime, Utc};
use heed::{
    Database as HeedDatabase, EnvOpenOptions, byteorder::BigEndian, types::{Str, U64}
};
use serde::{Deserialize, Serialize};
use std::{sync::LazyLock, time::Duration};
use crate::{config::Config, controller::SiteState, server::ConnectionMetadata, bincoded::Bincoded};

pub static DATABASE: LazyLock<Database> = LazyLock::new(Database::open);

const LATEST_DB_VERSION: u64 = 0;

#[derive(Serialize, Deserialize)]
struct StateChangeKey {
    pub service: String,
    #[serde(with = "chrono::serde::ts_nanoseconds")]
    pub timestamp: DateTime<Utc>,
}

pub struct Database {
    env: heed::Env,
    connections: HeedDatabase<U64<BigEndian>, Bincoded<Vec<ConnectionMetadata>>>,
    states: HeedDatabase<Bincoded<StateChangeKey>, Bincoded<SiteState>>,
}

impl Database {
    fn open() -> Self {
        let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));
        let config_data = std::fs::read_to_string(config_path).expect("could not read config file");
        let config: Config = toml::from_str(&config_data).expect("could not parse config file");
        let path = config.top_level.database_path();

        std::fs::create_dir_all(path).expect("couldn't create database directory");

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(4096 * 4096) // 16MiB
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

        let states = env
            .create_database(&mut wtxn, Some("states"))
            .expect("couldn't create tokens database");

        wtxn.commit().expect("couldn't commit transaction");

        Database { env, connections, states }
    }

    pub fn put_connection_metadata(&self, at: u64, metadata: ConnectionMetadata) -> AnyResult<()> {
        let mut wtxn = self.env.write_txn()?;

        let mut list = self.connections.get(&wtxn, &at)?.unwrap_or_default();
        list.push(metadata);
        self.connections.put(&mut wtxn, &at, &list)?;

        wtxn.commit()?;
        
        Ok(())
    }

    pub fn get_connection_history(&self, service: Option<&str>, before: Option<u64>, after: Option<u64>, min_results: usize) -> AnyResult<Vec<(u64, ConnectionMetadata)>> {
        let rtxn = self.env.read_txn()?;

        let mut results = Vec::new();

        match (before, after) {
            (Some(before), None) => {
                // Query backwards from 'before' timestamp
                let mut iter = self.connections.rev_range(&rtxn, &(0..before))?;
                while let Some((at, metadatas)) = iter.next().transpose()? {
                    for metadata in metadatas {
                        if service.is_some() && metadata.service.as_deref() != service {
                            continue;
                        }

                        results.push((at, metadata));
                    }

                    if results.len() >= min_results {
                        return Ok(results);
                    }
                }
            }
            (None, Some(after)) => {
                // Query forwards from 'after' timestamp
                let mut iter = self.connections.range(&rtxn, &((after + 1)..u64::MAX))?;
                while let Some((at, metadatas)) = iter.next().transpose()? {
                    for metadata in metadatas {
                        if service.is_some() && metadata.service.as_deref() != service {
                            continue;
                        }

                        results.push((at, metadata));
                    }

                    if results.len() >= min_results {
                        break;
                    }
                }

                // Reverse to show newest first
                results.reverse();
            }
            _ => {
                return Err(anyhow!("Must specify either 'before' or 'after', but not both"));
            }
        }

        Ok(results)
    }

    pub fn get_state_history(&self, service: Option<&str>, before: Option<DateTime<Utc>>, after: Option<DateTime<Utc>>, min_results: usize) -> AnyResult<Vec<(DateTime<Utc>, String, SiteState)>> {
        let rtxn = self.env.read_txn()?;

        let mut results = Vec::new();

        match (before, after) {
            (Some(before), None) => {
                // Query backwards from 'before' timestamp
                if let Some(svc) = service {
                    let min = StateChangeKey {
                        service: svc.to_string(),
                        timestamp: DateTime::from_timestamp_nanos(0),
                    };
                    let max = StateChangeKey {
                        service: svc.to_string(),
                        timestamp: before,
                    };
                    let mut iter = self.states.rev_range(&rtxn, &(min..max))?;
                    
                    while let Some((key, state)) = iter.next().transpose()? {
                        results.push((key.timestamp, key.service, state));
                        
                        if results.len() >= min_results {
                            return Ok(results);
                        }
                    }
                } else {
                    // Query all services
                    let min = StateChangeKey {
                        service: String::new(),
                        timestamp: DateTime::from_timestamp_nanos(0),
                    };
                    let max = StateChangeKey {
                        service: "\u{10FFFF}".repeat(100), // Max unicode string
                        timestamp: before,
                    };
                    let mut iter = self.states.rev_range(&rtxn, &(min..max))?;
                    
                    while let Some((key, state)) = iter.next().transpose()? {
                        results.push((key.timestamp, key.service, state));
                        
                        if results.len() >= min_results {
                            return Ok(results);
                        }
                    }
                }
            }
            (None, Some(after)) => {
                // Query forwards from 'after' timestamp
                if let Some(svc) = service {
                    let min = StateChangeKey {
                        service: svc.to_string(),
                        timestamp: after,
                    };
                    let max = StateChangeKey {
                        service: svc.to_string(),
                        timestamp: DateTime::from_timestamp_nanos(i64::MAX),
                    };
                    let mut iter = self.states.range(&rtxn, &(min..max))?;
                    
                    while let Some((key, state)) = iter.next().transpose()? {
                        if key.timestamp > after {
                            results.push((key.timestamp, key.service, state));
                        }
                        
                        if results.len() >= min_results {
                            break;
                        }
                    }
                } else {
                    // Query all services
                    let min = StateChangeKey {
                        service: String::new(),
                        timestamp: after,
                    };
                    let max = StateChangeKey {
                        service: "\u{10FFFF}".repeat(100), // Max unicode string
                        timestamp: DateTime::from_timestamp_nanos(i64::MAX),
                    };
                    let mut iter = self.states.range(&rtxn, &(min..max))?;
                    
                    while let Some((key, state)) = iter.next().transpose()? {
                        if key.timestamp > after {
                            results.push((key.timestamp, key.service, state));
                        }
                        
                        if results.len() >= min_results {
                            break;
                        }
                    }
                }

                // Reverse to show newest first
                results.reverse();
            }
            _ => {
                return Err(anyhow!("Must specify either 'before' or 'after', but not both"));
            }
        }

        Ok(results)
    }

    pub fn get_start_duration_estimate(&self, name: &str, percentile: usize) -> AnyResult<Duration> {
        let rtxn = self.env.read_txn()?;

        let min = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(0),
        };
        let max = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(i64::MAX),
        };
        let mut iter = self.states.rev_range(&rtxn, &(min..=max))?;

        let mut values = Vec::new();
        let mut last_started_time = None;
        while let Some((key, state)) = iter.next().transpose()? {
            match state {
                SiteState::Up => {
                    last_started_time = Some(key.timestamp);
                }
                SiteState::Starting => {
                    if let Some(started_time) = last_started_time.take() {
                        let duration = started_time.signed_duration_since(key.timestamp);
                        if let Ok(d) = duration.to_std() {
                            values.push(d);
                        }
                    }
                }
                _ => {
                    last_started_time = None;
                }
            }
        }

        if values.is_empty() {
            return Err(anyhow!("No durations stored"));
        }

        let idx = (values.len() * percentile) / 100;

        Ok(values[idx])
    }

    pub fn update_state(&self, name: &str, state: SiteState) -> AnyResult<()> {
        let mut wtxn = self.env.write_txn()?;

        let key = StateChangeKey {
            service: name.to_string(),
            timestamp: Utc::now(),
        };

        self.states.put(&mut wtxn, &key, &state)?;
        wtxn.commit()?;

        Ok(())
    }

    pub fn get_last_state(&self, name: &str) -> AnyResult<(SiteState, DateTime<Utc>)> {
        let rtxn = self.env.read_txn()?;

        let min = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(0),
        };
        let max = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(i64::MAX),
        };
        let mut iter = self.states.rev_range(&rtxn, &(min..=max))?;

        if let Some((key, state)) = iter.next().transpose()? {
            Ok((state, key.timestamp))
        } else {
            Err(anyhow!("No state found"))
        }
    }
}
