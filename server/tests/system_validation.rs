#[cfg(test)]
mod tests {
    use agredadb_server::shard::Shard;
    use agredadb_dbms::{InternalCommand, SqlEngine, AgredaTableProvider};
    use agredadb_wal::{WalManager, WalEntry, SyncPolicy};
    use tempfile::tempdir;
    use std::sync::Arc;
    use arrow::datatypes::{DataType, Field, Schema};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_full_io_pipeline_no_mocks() {
        let _dir = tempdir().unwrap();
        let (_tx, rx) = flume::unbounded::<InternalCommand>();
        
        let mut shard = Shard::new(0, rx).await;
        
        let mut jsons = Vec::new();
        for i in 0..10 {
            let vector: Vec<f32> = vec![0.1 * (i as f32); 128];
            jsons.push(format!("{{\"id\": \"test_{}\", \"vector\": {:?}}}", i, vector));
        }
        
        shard.insert_to_memory(jsons).await;
        
        let mut memory_table = shard.memory_table.lock().await;
        shard.flush_to_disk(&mut memory_table).await;
        
        let query: Vec<f32> = vec![0.5; 128];
        let disk_results = shard.disk_index.search(&query, 5);
        
        println!("Found {} results on disk", disk_results.len());
    }

    #[tokio::test]
    async fn test_sql_engine_data_flow() {
        // Usar un timeout para no bloquear el CI
        let result = timeout(Duration::from_secs(5), async {
            let sql_engine = SqlEngine::new();
            
            // En este test NO pasamos canales de shards para disparar la ruta de "Empty Scan"
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
        
        let mut wal = WalManager::new(&wal_path, SyncPolicy::EveryWrite).await.unwrap();
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
}