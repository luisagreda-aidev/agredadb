#[cfg(test)]
mod tests {
    use agredadb_server::shard::Shard;
    use agredadb_server::durability::DurabilityMode;
    use agredadb_dbms::{InternalCommand, SqlEngine, AgredaTableProvider};
    use agredadb_wal::{WalManager, WalEntry, SyncPolicy};
    use agredadb_security::SecurityManager;
    use tempfile::tempdir;
    use std::sync::Arc;
    use arrow::datatypes::{DataType, Field, Schema};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_insert_to_memory() {
        let (_tx, rx) = flume::unbounded::<InternalCommand>();

        let mut shard = Shard::new(0, rx).await;

        let mut jsons = Vec::new();
        for i in 0..10 {
            let vector: Vec<f32> = vec![0.1 * (i as f32); 128];
            jsons.push(format!("{{\"id\": \"test_{}\", \"vector\": {:?}}}", i, vector));
        }

        shard.insert_to_memory(jsons).await;
    }

    #[tokio::test]
    async fn test_sql_engine_data_flow() {
        let result = timeout(Duration::from_secs(5), async {
            let sql_engine = SqlEngine::new();

            let schema = Arc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("metadata", DataType::Utf8, false),
            ]));

            let provider = Arc::new(AgredaTableProvider::new(schema.clone(), vec![]));
            sql_engine.register_native_table("users", provider).unwrap();

            let results = sql_engine.query("SELECT * FROM users").await.unwrap();
            results
        }).await;

        match result {
            Ok(res) => assert!(res.is_empty(), "Expected empty results for no-shard scan"),
            Err(_) => panic!("Test timed out! Busy loop or channel hang detected."),
        }
    }

    #[tokio::test]
    async fn test_wal_recovery_integrity() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        let wal = WalManager::new(&wal_path, SyncPolicy::EveryWrite).await.unwrap();
        wal.append(WalEntry::Insert {
            table: "users".to_string(),
            data: b"{\"id\": \"1\", \"name\": \"Luis\"}".to_vec(),
        }).await.unwrap();

        let wal_recovered = WalManager::new(&wal_path, SyncPolicy::EveryWrite).await.unwrap();
        let entries = wal_recovered.replay().await.unwrap();

        assert_eq!(entries.len(), 1);
        if let WalEntry::Insert { table, .. } = &entries[0] {
            assert_eq!(table, "users");
        }
    }

    #[tokio::test]
    async fn test_insert_and_search_roundtrip() {
        let _ = std::fs::remove_dir_all("./data/shard_7777");
        let result = timeout(Duration::from_secs(10), async {
            let (tx, rx) = flume::unbounded::<InternalCommand>();
            let mut shard = Shard::new_with_mode(7777, rx, DurabilityMode::Memory).await;
            let shard_handle = tokio::spawn(async move { shard.run().await; });

            // Insert records with known vectors
            for i in 0..5 {
                let vector: Vec<f32> = vec![0.1 * (i as f32 + 1.0); 128];
                let json = format!(
                    r#"{{"id": "item_{}", "data": "record {}", "vector": {:?}}}"#,
                    i, i, vector
                );
                let (resp_tx, resp_rx) = flume::bounded(1);
                tx.send(InternalCommand::Insert {
                    _table: "test".into(),
                    json,
                    responder: Some(resp_tx),
                }).unwrap();
                let success = resp_rx.recv_async().await.unwrap();
                assert!(success, "Insert should succeed");
            }

            // Search with a vector similar to item_2 (vector of 0.3s)
            let query_vector: Vec<f32> = vec![0.3; 128];
            let (search_tx, search_rx) = flume::bounded(1);
            tx.send(InternalCommand::Search {
                _table: "test".into(),
                vector: query_vector,
                limit: 3,
                responder: search_tx,
            }).unwrap();
            let results = search_rx.recv_async().await.unwrap();

            assert!(!results.is_empty(), "Search should return results");
            assert!(results.len() <= 3, "Should respect limit");
            assert!(results[0].score > 0.0, "Best result should have positive score");

            drop(tx);
            let _ = shard_handle.await;
        }).await;

        let _ = std::fs::remove_dir_all("./data/shard_7777");
        assert!(result.is_ok(), "Test timed out");
    }

    #[tokio::test]
    async fn test_shard_wal_replay_recovery() {
        let shard_id = 8888;
        let data_path = format!("./data/shard_{}", shard_id);
        let _ = std::fs::remove_dir_all(&data_path);

        let result = timeout(Duration::from_secs(15), async {
            // Phase 1: Create shard, insert data (writes to WAL), then drop
            {
                let (tx, rx) = flume::unbounded::<InternalCommand>();
                let mut shard = Shard::new_with_mode(shard_id, rx, DurabilityMode::Strict).await;
                let shard_handle = tokio::spawn(async move { shard.run().await; });

                for i in 0..3 {
                    let vector: Vec<f32> = vec![0.5 + (i as f32 * 0.1); 128];
                    let json = format!(
                        r#"{{"id": "wal_item_{}", "data": "wal record {}", "vector": {:?}}}"#,
                        i, i, vector
                    );
                    let (resp_tx, resp_rx) = flume::bounded(1);
                    tx.send(InternalCommand::Insert {
                        _table: "test".into(),
                        json,
                        responder: Some(resp_tx),
                    }).unwrap();
                    let success = resp_rx.recv_async().await.unwrap();
                    assert!(success, "Insert should succeed with Strict WAL");
                }

                drop(tx);
                let _ = shard_handle.await;
            }

            // Phase 2: Recreate shard — WAL replay should recover data
            {
                let (tx, rx) = flume::unbounded::<InternalCommand>();
                let mut shard = Shard::new_with_mode(shard_id, rx, DurabilityMode::Strict).await;
                let shard_handle = tokio::spawn(async move { shard.run().await; });

                // Search for recovered data
                let (search_tx, search_rx) = flume::bounded(1);
                tx.send(InternalCommand::Search {
                    _table: "test".into(),
                    vector: vec![0.5; 128],
                    limit: 10,
                    responder: search_tx,
                }).unwrap();
                let results = search_rx.recv_async().await.unwrap();

                assert_eq!(results.len(), 3, "WAL replay should recover all 3 entries");

                drop(tx);
                let _ = shard_handle.await;
            }
        }).await;

        let _ = std::fs::remove_dir_all(&data_path);
        assert!(result.is_ok(), "WAL replay test timed out");
    }

    #[tokio::test]
    async fn test_auth_create_authenticate_verify() {
        let security = SecurityManager::new("test_secret_key_for_integration");

        // Create user
        let user = security.create_user("testuser", "testpass123", "tenant_a")
            .await
            .unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.tenant, "tenant_a");

        // Authenticate and get token
        let token = security.authenticate("testuser", "testpass123")
            .await
            .unwrap();
        assert!(!token.is_empty());

        // Verify token returns correct claims
        let claims = security.verify_token(&token).unwrap();
        assert_eq!(claims.username, "testuser");
        assert_eq!(claims.tenant, "tenant_a");
        assert_eq!(claims.sub, user.id);

        // Wrong password should fail
        let result = security.authenticate("testuser", "wrong_password").await;
        assert!(result.is_err(), "Wrong password should fail");

        // Invalid token should fail
        let result = security.verify_token("not.a.valid.token");
        assert!(result.is_err(), "Invalid token should fail");

        // Duplicate user creation should fail
        let result = security.create_user("testuser", "otherpass", "tenant_b").await;
        assert!(result.is_err(), "Duplicate username should fail");
    }

    #[tokio::test]
    async fn test_insert_batch_and_search() {
        let _ = std::fs::remove_dir_all("./data/shard_7778");
        let result = timeout(Duration::from_secs(10), async {
            let (tx, rx) = flume::unbounded::<InternalCommand>();
            let mut shard = Shard::new_with_mode(7778, rx, DurabilityMode::Memory).await;
            let shard_handle = tokio::spawn(async move { shard.run().await; });

            // Insert batch of records
            let mut jsons = Vec::new();
            for i in 0..20 {
                let vector: Vec<f32> = vec![i as f32 / 20.0; 128];
                jsons.push(format!(
                    r#"{{"id": "batch_{}", "data": "batch record {}", "vector": {:?}}}"#,
                    i, i, vector
                ));
            }

            let (resp_tx, resp_rx) = flume::bounded(1);
            tx.send(InternalCommand::InsertBatch {
                _table: "test".into(),
                jsons,
                responder: Some(resp_tx),
            }).unwrap();
            let success = resp_rx.recv_async().await.unwrap();
            assert!(success, "Batch insert should succeed");

            // Search should find results
            let (search_tx, search_rx) = flume::bounded(1);
            tx.send(InternalCommand::Search {
                _table: "test".into(),
                vector: vec![0.5; 128],
                limit: 5,
                responder: search_tx,
            }).unwrap();
            let results = search_rx.recv_async().await.unwrap();

            assert_eq!(results.len(), 5, "Should return exactly limit results");
            // Results should be sorted by score descending
            for i in 1..results.len() {
                assert!(results[i-1].score >= results[i].score, "Results should be sorted by score");
            }

            drop(tx);
            let _ = shard_handle.await;
        }).await;

        let _ = std::fs::remove_dir_all("./data/shard_7778");
        assert!(result.is_ok(), "Test timed out");
    }
}
