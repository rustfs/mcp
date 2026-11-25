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
mod complete_multipart;
mod copy_object;
mod create_bucket;
mod delete_bucket;
mod delete_object;
mod delete_objects;
mod detect_file_type;
mod download_object;
mod get_object;
mod head_object;
mod health_check;
mod initiate_multipart;
mod list_buckets;
mod list_objects;
mod list_parts;
mod presigned_url;
mod tagging;
mod types;
mod upload_content;
mod upload_file;
mod upload_options;
mod upload_part;

// Re-export the operation-specific types so external paths
// (`rustfs_mcp::s3_client::*`) stay stable across the split.
pub use complete_multipart::*;
pub use copy_object::*;
pub use delete_objects::*;
pub use head_object::*;
pub use initiate_multipart::*;
pub use list_parts::*;
pub use presigned_url::*;
pub use tagging::*;
pub use types::*;
pub use upload_content::*;
pub use upload_part::*;

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
mod tests {
    use super::*;

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
