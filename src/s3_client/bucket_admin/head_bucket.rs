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

use anyhow::Result;
use aws_sdk_s3::operation::head_bucket::HeadBucketError;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;

/// Result payload for checking that a bucket exists and is accessible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadBucketResult {
    pub bucket: String,
    /// Region the bucket lives in, when the storage backend reports it.
    pub region: Option<String>,
}

/// HTTP status of a failed bucket HEAD, when the server answered at all.
fn http_status(err: &aws_sdk_s3::error::SdkError<HeadBucketError>) -> Option<u16> {
    match err {
        aws_sdk_s3::error::SdkError::ServiceError(err) => Some(err.raw().status().as_u16()),
        aws_sdk_s3::error::SdkError::ResponseError(err) => Some(err.raw().status().as_u16()),
        _ => None,
    }
}

/// Explain why a bucket HEAD failed. `not_found` is set when S3 answered
/// with its modeled not-found error and `status` is the HTTP status code.
fn describe_head_failure(bucket_name: &str, not_found: bool, status: Option<u16>) -> String {
    if not_found || status == Some(404) {
        return format!("Bucket '{bucket_name}' does not exist");
    }
    if status == Some(403) {
        return format!(
            "Access to bucket '{bucket_name}' is denied: it may belong to another account, \
             or the credentials lack permission for it"
        );
    }
    format!("Bucket '{bucket_name}' could not be reached")
}

impl S3Client {
    /// Check that a bucket exists and is accessible with the configured
    /// credentials.
    pub async fn head_bucket(&self, bucket_name: &str) -> Result<HeadBucketResult> {
        debug!("Checking bucket '{}'", bucket_name);

        let response = match self.client.head_bucket().bucket(bucket_name).send().await {
            Ok(response) => response,
            Err(err) => {
                let not_found =
                    matches!(err.as_service_error(), Some(HeadBucketError::NotFound(_)));
                let status = http_status(&err);
                anyhow::bail!("{}", describe_head_failure(bucket_name, not_found, status));
            }
        };

        debug!("Bucket '{}' is accessible", bucket_name);

        Ok(HeadBucketResult {
            bucket: bucket_name.to_string(),
            region: response.bucket_region().map(|region| region.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_head_failure_distinguishes_not_found_from_denied() {
        let not_found = describe_head_failure("logs", true, Some(404));
        assert!(not_found.contains("'logs' does not exist"));

        let denied = describe_head_failure("logs", false, Some(403));
        assert!(denied.contains("Access to bucket 'logs' is denied"));

        // The modeled error wins even without a status code.
        assert!(describe_head_failure("logs", true, None).contains("does not exist"));
        // A 404 alone is enough as well.
        assert!(describe_head_failure("logs", false, Some(404)).contains("does not exist"));
    }

    #[test]
    fn test_describe_head_failure_for_other_outcomes() {
        let text = describe_head_failure("logs", false, Some(500));
        assert!(text.contains("could not be reached"));
        let text = describe_head_failure("logs", false, None);
        assert!(text.contains("could not be reached"));
    }
}
