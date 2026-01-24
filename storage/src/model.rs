use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone)]
#[archive(check_bytes)]
#[repr(C)]
pub struct PersistentNode {
    pub id: u64,
    pub uuid: String,
    pub metadata: String,
    pub vector: Vec<f32>,
    pub neighbors: Vec<u64>,
}
