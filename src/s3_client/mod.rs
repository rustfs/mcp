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

mod abort_multipart;
mod acl;
mod bucket_admin;
mod bucket_probe;
mod complete_multipart;
mod copy_object;
mod cors;
mod create_bucket;
mod delete_bucket;
mod delete_object;
mod delete_objects;
mod detect_file_type;
mod download_object;
mod encryption;
#[cfg(test)]
mod fake_s3;
mod get_object;
mod head_object;
mod health_check;
mod initiate_multipart;
mod lifecycle;
mod list_buckets;
mod list_objects;
mod list_parts;
mod logging;
mod multipart_uploads;
mod notification;
mod object_attributes;
mod object_lock;
mod presigned_url;
mod replication;
mod tagging;
mod types;
mod upload_content;
mod upload_file;
mod upload_options;
mod upload_part;
mod versioning;
mod website;

// Re-export the operation-specific types so external paths
// (`rustfs_mcp::s3_client::*`) stay stable across the split.
pub use acl::*;
pub use bucket_admin::*;
pub use complete_multipart::*;
pub use copy_object::*;
pub use cors::*;
pub use delete_objects::*;
pub use encryption::*;
pub use head_object::*;
pub use initiate_multipart::*;
pub use lifecycle::*;
pub use list_parts::*;
pub use logging::*;
pub use multipart_uploads::*;
pub use notification::*;
pub use object_attributes::{Checksum, ObjectAttributesInfo, ObjectPart};
pub use object_lock::*;
pub use presigned_url::*;
pub use replication::*;
pub use tagging::*;
pub use types::*;
pub use upload_content::*;
pub use upload_part::*;
pub use versioning::*;
pub use website::*;

use anyhow::Result;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::{Client, Config as S3Config};
use aws_smithy_http_client::Builder as SmithyHttpClientBuilder;
use tracing::{debug, info};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct S3Client {
    client: Client,
}

impl S3Client {
    /// Create an S3 client from validated [`Config`].
    pub async fn new(config: &Config) -> Result<Self> {
        info!("Initializing S3 client from configuration");

        let access_key = config.access_key_id();
        let secret_key = config.secret_access_key();

        debug!("Using AWS region: {}", config.region);
        if let Some(ref endpoint) = config.endpoint_url {
            debug!("Using custom endpoint: {}", endpoint);
        }

        let credentials = Credentials::new(access_key, secret_key, None, None, "rustfs-mcp-server");

        let mut config_builder = S3Config::builder()
            .credentials_provider(credentials)
            .region(Region::new(config.region.clone()))
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest());

        if config
            .endpoint_url
            .as_deref()
            .is_some_and(|endpoint| endpoint.starts_with("http://"))
        {
            config_builder =
                config_builder.http_client(SmithyHttpClientBuilder::new().build_http());
        }

        // Set force path style if custom endpoint or explicitly requested
        let should_force_path_style = config.endpoint_url.is_some() || config.force_path_style;
        if should_force_path_style {
            config_builder = config_builder.force_path_style(true);
        }

        if let Some(endpoint) = &config.endpoint_url {
            config_builder = config_builder.endpoint_url(endpoint);
        }

        let s3_config = config_builder.build();
        let client = Client::from_conf(s3_config);

        info!("S3 client initialized successfully");

        Ok(Self { client })
    }
}

#[cfg(test)]
impl S3Client {
    /// A client for the in-process endpoint of [`fake_s3`]: path-style,
    /// without retries, so that a scripted 5xx answer comes back at once
    /// instead of after the retry backoff.
    pub(crate) fn for_endpoint(endpoint: &str) -> Self {
        let config = S3Config::builder()
            .credentials_provider(Credentials::new("key", "secret", None, None, "test"))
            .region(Region::new("us-east-1"))
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .force_path_style(true)
            .http_client(SmithyHttpClientBuilder::new().build_http())
            .retry_config(aws_sdk_s3::config::retry::RetryConfig::disabled())
            .build();

        Self {
            client: Client::from_conf(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::fake_s3::{Answer, FakeS3, OK};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fake_endpoint_round_trip() {
        // The scripted answers reach the client as parsed S3 results, and
        // the endpoint sees the requests the client makes.
        let endpoint = FakeS3::start(|request| {
            if request.method == "HEAD" && request.bucket == "missing" {
                Answer::error(404, "NoSuchBucket", "The specified bucket does not exist")
            } else {
                OK
            }
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        assert!(client.bucket_is_reachable("locked").await);
        assert!(!client.bucket_is_reachable("missing").await);

        let requests = endpoint.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "HEAD");
        assert_eq!(requests[0].bucket, "locked");
    }

    #[tokio::test]
    #[ignore] // Requires AWS credentials
    async fn test_s3_client_creation() {
        let config = Config {
            access_key_id: Some("test_key".to_string()),
            secret_access_key: Some("test_secret".to_string()),
            region: "us-east-1".to_string(),
            ..Config::default()
        };

        let result = S3Client::new(&config).await;
        assert!(result.is_ok());
    }
}
