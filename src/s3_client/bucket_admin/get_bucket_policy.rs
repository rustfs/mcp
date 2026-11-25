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
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;

/// Error code S3 answers with when a bucket has no policy.
const NO_POLICY_ERROR_CODE: &str = "NoSuchBucketPolicy";

/// Result payload for reading a bucket policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketPolicy {
    pub bucket: String,
    /// The policy document as JSON text; `None` when the bucket has no policy.
    pub policy: Option<String>,
}

impl S3Client {
    /// Read a bucket's policy document.
    pub async fn get_bucket_policy(&self, bucket_name: &str) -> Result<BucketPolicy> {
        debug!("Fetching the policy of bucket '{}'", bucket_name);

        let response = match self
            .client
            .get_bucket_policy()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a policy answers with a dedicated error code;
            // report that as "no policy" instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code()) == Some(NO_POLICY_ERROR_CODE) =>
            {
                return Ok(BucketPolicy {
                    bucket: bucket_name.to_string(),
                    policy: None,
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the policy of bucket '{bucket_name}'"
                ));
            }
        };

        Ok(BucketPolicy {
            bucket: bucket_name.to_string(),
            policy: response.policy().map(|policy| policy.to_string()),
        })
    }
}
