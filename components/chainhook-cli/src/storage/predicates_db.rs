use chainhook_sdk::chainhooks::database::PredicatesDatabaseAccess;
use chainhook_sdk::chainhooks::stacks::StacksChainhookInstance;
use chainhook_sdk::chainhooks::types::PredicateStatus;
use chainhook_sdk::chainhooks::{bitcoin::BitcoinChainhookInstance, types::ChainhookInstance};
use chainhook_sdk::utils::Context;
use redis::Connection;

use crate::service::http_api::get_entries_from_predicates_db;

pub struct RedisPredicatesDatabaseAccess {
    redis_uri: String,
}

impl RedisPredicatesDatabaseAccess {
    pub fn new(redis_uri: String) -> Self {
        Self { redis_uri }
    }

    fn get_predicate_db_conn(&self) -> Result<Connection, String> {
        let client = redis::Client::open(self.redis_uri.as_str())
            .map_err(|e| format!("unable to connect to redis: {}", e))?;
        let conn = client
            .get_connection()
            .map_err(|e| format!("unable to connect to redis: {}", e))?;
        Ok(conn)
    }
}

impl PredicatesDatabaseAccess for RedisPredicatesDatabaseAccess {

    fn get_active_stacks_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(StacksChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let entries = get_entries_from_predicates_db(&mut conn, ctx)?;
        let stacks_predicates = entries
            .into_iter()
            .filter_map(|(spec, status)| match spec {
                ChainhookInstance::Stacks(stacks_spec) => {
                    if stacks_spec.enabled
                        && !matches!(
                            status,
                            PredicateStatus::ConfirmedExpiration(_)
                                | PredicateStatus::UnconfirmedExpiration(_)
                                | PredicateStatus::Interrupted(_)
                        )
                    {
                        Some((stacks_spec, status))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        Ok(stacks_predicates)
    }

    fn get_active_bitcoin_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(BitcoinChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let entries = get_entries_from_predicates_db(&mut conn, ctx)?;
        let bitcoin_predicates = entries
            .into_iter()
            .filter_map(|(spec, status)| match spec {
                ChainhookInstance::Bitcoin(bitcoin_spec) => {
                    if bitcoin_spec.enabled && !matches!(status, PredicateStatus::ConfirmedExpiration(_)
                                | PredicateStatus::UnconfirmedExpiration(_)
                                | PredicateStatus::Interrupted(_)
                        )
                    {
                        Some((bitcoin_spec, status))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        Ok(bitcoin_predicates)
    }
}
