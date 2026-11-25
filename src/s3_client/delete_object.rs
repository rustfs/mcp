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

use super::S3Client;

impl S3Client {
    /// Delete a single object, optionally by version ID.
    pub async fn delete_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<()> {
        info!("Deleting object: s3://{}/{}", bucket_name, object_key);

        let mut request = self
            .client
            .delete_object()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to delete object s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Object '{}' deleted successfully from bucket '{}'",
            object_key, bucket_name
        );
        Ok(())
    }
}
