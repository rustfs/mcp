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

/// Information about an already-uploaded part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub part_number: i32,
    pub etag: Option<String>,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
}

impl S3Client {
    /// List the parts uploaded so far for a multipart upload.
    pub async fn list_parts(
        &self,
        bucket_name: &str,
        object_key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartInfo>> {
        debug!(
            "Listing parts for multipart upload '{}': s3://{}/{}",
            upload_id, bucket_name, object_key
        );

        let mut parts: Vec<PartInfo> = Vec::new();
        let mut request = self
            .client
            .list_parts()
            .bucket(bucket_name)
            .key(object_key)
            .upload_id(upload_id);

        // S3 returns at most 1,000 parts per page; follow the part number
        // marker while the response is truncated so the caller always sees
        // the complete inventory.
        loop {
            let response = request.send().await.context(format!(
                "Failed to list parts for s3://{bucket_name}/{object_key}"
            ))?;

            parts.extend(response.parts().iter().map(|part| PartInfo {
                part_number: part.part_number().unwrap_or(0),
                etag: part.e_tag().map(|e| e.to_string()),
                size: part.size(),
                last_modified: part.last_modified().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap()
                }),
            }));

            match response.is_truncated() {
                Some(true) => match response.next_part_number_marker() {
                    Some(marker) => {
                        request = self
                            .client
                            .list_parts()
                            .bucket(bucket_name)
                            .key(object_key)
                            .upload_id(upload_id)
                            .part_number_marker(marker);
                    }
                    None => break,
                },
                _ => break,
            }
        }

        debug!("Found {} part(s)", parts.len());

        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_part_info_serialization() {
        let info = PartInfo {
            part_number: 2,
            etag: Some("\"etag2\"".to_string()),
            size: Some(1024),
            last_modified: Some("2026-11-25T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: PartInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.etag, Some("\"etag2\"".to_string()));
    }
}
