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
use tracing::{debug, info};

use super::S3Client;
use super::types::{DetectedFileType, GetObjectOptions, GetObjectResult};

impl S3Client {
    /// Fetch object content and metadata from S3.
    pub async fn get_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        options: GetObjectOptions,
    ) -> Result<GetObjectResult> {
        info!("Getting object: s3://{}/{}", bucket_name, object_key);

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

        debug!("Executing S3 get_object request");
        let response = request.send().await.context(format!(
            "Failed to get object from s3://{bucket_name}/{object_key}"
        ))?;

        let content_type = response
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let content_length = response.content_length().unwrap_or(0) as u64;
        let last_modified = response.last_modified().map(|dt| {
            dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                .unwrap()
        });
        let etag = response.e_tag().map(|e| e.to_string());
        let version_id = response.version_id().map(|v| v.to_string());

        let max_size = options.max_content_size.unwrap_or(10 * 1024 * 1024);
        let mut content = Vec::new();
        let mut byte_stream = response.body;
        let mut total_read = 0;

        while let Some(bytes_result) = byte_stream
            .try_next()
            .await
            .context("Failed to read object content")?
        {
            if total_read + bytes_result.len() > max_size {
                anyhow::bail!("Object size exceeds maximum allowed size of {max_size} bytes");
            }
            content.extend_from_slice(&bytes_result);
            total_read += bytes_result.len();
        }

        debug!("Read {} bytes from object", content.len());

        let detected_type = Self::detect_file_type(Some(&content_type), &content);
        debug!("Detected file type: {detected_type:?}");

        let text_content = match &detected_type {
            DetectedFileType::Text => match std::str::from_utf8(&content) {
                Ok(text) => Some(text.to_string()),
                Err(_) => {
                    debug!("Failed to decode content as UTF-8, treating as binary");
                    None
                }
            },
            _ => None,
        };

        let result = GetObjectResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            content_type,
            content_length,
            last_modified,
            etag,
            version_id,
            detected_type,
            content: Some(content),
            text_content,
        };

        info!(
            "Object retrieved successfully: {} bytes from s3://{}/{}",
            result.content_length, bucket_name, object_key
        );

        Ok(result)
    }
}
