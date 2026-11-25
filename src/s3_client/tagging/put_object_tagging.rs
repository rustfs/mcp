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
use std::collections::HashMap;
use tracing::info;

use super::super::S3Client;
use super::{MAX_OBJECT_TAGS, tagging_request, validate_tags};

impl S3Client {
    /// Replace the tag set of an object, or of one specific version of it.
    pub async fn put_object_tagging(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
        tags: &HashMap<String, String>,
    ) -> Result<()> {
        info!(
            "Setting {} tag(s) on s3://{}/{}",
            tags.len(),
            bucket_name,
            object_key
        );

        validate_tags(tags, "object", MAX_OBJECT_TAGS)?;
        let tagging = tagging_request(tags)?;

        let mut request = self
            .client
            .put_object_tagging()
            .bucket(bucket_name)
            .key(object_key)
            .tagging(tagging);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to set tags on s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Object tagging for s3://{}/{} updated",
            bucket_name, object_key
        );

        Ok(())
    }
}
