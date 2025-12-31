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
                .map_size(10 * 4096 * 4096) // 160MiB
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

    pub fn get_state_history(&self, service: &str, before: Option<DateTime<Utc>>, after: Option<DateTime<Utc>>, min_results: usize) -> AnyResult<Vec<(DateTime<Utc>, DateTime<Utc>, SiteState)>> {
        let rtxn = self.env.read_txn()?;

        let mut raw_results = Vec::new();

        match (before, after) {
            (Some(before), None) => {
                // Query backwards from 'before' timestamp
                let min = StateChangeKey {
                    service: service.to_string(),
                    timestamp: DateTime::from_timestamp_nanos(0),
                };
                let max = StateChangeKey {
                    service: service.to_string(),
                    timestamp: before,
                };
                let mut iter = self.states.rev_range(&rtxn, &(min..max))?;
                
                let mut deduplicated_count = 0;
                let mut last_state: Option<SiteState> = None;
                
                while let Some((key, state)) = iter.next().transpose()? {
                    raw_results.push((key.timestamp, state));
                    
                    // Count deduplicated entries (state changes)
                    match &last_state {
                        Some(last_site_state) if last_site_state == &state => {
                            // Same state continues, don't increment count
                        }
                        _ => {
                            // State changed or first entry
                            deduplicated_count += 1;
                            last_state = Some(state);
                        }
                    }
                    
                    if deduplicated_count >= min_results {
                        break;
                    }
                }

                raw_results.reverse();
            }
            (None, Some(after)) => {
                // Query forwards from 'after' timestamp
                let min = StateChangeKey {
                    service: service.to_string(),
                    timestamp: after,
                };
                let max = StateChangeKey {
                    service: service.to_string(),
                    timestamp: DateTime::from_timestamp_nanos(i64::MAX),
                };
                let mut iter = self.states.range(&rtxn, &(min..max))?;
                
                let mut deduplicated_count = 0;
                let mut last_state: Option<SiteState> = None;
                
                while let Some((key, state)) = iter.next().transpose()? {
                    if key.timestamp > after {
                        raw_results.push((key.timestamp, state));
                        
                        // Count deduplicated entries (state changes)
                        match &last_state {
                            Some(last_site_state) if last_site_state == &state => {
                                // Same state continues, don't increment count
                            }
                            _ => {
                                // State changed or first entry
                                deduplicated_count += 1;
                                last_state = Some(state);
                            }
                        }
                        
                        if deduplicated_count >= min_results {
                            break;
                        }
                    }
                }
            }
            _ => {
                return Err(anyhow!("Must specify either 'before' or 'after', but not both"));
            }
        }

        // Convert raw results into ranges
        let mut ranges = Vec::new();
        let mut current_range: Option<(DateTime<Utc>, SiteState)> = None;

        for (timestamp, state) in raw_results {
            match &current_range {
                None => {
                    // First state
                    current_range = Some((timestamp, state));
                }
                Some((start_time, current_state)) => {
                    if current_state == &state {
                        // Same state continues, extend the range (do nothing, we'll use timestamp as end when it changes)
                    } else {
                        // State changed, emit the previous range
                        ranges.push((*start_time, timestamp, *current_state));
                        current_range = Some((timestamp, state));
                    }
                }
            }
        }

        // Emit the last state (it's still ongoing, so end_time = now)
        if let Some((start_time, state)) = current_range {
            ranges.push((start_time, Utc::now(), state));
        }

        Ok(ranges)
    }

    pub fn get_state_history_since(&self, service: &str, since: DateTime<Utc>) -> AnyResult<Vec<(DateTime<Utc>, SiteState)>> {
        let rtxn = self.env.read_txn()?;

        let mut results = Vec::new();

        let min = StateChangeKey {
            service: service.to_string(),
            timestamp: DateTime::from_timestamp_nanos(0),
        };
        let max = StateChangeKey {
            service: service.to_string(),
            timestamp: DateTime::from_timestamp_nanos(i64::MAX),
        };
        let mut iter = self.states.rev_range(&rtxn, &(min..max))?;
        
        while let Some((key, state)) = iter.next().transpose()? {
            if key.timestamp < since {
                results.push((since, state));
                break;
            } else {
                results.push((key.timestamp, state));
            }
        }

        results.reverse();

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

    /// Try to update state only if it's not already in the specified state or states.
    /// Returns true if the state was updated, false if it was already in one of the excluded states.
    pub fn try_update_state(&self, name: &str, new_state: SiteState, exclude_states: &[SiteState]) -> AnyResult<bool> {
        let mut wtxn = self.env.write_txn()?;

        // Check current state within the transaction
        let min = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(0),
        };
        let max = StateChangeKey {
            service: name.to_string(),
            timestamp: DateTime::from_timestamp_nanos(i64::MAX),
        };
        let mut iter = self.states.rev_range(&wtxn, &(min..=max))?;

        if let Some((_, current_state)) = iter.next().transpose()? {
            // Check if current state is in the exclude list
            if exclude_states.contains(&current_state) {
                return Ok(false);
            }
        }

        // State is not excluded, proceed with update
        drop(iter);
        let key = StateChangeKey {
            service: name.to_string(),
            timestamp: Utc::now(),
        };

        self.states.put(&mut wtxn, &key, &new_state)?;
        wtxn.commit()?;

        Ok(true)
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

        // Get the most recent state
        if let Some((key, state)) = iter.next().transpose()? {
            let current_state = state;
            let mut start_time = key.timestamp;
            
            // Iterate backwards to find when this state actually started
            while let Some((key, prev_state)) = iter.next().transpose()? {
                if prev_state == current_state {
                    // Same state continues further back
                    start_time = key.timestamp;
                } else {
                    // State changed, we found the start
                    break;
                }
            }
            
            Ok((current_state, start_time))
        } else {
            Err(anyhow!("No state found"))
        }
    }
}
