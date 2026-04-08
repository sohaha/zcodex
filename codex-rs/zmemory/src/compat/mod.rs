mod browse;
mod contracts;
mod maintenance;
mod review;

pub use contracts::AdminDoctorResponse;
pub use contracts::AdminStatsResponse;
pub use contracts::BrowseNodePayload;
pub use contracts::DeleteOrphanResponse;
pub use contracts::DomainSummary;
pub use contracts::ErrorDetailResponse;
pub use contracts::GlossaryListResponse;
pub use contracts::HealthResponse;
pub use contracts::OrphanDetailResponse;
pub use contracts::OrphanListItemResponse;
pub use contracts::RebuildSearchResponse;
pub use contracts::ReviewDeprecatedResponse;
pub use contracts::ReviewDiffResponse;
pub use contracts::ReviewGroupItemResponse;
pub use contracts::SuccessMessageResponse;
pub use contracts::UpdateNodeResponse;

use crate::ZmemoryConfig;
use crate::repository::ZmemoryRepository;
use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct CompatService {
    base_config: ZmemoryConfig,
}

impl CompatService {
    pub fn new(base_config: ZmemoryConfig) -> Self {
        Self { base_config }
    }

    pub fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            database: "connected".to_string(),
        }
    }

    fn config_for(&self, namespace: Option<&str>) -> ZmemoryConfig {
        self.base_config.with_namespace(
            namespace
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
        )
    }

    fn connect(&self, namespace: Option<&str>) -> Result<(Connection, ZmemoryConfig)> {
        let config = self.config_for(namespace);
        let conn = ZmemoryRepository::new(config.clone()).connect()?;
        Ok((conn, config))
    }
}
