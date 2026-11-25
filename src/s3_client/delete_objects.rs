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

use super::S3Client;

/// Per-key failure inside a batch delete response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectsError {
    pub key: String,
    pub message: String,
}

/// Result payload for batch object deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectsResult {
    pub bucket: String,
    pub deleted: Vec<String>,
    pub errors: Vec<DeleteObjectsError>,
}

impl S3Client {
    /// Delete up to 1,000 objects in a single batch request.
    pub async fn delete_objects(
        &self,
        bucket_name: &str,
        object_keys: &[String],
    ) -> Result<DeleteObjectsResult> {
        info!(
            "Batch deleting {} objects from bucket '{}'",
            object_keys.len(),
            bucket_name
        );

        if object_keys.is_empty() {
            anyhow::bail!("No object keys provided for batch deletion");
        }
        if object_keys.len() > 1000 {
            anyhow::bail!(
                "Cannot delete more than 1000 objects in a single request (got {}); \
                 split the list into smaller batches",
                object_keys.len()
            );
        }

        let identifiers = object_keys
            .iter()
            .map(|key| ObjectIdentifier::builder().key(key).build())
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to build object identifiers")?;

        let delete = Delete::builder()
            .set_objects(Some(identifiers))
            .build()
            .context("Failed to build batch delete request")?;

        let response = self
            .client
            .delete_objects()
            .bucket(bucket_name)
            .delete(delete)
            .send()
            .await
            .context(format!(
                "Failed to batch delete objects in bucket '{bucket_name}'"
            ))?;

        let deleted: Vec<String> = response
            .deleted()
            .iter()
            .filter_map(|d| d.key())
            .map(|k| k.to_string())
            .collect();

        let errors: Vec<DeleteObjectsError> = response
            .errors()
            .iter()
            .map(|e| DeleteObjectsError {
                key: e.key().unwrap_or("unknown").to_string(),
                message: e.message().unwrap_or("unknown error").to_string(),
            })
            .collect();

        debug!(
            "Batch delete completed: {} deleted, {} errors",
            deleted.len(),
            errors.len()
        );

        Ok(DeleteObjectsResult {
            bucket: bucket_name.to_string(),
            deleted,
            errors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_objects_result_serialization() {
        let result = DeleteObjectsResult {
            bucket: "test-bucket".to_string(),
            deleted: vec!["a.txt".to_string(), "b.txt".to_string()],
            errors: vec![DeleteObjectsError {
                key: "locked.txt".to_string(),
                message: "Access Denied".to_string(),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DeleteObjectsResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.deleted.len(), 2);
        assert_eq!(deserialized.errors.len(), 1);
        assert_eq!(deserialized.errors[0].key, "locked.txt");
    }
}
