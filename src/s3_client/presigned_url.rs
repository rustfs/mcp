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
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::S3Client;

/// HTTP method a presigned URL grants.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum PresignMethod {
    /// Presigned URL for downloading the object (HTTP GET).
    #[serde(rename = "get")]
    Get,
    /// Presigned URL for uploading the object (HTTP PUT).
    #[serde(rename = "put")]
    Put,
}

/// Result payload for presigned URL generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedUrlResult {
    pub url: String,
    pub method: String,
    pub bucket: String,
    pub key: String,
    pub expires_in_secs: u64,
}

impl S3Client {
    /// Generate a presigned URL for downloading or uploading an object.
    ///
    /// The URL embeds SigV4 authentication in its query string, so the
    /// caller can perform the operation without credentials until it expires.
    pub async fn presigned_url(
        &self,
        bucket_name: &str,
        object_key: &str,
        method: PresignMethod,
        expires_in_secs: u64,
    ) -> Result<PresignedUrlResult> {
        info!(
            "Generating presigned {:?} URL for s3://{}/{} (expires in {}s)",
            method, bucket_name, object_key, expires_in_secs
        );

        if !(1..=604800).contains(&expires_in_secs) {
            anyhow::bail!(
                "expires_in_secs must be between 1 and 604800 seconds (7 days), got {expires_in_secs}"
            );
        }

        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::builder()
            .expires_in(std::time::Duration::from_secs(expires_in_secs))
            .build()
            .context("Failed to build presigning config")?;

        let presigned = match method {
            PresignMethod::Get => self
                .client
                .get_object()
                .bucket(bucket_name)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .context(format!(
                    "Failed to generate presigned URL for s3://{bucket_name}/{object_key}"
                ))?,
            PresignMethod::Put => self
                .client
                .put_object()
                .bucket(bucket_name)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .context(format!(
                    "Failed to generate presigned URL for s3://{bucket_name}/{object_key}"
                ))?,
        };

        let method_label = match method {
            PresignMethod::Get => "GET",
            PresignMethod::Put => "PUT",
        };

        // Never log the URL itself: it embeds SigV4 credentials in its query string.
        debug!(
            "Presigned {} URL generated for s3://{}/{}",
            method_label, bucket_name, object_key
        );

        Ok(PresignedUrlResult {
            url: presigned.uri().to_string(),
            method: method_label.to_string(),
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            expires_in_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presign_method_serialization() {
        let json = serde_json::to_string(&PresignMethod::Put).unwrap();
        assert_eq!(json, r#""put""#);

        let deserialized: PresignMethod = serde_json::from_str(r#""get""#).unwrap();
        assert_eq!(deserialized, PresignMethod::Get);
    }

    #[test]
    fn test_presigned_url_result_serialization() {
        let result = PresignedUrlResult {
            url: "https://example.com/bucket/key?X-Amz-Signature=abc".to_string(),
            method: "GET".to_string(),
            bucket: "test-bucket".to_string(),
            key: "docs/a.txt".to_string(),
            expires_in_secs: 3600,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PresignedUrlResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.method, "GET");
        assert_eq!(deserialized.expires_in_secs, 3600);
        assert!(deserialized.url.contains("X-Amz-Signature"));
    }
}
