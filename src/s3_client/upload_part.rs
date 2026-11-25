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
use aws_sdk_s3::primitives::ByteStream;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

use super::S3Client;

/// Result payload for a single multipart part upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedPart {
    pub part_number: i32,
    pub etag: String,
    pub size: u64,
}

impl S3Client {
    /// Upload a local file as a single multipart part.
    ///
    /// The whole file is sent as one part identified by `part_number`
    /// (1-10000). Record the returned ETag for the completion call.
    pub async fn upload_part_from_file(
        &self,
        local_path: &str,
        bucket_name: &str,
        object_key: &str,
        upload_id: &str,
        part_number: i32,
    ) -> Result<UploadedPart> {
        info!(
            "Uploading part {} for multipart upload '{}': '{}' -> s3://{}/{}",
            part_number, upload_id, local_path, bucket_name, object_key
        );

        if !(1..=10000).contains(&part_number) {
            anyhow::bail!("part_number must be between 1 and 10000, got {part_number}");
        }

        let path = Path::new(local_path);
        let canonical_path = path
            .canonicalize()
            .context(format!("Failed to resolve file path: {local_path}"))?;

        if !canonical_path.exists() {
            anyhow::bail!("File does not exist: {local_path}");
        }

        if !canonical_path.is_file() {
            anyhow::bail!("Path is not a file: {local_path}");
        }

        let size = tokio::fs::metadata(&canonical_path)
            .await
            .context(format!("Failed to read file metadata: {local_path}"))?
            .len();

        // Stream the part from disk instead of buffering it in memory:
        // valid multipart parts can be several GiB each.
        let byte_stream = ByteStream::from_path(&canonical_path)
            .await
            .context(format!("Failed to open file for streaming: {local_path}"))?;

        let response = self
            .client
            .upload_part()
            .bucket(bucket_name)
            .key(object_key)
            .upload_id(upload_id)
            .part_number(part_number)
            .content_length(size as i64)
            .body(byte_stream)
            .send()
            .await
            .context(format!(
                "Failed to upload part {part_number} for s3://{bucket_name}/{object_key}"
            ))?;

        let etag = response
            .e_tag()
            .context("S3 did not return an ETag for the uploaded part")?
            .to_string();

        info!(
            "Part {} uploaded successfully ({} bytes, ETag {})",
            part_number, size, etag
        );

        Ok(UploadedPart {
            part_number,
            etag,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uploaded_part_serialization() {
        let part = UploadedPart {
            part_number: 1,
            etag: "\"etag1\"".to_string(),
            size: 5 * 1024 * 1024,
        };
        let json = serde_json::to_string(&part).unwrap();
        let deserialized: UploadedPart = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.part_number, 1);
        assert_eq!(deserialized.size, 5 * 1024 * 1024);
    }
}
