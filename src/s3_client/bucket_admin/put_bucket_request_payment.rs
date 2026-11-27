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
use super::get_bucket_request_payment::BucketPayer;

impl S3Client {
    /// Set who pays for the requests to a bucket.
    pub async fn put_bucket_request_payment(
        &self,
        bucket_name: &str,
        payer: BucketPayer,
    ) -> Result<()> {
        info!(
            "Setting the request payer of bucket '{}' to {}",
            bucket_name,
            payer.label()
        );

        // Some S3 implementations accept the configuration for a bucket that
        // does not exist and only apply it once the bucket is created.
        // Confirm the bucket first so a typo is not reported as a successful
        // update.
        self.client
            .head_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to set the request payment configuration of bucket '{bucket_name}': \
                     the bucket does not exist or is not accessible"
                )
            })?;

        let configuration = aws_sdk_s3::types::RequestPaymentConfiguration::builder()
            .payer(payer.to_sdk())
            .build()
            .context("Failed to build the request payment configuration")?;

        self.client
            .put_bucket_request_payment()
            .bucket(bucket_name)
            .request_payment_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the request payment configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Request payment configuration of bucket '{}' updated",
            bucket_name
        );

        Ok(())
    }
}
