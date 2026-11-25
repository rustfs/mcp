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
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::super::S3Client;

/// Identifies one object version (or delete marker) by key and version ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ObjectVersion {
    /// Object key the version belongs to.
    pub key: String,
    /// Version ID as returned by `list_object_versions`.
    pub version_id: String,
}

/// One version removed by a batch delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedObjectVersion {
    pub key: String,
    pub version_id: Option<String>,
    /// Whether the removed version was a delete marker.
    pub delete_marker: bool,
}

/// A version that could not be deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDeleteError {
    pub key: String,
    pub version_id: Option<String>,
    pub message: String,
}

/// Result payload for batch-deleting object versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectVersionsResult {
    pub bucket: String,
    pub deleted: Vec<DeletedObjectVersion>,
    pub errors: Vec<VersionDeleteError>,
}

impl S3Client {
    /// Permanently delete up to 1,000 object versions (delete markers
    /// included) in a single batch request.
    pub async fn delete_object_versions(
        &self,
        bucket_name: &str,
        versions: &[ObjectVersion],
    ) -> Result<DeleteObjectVersionsResult> {
        info!(
            "Batch deleting {} object version(s) from bucket '{}'",
            versions.len(),
            bucket_name
        );

        if versions.is_empty() {
            anyhow::bail!("No object versions provided for batch deletion");
        }
        if versions.len() > 1000 {
            anyhow::bail!(
                "Cannot delete more than 1000 object versions in a single request (got {}); \
                 split the list into smaller batches",
                versions.len()
            );
        }
        if let Some(version) = versions.iter().find(|v| v.version_id.is_empty()) {
            // An identifier without a version ID would delete the latest
            // version of the key instead, which is not what was asked for.
            anyhow::bail!(
                "version_id must not be empty (key '{}'); use the key-based batch delete \
                 for plain keys",
                version.key
            );
        }

        let identifiers = versions
            .iter()
            .map(|version| {
                ObjectIdentifier::builder()
                    .key(&version.key)
                    .version_id(&version.version_id)
                    .build()
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to build object version identifiers")?;

        let delete = Delete::builder()
            .set_objects(Some(identifiers))
            .build()
            .context("Failed to build the batch delete request")?;

        let response = self
            .client
            .delete_objects()
            .bucket(bucket_name)
            .delete(delete)
            .send()
            .await
            .context(format!(
                "Failed to batch delete object versions in bucket '{bucket_name}'"
            ))?;

        let deleted: Vec<DeletedObjectVersion> = response
            .deleted()
            .iter()
            .map(|d| DeletedObjectVersion {
                key: d.key().unwrap_or("unknown").to_string(),
                version_id: d.version_id().map(|v| v.to_string()),
                delete_marker: d.delete_marker().unwrap_or(false),
            })
            .collect();

        let errors: Vec<VersionDeleteError> = response
            .errors()
            .iter()
            .map(|e| VersionDeleteError {
                key: e.key().unwrap_or("unknown").to_string(),
                version_id: e.version_id().map(|v| v.to_string()),
                message: e.message().unwrap_or("unknown error").to_string(),
            })
            .collect();

        debug!(
            "Batch version delete completed: {} deleted, {} errors",
            deleted.len(),
            errors.len()
        );

        Ok(DeleteObjectVersionsResult {
            bucket: bucket_name.to_string(),
            deleted,
            errors,
        })
    }
}
