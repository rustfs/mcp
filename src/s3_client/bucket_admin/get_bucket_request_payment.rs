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
use tracing::debug;

use super::super::S3Client;

/// Result payload for reading a bucket's request payment configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BucketRequestPayment {
    pub bucket: String,
    /// Who pays for the requests to the bucket.
    pub payer: BucketPayer,
}

/// Who pays for the requests to a bucket. S3's default is the bucket owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum BucketPayer {
    /// The bucket owner pays for the requests, including anonymous ones.
    #[serde(rename = "BucketOwner")]
    BucketOwner,
    /// The requester pays and must authenticate; anonymous access is off.
    #[serde(rename = "Requester")]
    Requester,
}

impl BucketPayer {
    /// Map the SDK enum, reporting a value this crate does not model as the
    /// default.
    fn from_sdk(payer: Option<&aws_sdk_s3::types::Payer>) -> Self {
        match payer {
            Some(aws_sdk_s3::types::Payer::Requester) => Self::Requester,
            // S3 leaves the element off when the default applies, and a
            // payer this crate does not model cannot be relied on either.
            _ => Self::BucketOwner,
        }
    }

    pub(super) fn to_sdk(self) -> aws_sdk_s3::types::Payer {
        match self {
            Self::BucketOwner => aws_sdk_s3::types::Payer::BucketOwner,
            Self::Requester => aws_sdk_s3::types::Payer::Requester,
        }
    }

    /// Display name for summaries.
    pub fn label(&self) -> &'static str {
        match self {
            Self::BucketOwner => "the bucket owner",
            Self::Requester => "the requester",
        }
    }
}

impl S3Client {
    /// Read who pays for the requests to a bucket.
    ///
    /// A bucket without an explicit configuration is reported with the S3
    /// default: the bucket owner pays.
    pub async fn get_bucket_request_payment(
        &self,
        bucket_name: &str,
    ) -> Result<BucketRequestPayment> {
        debug!(
            "Fetching the request payment configuration of bucket '{}'",
            bucket_name
        );

        let response = self
            .client
            .get_bucket_request_payment()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to get the request payment configuration of bucket '{bucket_name}'"
            ))?;

        let payer = BucketPayer::from_sdk(response.payer());
        debug!("Bucket '{}' request payer: {:?}", bucket_name, payer);

        Ok(BucketRequestPayment {
            bucket: bucket_name.to_string(),
            payer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_payer_from_sdk_defaults_to_bucket_owner() {
        // S3 leaves the element off when the default applies.
        assert_eq!(BucketPayer::from_sdk(None), BucketPayer::BucketOwner);
        assert_eq!(
            BucketPayer::from_sdk(Some(&aws_sdk_s3::types::Payer::BucketOwner)),
            BucketPayer::BucketOwner
        );
        assert_eq!(
            BucketPayer::from_sdk(Some(&aws_sdk_s3::types::Payer::Requester)),
            BucketPayer::Requester
        );
        // A payer the SDK does not model arrives as its `Unknown` variant.
        let unknown = aws_sdk_s3::types::Payer::from("future-payer");
        assert_eq!(
            BucketPayer::from_sdk(Some(&unknown)),
            BucketPayer::BucketOwner
        );
    }

    #[test]
    fn test_bucket_payer_sdk_round_trip_and_serialization() {
        for payer in [BucketPayer::BucketOwner, BucketPayer::Requester] {
            let sdk = payer.to_sdk();
            assert_eq!(BucketPayer::from_sdk(Some(&sdk)), payer);
        }

        assert_eq!(
            serde_json::to_string(&BucketPayer::Requester).unwrap(),
            r#""Requester""#
        );
        let parsed: BucketPayer = serde_json::from_str(r#""BucketOwner""#).unwrap();
        assert_eq!(parsed, BucketPayer::BucketOwner);
        assert!(serde_json::from_str::<BucketPayer>(r#""SomeoneElse""#).is_err());
        assert_eq!(BucketPayer::Requester.label(), "the requester");
        assert_eq!(BucketPayer::BucketOwner.label(), "the bucket owner");
    }

    #[test]
    fn test_bucket_request_payment_serialization() {
        let config = BucketRequestPayment {
            bucket: "downloads".to_string(),
            payer: BucketPayer::Requester,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(json, r#"{"bucket":"downloads","payer":"Requester"}"#);
        let deserialized: BucketRequestPayment = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, config);
    }
}
