use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub creation_date: chrono::DateTime<chrono::Utc>,
}
