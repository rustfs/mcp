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
use super::{tagging_request, validate_tags};

impl S3Client {
    /// Replace a bucket's tag set with `tags`.
    pub async fn put_bucket_tagging(
        &self,
        bucket_name: &str,
        tags: &HashMap<String, String>,
    ) -> Result<()> {
        info!("Setting {} tag(s) on bucket '{}'", tags.len(), bucket_name);

        validate_tags(tags)?;
        let tagging = tagging_request(tags)?;

        self.client
            .put_bucket_tagging()
            .bucket(bucket_name)
            .tagging(tagging)
            .send()
            .await
            .context(format!("Failed to set tags on bucket '{bucket_name}'"))?;

        info!("Bucket tagging for '{}' updated", bucket_name);

        Ok(())
    }
}
