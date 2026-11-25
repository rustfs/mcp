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
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use super::S3Client;
use super::types::GetObjectOptions;

impl S3Client {
    /// Download an S3 object directly to a local file path.
    pub async fn download_object_to_file(
        &self,
        bucket_name: &str,
        object_key: &str,
        local_path: &str,
        options: GetObjectOptions,
    ) -> Result<(u64, String)> {
        info!(
            "Downloading object: s3://{}/{} -> {}",
            bucket_name, object_key, local_path
        );

        let mut request = self.client.get_object().bucket(bucket_name).key(object_key);

        if let Some(version_id) = &options.version_id {
            request = request.version_id(version_id);
        }

        if let Some(range) = &options.range {
            request = request.range(range);
        }

        if let Some(if_modified_since) = &options.if_modified_since {
            request = request.if_modified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_modified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_modified_since date")?,
            );
        }

        if let Some(if_unmodified_since) = &options.if_unmodified_since {
            request = request.if_unmodified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_unmodified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_unmodified_since date")?,
            );
        }

        debug!("Executing S3 get_object request for download");
        let response = request.send().await.context(format!(
            "Failed to get object from s3://{bucket_name}/{object_key}"
        ))?;

        let local_file_path = Path::new(local_path);

        if let Some(parent) = local_file_path.parent() {
            tokio::fs::create_dir_all(parent).await.context(format!(
                "Failed to create parent directories for {local_path}"
            ))?;
        }

        let mut file = tokio::fs::File::create(local_file_path)
            .await
            .context(format!("Failed to create local file: {local_path}"))?;

        let mut byte_stream = response.body;
        let mut total_bytes = 0u64;

        while let Some(bytes_result) = byte_stream
            .try_next()
            .await
            .context("Failed to read object content")?
        {
            file.write_all(&bytes_result)
                .await
                .context(format!("Failed to write to local file: {local_path}"))?;
            total_bytes += bytes_result.len() as u64;
        }

        file.flush().await.context("Failed to flush file to disk")?;

        let absolute_path = local_file_path
            .canonicalize()
            .unwrap_or_else(|_| local_file_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        info!(
            "Object downloaded successfully: {} bytes from s3://{}/{} to {}",
            total_bytes, bucket_name, object_key, absolute_path
        );

        Ok((total_bytes, absolute_path))
    }
}
