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
use std::path::Path;
use tracing::{debug, info};

use super::S3Client;
use super::types::{UploadFileOptions, UploadResult};

impl S3Client {
    /// Upload a local file to S3.
    pub async fn upload_file(
        &self,
        local_path: &str,
        bucket_name: &str,
        object_key: &str,
        options: UploadFileOptions,
    ) -> Result<UploadResult> {
        info!(
            "Starting file upload: '{}' -> s3://{}/{}",
            local_path, bucket_name, object_key
        );

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

        let metadata = tokio::fs::metadata(&canonical_path)
            .await
            .context(format!("Failed to read file metadata: {local_path}"))?;

        let file_size = metadata.len();
        debug!("File size: {file_size} bytes");

        let content_type = options.content_type.unwrap_or_else(|| {
            let detected = mime_guess::from_path(&canonical_path)
                .first_or_octet_stream()
                .to_string();
            debug!("Auto-detected content type: {detected}");
            detected
        });

        // Stream the file from disk instead of buffering it in memory:
        // uploads can be several GiB large.
        let byte_stream = ByteStream::from_path(&canonical_path)
            .await
            .context(format!("Failed to open file for streaming: {local_path}"))?;

        let mut request = self
            .client
            .put_object()
            .bucket(bucket_name)
            .key(object_key)
            .body(byte_stream)
            .content_type(&content_type)
            .content_length(file_size as i64);

        if let Some(storage_class) = &options.storage_class {
            request = request.storage_class(storage_class.as_str().into());
        }

        if let Some(cache_control) = &options.cache_control {
            request = request.cache_control(cache_control);
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

        if let Some(sse) = &options.server_side_encryption {
            request = request.server_side_encryption(sse.as_str().into());
        }

        if let Some(metadata_map) = &options.metadata {
            for (key, value) in metadata_map {
                request = request.metadata(key, value);
            }
        }

        debug!("Executing S3 put_object request");
        let response = request.send().await.context(format!(
            "Failed to upload file to s3://{bucket_name}/{object_key}"
        ))?;

        let etag = response.e_tag().unwrap_or("unknown").to_string();
        let version_id = response.version_id().map(|v| v.to_string());

        let location = format!("s3://{bucket_name}/{object_key}");

        let upload_result = UploadResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            etag,
            location,
            version_id,
            file_size,
            content_type,
            upload_id: None,
        };

        info!(
            "File upload completed successfully: {} bytes uploaded to s3://{}/{}",
            file_size, bucket_name, object_key
        );

        Ok(upload_result)
    }
}
