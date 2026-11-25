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
    /// Abort a multipart upload and discard all parts uploaded so far.
    pub async fn abort_multipart_upload(
        &self,
        bucket_name: &str,
        object_key: &str,
        upload_id: &str,
    ) -> Result<()> {
        info!(
            "Aborting multipart upload '{}': s3://{}/{}",
            upload_id, bucket_name, object_key
        );

        self.client
            .abort_multipart_upload()
            .bucket(bucket_name)
            .key(object_key)
            .upload_id(upload_id)
            .send()
            .await
            .context(format!(
                "Failed to abort multipart upload for s3://{bucket_name}/{object_key}"
            ))?;

        info!("Multipart upload aborted successfully");
        Ok(())
    }
}
