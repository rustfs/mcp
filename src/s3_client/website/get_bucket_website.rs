use anyhow::{Context, Result};
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::WebsiteConfiguration;

/// Error code S3 answers with when a bucket has no website configuration.
const NO_WEBSITE_ERROR_CODE: &str = "NoSuchWebsiteConfiguration";

/// Result payload for reading a bucket's website configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketWebsiteConfig {
    pub bucket: String,
    /// `None` when the bucket has no website configuration.
    pub configuration: Option<WebsiteConfiguration>,
}

impl S3Client {
    /// Read a bucket's website configuration.
    ///
    /// A bucket without a website configuration is returned as a
    /// configuration of `None`.
    pub async fn get_bucket_website(&self, bucket_name: &str) -> Result<BucketWebsiteConfig> {
        debug!(
            "Fetching website configuration for bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_bucket_website()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a website configuration answers with a
            // dedicated error code; report that as "not configured" instead
            // of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code()) == Some(NO_WEBSITE_ERROR_CODE) =>
            {
                return Ok(BucketWebsiteConfig {
                    bucket: bucket_name.to_string(),
                    configuration: None,
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the website configuration of bucket '{bucket_name}'"
                ));
            }
        };

        let configuration = WebsiteConfiguration::from_sdk(
            response.index_document(),
            response.error_document(),
            response.redirect_all_requests_to(),
            response.routing_rules(),
        )
        .with_context(|| {
            format!("Failed to read the website configuration of bucket '{bucket_name}'")
        })?;

        debug!(
            "Bucket '{}' has website configuration: {:?}",
            bucket_name, configuration
        );

        Ok(BucketWebsiteConfig {
            bucket: bucket_name.to_string(),
            configuration: Some(configuration),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::{RoutingRule, WebsiteProtocol};

    #[test]
    fn test_bucket_website_config_serialization() {
        let config = BucketWebsiteConfig {
            bucket: "test-bucket".to_string(),
            configuration: Some(WebsiteConfiguration {
                index_document_suffix: Some("index.html".to_string()),
                error_document_key: None,
                redirect_all_requests_to: Some(crate::s3_client::RedirectAllRequestsTo {
                    host_name: "www.example.com".to_string(),
                    protocol: Some(WebsiteProtocol::Https),
                }),
                routing_rules: vec![RoutingRule {
                    key_prefix_equals: "docs/".to_string(),
                    replace_key_prefix_with: "documents/".to_string(),
                }],
            }),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketWebsiteConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.configuration, config.configuration);
    }

    #[test]
    fn test_bucket_website_config_without_configuration_serialization() {
        // "No website is configured" is a configuration of None.
        let json = r#"{"bucket": "test-bucket", "configuration": null}"#;
        let deserialized: BucketWebsiteConfig = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert!(deserialized.configuration.is_none());
    }
}
