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
    /// Map the SDK enum. S3 leaves the payer element off when the default
    /// applies, so an absent one is the bucket owner; a value this crate does
    /// not know is reported instead of guessed at.
    fn from_sdk(payer: Option<&aws_sdk_s3::types::Payer>) -> Result<Self> {
        match payer.map(aws_sdk_s3::types::Payer::as_str) {
            // S3 leaves the payer element off when the default applies.
            None | Some("BucketOwner") => Ok(Self::BucketOwner),
            Some("Requester") => Ok(Self::Requester),
            Some(unknown) => Err(anyhow::anyhow!(
                "The storage backend reported the payer '{unknown}', which this tool does not know"
            )),
        }
    }

    pub(super) fn to_sdk(self) -> aws_sdk_s3::types::Payer {
        match self {
            Self::BucketOwner => aws_sdk_s3::types::Payer::BucketOwner,
            Self::Requester => aws_sdk_s3::types::Payer::Requester,
        }
    }

    /// The name S3 uses for the payer.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BucketOwner => "BucketOwner",
            Self::Requester => "Requester",
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

        let payer = BucketPayer::from_sdk(response.payer()).with_context(|| {
            format!("Failed to get the request payment configuration of bucket '{bucket_name}'")
        })?;
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
        assert_eq!(
            BucketPayer::from_sdk(None).unwrap(),
            BucketPayer::BucketOwner
        );
        assert_eq!(
            BucketPayer::from_sdk(Some(&aws_sdk_s3::types::Payer::BucketOwner)).unwrap(),
            BucketPayer::BucketOwner
        );
        assert_eq!(
            BucketPayer::from_sdk(Some(&aws_sdk_s3::types::Payer::Requester)).unwrap(),
            BucketPayer::Requester
        );

        // A payer the SDK does not model arrives as its `Unknown` variant.
        // Who pays is not guessed: the read fails and names the value.
        let unknown = aws_sdk_s3::types::Payer::from("future-payer");
        let error = BucketPayer::from_sdk(Some(&unknown)).unwrap_err();
        assert!(
            error.to_string().contains("the payer 'future-payer'"),
            "{error}"
        );
    }

    #[test]
    fn test_bucket_payer_sdk_round_trip_and_serialization() {
        for payer in [BucketPayer::BucketOwner, BucketPayer::Requester] {
            let sdk = payer.to_sdk();
            assert_eq!(BucketPayer::from_sdk(Some(&sdk)).unwrap(), payer);
        }

        assert_eq!(
            serde_json::to_string(&BucketPayer::Requester).unwrap(),
            r#""Requester""#
        );
        let parsed: BucketPayer = serde_json::from_str(r#""BucketOwner""#).unwrap();
        assert_eq!(parsed, BucketPayer::BucketOwner);
        assert!(serde_json::from_str::<BucketPayer>(r#""SomeoneElse""#).is_err());

        assert_eq!(BucketPayer::BucketOwner.as_str(), "BucketOwner");
        assert_eq!(BucketPayer::Requester.as_str(), "Requester");
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

/// The request payment tools against an endpoint that answers the way the
/// backends do, so that the real SDK delivers their answers.
#[cfg(test)]
mod backend_tests {
    use super::*;
    use crate::s3_client::fake_s3::{Answer, FakeS3, OK};

    /// The document every backend that implements the setting answers.
    fn configured(payer: &str) -> Answer {
        Answer::ok(format!(
            "<RequestPaymentConfiguration><Payer>{payer}</Payer></RequestPaymentConfiguration>"
        ))
    }

    /// The error and everything behind it, the way a tool response shows it.
    fn shown(error: anyhow::Error) -> String {
        format!("{error:#}")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_reports_the_payer() {
        let endpoint = FakeS3::start(|request| match request.bucket.as_str() {
            "owner" => configured("BucketOwner"),
            "requester" => configured("Requester"),
            // An answer without the payer element means the S3 default.
            "silent" => Answer::ok("<RequestPaymentConfiguration></RequestPaymentConfiguration>"),
            _ => configured("BucketOwner"),
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let read = client.get_bucket_request_payment("owner").await.unwrap();
        assert_eq!(read.payer, BucketPayer::BucketOwner);
        let read = client
            .get_bucket_request_payment("requester")
            .await
            .unwrap();
        assert_eq!(read.payer, BucketPayer::Requester);
        let read = client.get_bucket_request_payment("silent").await.unwrap();
        assert_eq!(read.payer, BucketPayer::BucketOwner);

        // The request addresses the requestPayment subresource.
        let requests = endpoint.requests();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| {
            request.method == "GET" && request.has("requestPayment") && request.key.is_empty()
        }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_keeps_failures() {
        let endpoint = FakeS3::start(|request| match request.bucket.as_str() {
            // A value this tool does not know: who pays is not guessed.
            "future" => configured("FuturePayer"),
            // MinIO does not implement the setting.
            "minio" => Answer::error(
                501,
                "NotImplemented",
                "A header or query you provided implies functionality that is not implemented",
            ),
            "absent" => Answer::error(404, "NoSuchBucket", "The specified bucket does not exist"),
            _ => Answer::error(500, "InternalError", "We encountered an internal error."),
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let error = shown(
            client
                .get_bucket_request_payment("future")
                .await
                .unwrap_err(),
        );
        assert!(error.contains("the payer 'FuturePayer'"), "{error}");

        let error = shown(
            client
                .get_bucket_request_payment("minio")
                .await
                .unwrap_err(),
        );
        assert!(
            error.starts_with("Failed to get the request payment configuration of bucket 'minio'"),
            "{error}"
        );
        assert!(error.contains("NotImplemented"), "{error}");

        let error = shown(
            client
                .get_bucket_request_payment("absent")
                .await
                .unwrap_err(),
        );
        assert!(error.contains("NoSuchBucket"), "{error}");
        assert!(client.get_bucket_request_payment("broken").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_confirms_the_bucket_then_sends_the_payer() {
        let endpoint = FakeS3::start(|request| {
            if request.method == "HEAD" && request.bucket == "absent" {
                Answer::error(404, "NoSuchBucket", "The specified bucket does not exist")
            } else {
                OK
            }
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        client
            .put_bucket_request_payment("downloads", BucketPayer::Requester)
            .await
            .unwrap();
        client
            .put_bucket_request_payment("downloads", BucketPayer::BucketOwner)
            .await
            .unwrap();

        let requests = endpoint.requests();
        let shape: Vec<(&str, &str, bool)> = requests
            .iter()
            .map(|request| {
                (
                    request.method.as_str(),
                    request.bucket.as_str(),
                    request.has("requestPayment"),
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                ("HEAD", "downloads", false),
                ("PUT", "downloads", true),
                ("HEAD", "downloads", false),
                ("PUT", "downloads", true),
            ]
        );
        // The payer travels in the document S3 defines, under its namespace.
        let body = requests[1].body_text();
        assert!(
            body.starts_with(
                "<RequestPaymentConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            ),
            "{body}"
        );
        assert!(body.contains("<Payer>Requester</Payer>"), "{body}");
        assert!(
            requests[3]
                .body_text()
                .contains("<Payer>BucketOwner</Payer>")
        );

        // A bucket that does not exist is refused before anything is sent.
        let error = shown(
            client
                .put_bucket_request_payment("absent", BucketPayer::Requester)
                .await
                .unwrap_err(),
        );
        assert!(
            error.contains("the bucket does not exist or is not accessible"),
            "{error}"
        );
        assert_eq!(endpoint.requests().len(), 5);
    }
}
