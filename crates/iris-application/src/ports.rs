use async_trait::async_trait;
use iris_core::{IrisPersistedState, IrisSnapshot, Result};

#[async_trait]
pub trait StateRepository: Send + Sync {
    async fn load(&self) -> Result<IrisPersistedState>;
    async fn save(&self, state: &IrisPersistedState) -> Result<()>;
}

#[async_trait]
pub trait MqttPublisher: Send + Sync {
    async fn publish_state(&self, snapshot: &IrisSnapshot) -> Result<()>;
}
