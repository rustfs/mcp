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
use super::types::UploadFileOptions;

/// Result payload for multipart upload initiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartInitResult {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
}

impl S3Client {
    /// Initiate a multipart upload and return its upload ID.
    pub async fn initiate_multipart_upload(
        &self,
        bucket_name: &str,
        object_key: &str,
        options: UploadFileOptions,
    ) -> Result<MultipartInitResult> {
        info!(
            "Initiating multipart upload: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .create_multipart_upload()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(content_type) = &options.content_type {
            request = request.content_type(content_type);
        }
        if let Some(storage_class) = &options.storage_class {
            request = request.storage_class(storage_class.as_str().into());
        }
        if let Some(cache_control) = &options.cache_control {
            request = request.cache_control(cache_control);
        }
        if let Some(metadata_map) = &options.metadata {
            for (key, value) in metadata_map {
                request = request.metadata(key, value);
            }
        }
        if let Some(sse) = &options.server_side_encryption {
            request = request.server_side_encryption(sse.as_str().into());
        }
        if let Some(content_disposition) = &options.content_disposition {
            request = request.content_disposition(content_disposition);
        }
        if let Some(content_encoding) = &options.content_encoding {
            request = request.content_encoding(content_encoding);
        }
        if let Some(content_language) = &options.content_language {
            request = request.content_language(content_language);
        }

        let response = request.send().await.context(format!(
            "Failed to initiate multipart upload for s3://{bucket_name}/{object_key}"
        ))?;

        let upload_id = response
            .upload_id()
            .context("S3 did not return an upload ID")?
            .to_string();

        info!("Multipart upload initiated with ID: {}", upload_id);

        Ok(MultipartInitResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            upload_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multipart_init_result_serialization() {
        let init = MultipartInitResult {
            bucket: "test-bucket".to_string(),
            key: "big/video.mp4".to_string(),
            upload_id: "abc123".to_string(),
        };
        let json = serde_json::to_string(&init).unwrap();
        let deserialized: MultipartInitResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.upload_id, "abc123");
    }
}
