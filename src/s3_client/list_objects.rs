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
use tracing::debug;

use super::S3Client;
use super::types::{ListObjectsOptions, ListObjectsResult, ObjectInfo};

impl S3Client {
    /// List objects in a bucket with optional pagination/filtering.
    pub async fn list_objects_v2(
        &self,
        bucket_name: &str,
        options: ListObjectsOptions,
    ) -> Result<ListObjectsResult> {
        debug!(
            "Listing objects in bucket '{}' with options: {:?}",
            bucket_name, options
        );

        let mut request = self.client.list_objects_v2().bucket(bucket_name);

        if let Some(prefix) = options.prefix {
            request = request.prefix(prefix);
        }

        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        if let Some(max_keys) = options.max_keys {
            request = request.max_keys(max_keys);
        }

        if let Some(continuation_token) = options.continuation_token {
            request = request.continuation_token(continuation_token);
        }

        if let Some(start_after) = options.start_after {
            request = request.start_after(start_after);
        }

        let response = request
            .send()
            .await
            .context(format!("Failed to list objects in bucket '{bucket_name}'"))?;

        let objects: Vec<ObjectInfo> = response
            .contents()
            .iter()
            .map(|obj| {
                let key = obj.key().unwrap_or("unknown").to_string();
                let size = obj.size();
                let last_modified = obj.last_modified().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap()
                });
                let etag = obj.e_tag().map(|e| e.to_string());
                let storage_class = obj.storage_class().map(|sc| sc.as_str().to_string());

                ObjectInfo {
                    key,
                    size,
                    last_modified,
                    etag,
                    storage_class,
                }
            })
            .collect();

        let common_prefixes: Vec<String> = response
            .common_prefixes()
            .iter()
            .filter_map(|cp| cp.prefix())
            .map(|p| p.to_string())
            .collect();

        let result = ListObjectsResult {
            objects,
            common_prefixes,
            is_truncated: response.is_truncated().unwrap_or(false),
            next_continuation_token: response.next_continuation_token().map(|t| t.to_string()),
            max_keys: response.max_keys(),
            key_count: response.key_count().unwrap_or(0),
        };

        debug!(
            "Found {} objects and {} common prefixes in bucket '{}'",
            result.objects.len(),
            result.common_prefixes.len(),
            bucket_name
        );

        Ok(result)
    }
}
