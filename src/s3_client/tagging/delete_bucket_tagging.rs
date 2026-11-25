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
    /// Remove every tag from a bucket.
    pub async fn delete_bucket_tagging(&self, bucket_name: &str) -> Result<()> {
        info!("Removing all tags from bucket '{}'", bucket_name);

        self.client
            .delete_bucket_tagging()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to remove the tags of bucket '{bucket_name}'"
            ))?;

        info!("Bucket tagging for '{}' removed", bucket_name);

        Ok(())
    }
}
