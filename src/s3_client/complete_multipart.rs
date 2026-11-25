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
use tracing::info;

use super::S3Client;

/// Result payload for completing a multipart upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedMultipartResult {
    pub bucket: String,
    pub key: String,
    pub location: Option<String>,
    pub etag: Option<String>,
    pub version_id: Option<String>,
}

impl S3Client {
    /// Complete a multipart upload from the collected parts.
    ///
    /// Parts are sorted by part number ascending, as S3 requires.
    pub async fn complete_multipart_upload(
        &self,
        bucket_name: &str,
        object_key: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<CompletedMultipartResult> {
        info!(
            "Completing multipart upload '{}' with {} part(s): s3://{}/{}",
            upload_id,
            parts.len(),
            bucket_name,
            object_key
        );

        if parts.is_empty() {
            anyhow::bail!("No parts provided; at least one uploaded part is required");
        }

        let mut sorted = parts;
        sorted.sort_by_key(|(number, _)| *number);

        let completed_parts: Vec<aws_sdk_s3::types::CompletedPart> = sorted
            .iter()
            .map(|(number, etag)| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(*number)
                    .e_tag(etag)
                    .build()
            })
            .collect();

        let completed_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        let response = self
            .client
            .complete_multipart_upload()
            .bucket(bucket_name)
            .key(object_key)
            .upload_id(upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .context(format!(
                "Failed to complete multipart upload for s3://{bucket_name}/{object_key}"
            ))?;

        info!("Multipart upload completed successfully");

        Ok(CompletedMultipartResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            location: response.location().map(|l| l.to_string()),
            etag: response.e_tag().map(|e| e.to_string()),
            version_id: response.version_id().map(|v| v.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completed_multipart_result_serialization() {
        let completed = CompletedMultipartResult {
            bucket: "test-bucket".to_string(),
            key: "big/video.mp4".to_string(),
            location: Some("https://example.com/test-bucket/big/video.mp4".to_string()),
            etag: Some("\"combined-etag\"".to_string()),
            version_id: None,
        };
        let json = serde_json::to_string(&completed).unwrap();
        let deserialized: CompletedMultipartResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.location.is_some());
        assert!(deserialized.version_id.is_none());
    }
}
