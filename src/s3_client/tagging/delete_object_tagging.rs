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
use tracing::info;

use super::super::S3Client;

impl S3Client {
    /// Remove every tag from an object, or from one specific version of it.
    pub async fn delete_object_tagging(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<()> {
        info!("Removing all tags from s3://{}/{}", bucket_name, object_key);

        let mut request = self
            .client
            .delete_object_tagging()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to remove the tags of s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Object tagging for s3://{}/{} removed",
            bucket_name, object_key
        );

        Ok(())
    }
}
