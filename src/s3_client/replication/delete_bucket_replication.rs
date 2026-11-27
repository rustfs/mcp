// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::super::S3Client;
use super::get_bucket_replication::BucketReplicationConfig;

/// What the removal of a replication configuration left behind, as the
/// storage backend reports the bucket afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicationRemoval {
    /// The bucket has no replication configuration any more.
    Removed,
    /// The storage backend still reports these rules. A removal drops the
    /// rules written through S3 only: RustFS keeps the rules of a site
    /// replication, and they stay in effect.
    RulesRemain(BucketReplicationConfig),
    /// Whether anything remains could not be read; the text says why.
    Unconfirmed(String),
}

impl S3Client {
    /// Remove a bucket's replication configuration, then read it back to
    /// report what the storage backend kept.
    pub async fn delete_bucket_replication(&self, bucket_name: &str) -> Result<ReplicationRemoval> {
        info!(
            "Removing the replication configuration of bucket '{}'",
            bucket_name
        );

        self.client
            .delete_bucket_replication()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to remove the replication configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Replication configuration of bucket '{}' removed",
            bucket_name
        );

        // The request succeeded, but it need not have removed everything:
        // the rules a site replication manages survive it on RustFS.
        match self.get_bucket_replication(bucket_name).await {
            Ok(config) if config.rules.is_empty() => Ok(ReplicationRemoval::Removed),
            Ok(config) => {
                warn!(
                    "The storage backend still reports {} replication rule(s) for bucket '{}'",
                    config.rules.len(),
                    bucket_name
                );
                Ok(ReplicationRemoval::RulesRemain(config))
            }
            Err(err) => Ok(ReplicationRemoval::Unconfirmed(format!("{err:#}"))),
        }
    }
}
