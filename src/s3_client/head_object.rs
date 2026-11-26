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
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::S3Client;

/// Result payload for object metadata retrieval (HEAD).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadObjectResult {
    pub bucket: String,
    pub key: String,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub content_type: Option<String>,
    pub cache_control: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub content_language: Option<String>,
    pub storage_class: Option<String>,
    pub version_id: Option<String>,
    pub user_metadata: std::collections::HashMap<String, String>,
    /// Retention mode of the version, when it carries a retention period.
    pub object_lock_mode: Option<super::ObjectLockRetentionMode>,
    /// ISO 8601 timestamp until which the version stays retained.
    pub object_lock_retain_until: Option<String>,
    /// Legal hold status of the version, when the storage backend reports one.
    pub object_lock_legal_hold: Option<super::ObjectLockLegalHoldStatus>,
}

impl S3Client {
    /// Retrieve object metadata without downloading its content.
    pub async fn head_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<HeadObjectResult> {
        debug!(
            "Fetching metadata for object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .head_object()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let response = request.send().await.context(format!(
            "Failed to fetch metadata for s3://{bucket_name}/{object_key}"
        ))?;

        let last_modified = response.last_modified().map(|dt| {
            dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                .unwrap()
        });

        // A version can carry a retention period, a legal hold, or both;
        // they are read from the headers of the same response.
        let object_lock_mode = response
            .object_lock_mode()
            .and_then(super::ObjectLockRetentionMode::from_sdk_head);
        let object_lock_retain_until = super::iso8601(response.object_lock_retain_until_date());

        let result = HeadObjectResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            size: response.content_length(),
            last_modified,
            etag: response.e_tag().map(|e| e.to_string()),
            content_type: response.content_type().map(|ct| ct.to_string()),
            cache_control: response.cache_control().map(|cc| cc.to_string()),
            content_encoding: response.content_encoding().map(|ce| ce.to_string()),
            content_disposition: response.content_disposition().map(|cd| cd.to_string()),
            content_language: response.content_language().map(|cl| cl.to_string()),
            storage_class: response.storage_class().map(|sc| sc.as_str().to_string()),
            version_id: response.version_id().map(|v| v.to_string()),
            user_metadata: response.metadata().cloned().unwrap_or_default(),
            object_lock_mode,
            object_lock_retain_until,
            object_lock_legal_hold: response
                .object_lock_legal_hold_status()
                .and_then(super::ObjectLockLegalHoldStatus::from_sdk),
        };

        debug!(
            "Object metadata retrieved: {} bytes",
            result.size.unwrap_or(0)
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_head_object_result_serialization() {
        let result = HeadObjectResult {
            bucket: "test-bucket".to_string(),
            key: "docs/report.pdf".to_string(),
            size: Some(2048),
            last_modified: Some("2026-11-23T00:00:00Z".to_string()),
            etag: Some("\"abc123\"".to_string()),
            content_type: Some("application/pdf".to_string()),
            cache_control: Some("max-age=3600".to_string()),
            content_encoding: Some("gzip".to_string()),
            content_disposition: Some("attachment".to_string()),
            content_language: Some("en-US".to_string()),
            storage_class: Some("STANDARD".to_string()),
            version_id: Some("v1".to_string()),
            user_metadata: std::collections::HashMap::from([(
                "author".to_string(),
                "rustfs".to_string(),
            )]),
            object_lock_mode: Some(crate::s3_client::ObjectLockRetentionMode::Compliance),
            object_lock_retain_until: Some("2030-01-01T00:00:00Z".to_string()),
            object_lock_legal_hold: Some(crate::s3_client::ObjectLockLegalHoldStatus::On),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: HeadObjectResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.size, Some(2048));
        assert_eq!(deserialized.content_encoding, Some("gzip".to_string()));
        assert_eq!(
            deserialized.user_metadata.get("author"),
            Some(&"rustfs".to_string())
        );
        assert_eq!(
            deserialized.object_lock_mode,
            Some(crate::s3_client::ObjectLockRetentionMode::Compliance)
        );
        assert_eq!(
            deserialized.object_lock_retain_until.as_deref(),
            Some("2030-01-01T00:00:00Z")
        );
        assert_eq!(
            deserialized.object_lock_legal_hold,
            Some(crate::s3_client::ObjectLockLegalHoldStatus::On)
        );
    }
}
