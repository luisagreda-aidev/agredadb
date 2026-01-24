//! WAL entry types

use serde::{Deserialize, Serialize};

/// WAL entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    /// Insert operation
    Insert {
        table: String,
        data: Vec<u8>,
    },
    /// Delete operation
    Delete {
        table: String,
        id: String,
    },
    /// Update operation
    Update {
        table: String,
        id: String,
        data: Vec<u8>,
    },
}

impl WalEntry {
    /// Serialize entry to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize entry from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let entry = WalEntry::Insert {
            table: "test".to_string(),
            data: vec![1, 2, 3],
        };

        let bytes = entry.to_bytes().unwrap();
        let deserialized = WalEntry::from_bytes(&bytes).unwrap();

        match deserialized {
            WalEntry::Insert { table, data } => {
                assert_eq!(table, "test");
                assert_eq!(data, vec![1, 2, 3]);
            }
            _ => panic!("Wrong entry type"),
        }
    }
}
