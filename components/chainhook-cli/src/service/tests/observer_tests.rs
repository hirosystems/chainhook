use crate::service::tests::{
    helpers::{
        build_predicates::build_stacks_payload,
        mock_service::{call_ping, call_register_predicate, flush_redis},
    },
    setup_stacks_chainhook_test,
};

use super::helpers::build_predicates::get_random_uuid;

#[tokio::test]
async fn ping_endpoint_returns_metrics() {
    let (
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        _,
    ) = setup_stacks_chainhook_test(1, None, None).await;

    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(Some("devnet"), None, None, None, Some(uuid));
    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let metrics = call_ping(stacks_ingestion_port).await.unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    assert_eq!(metrics.stacks.registered_predicates, 1);
    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}
