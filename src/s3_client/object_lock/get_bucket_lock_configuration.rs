use anyhow::{Context, Result};
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::ObjectLockConfiguration;

/// Error code S3 answers with when a bucket has no Object Lock configuration.
const NO_LOCK_CONFIGURATION_ERROR_CODE: &str = "ObjectLockConfigurationNotFoundError";

/// Result payload for reading a bucket's Object Lock configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketObjectLockConfig {
    pub bucket: String,
    /// `None` when the bucket was not created with Object Lock enabled.
    pub configuration: Option<ObjectLockConfiguration>,
}

impl S3Client {
    /// Read a bucket's Object Lock configuration.
    ///
    /// A bucket without Object Lock is returned as a configuration of
    /// `None`.
    pub async fn get_bucket_lock_configuration(
        &self,
        bucket_name: &str,
    ) -> Result<BucketObjectLockConfig> {
        debug!(
            "Fetching Object Lock configuration for bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_object_lock_configuration()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without Object Lock answers with a dedicated error
            // code; report that as "not enabled" instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some(NO_LOCK_CONFIGURATION_ERROR_CODE) =>
            {
                // Some S3 implementations answer with the same code for a
                // bucket that does not exist, so check that the bucket is
                // reachable before claiming Object Lock is merely disabled.
                if !self.bucket_is_reachable(bucket_name).await {
                    return Err(anyhow::anyhow!(
                        "Failed to get the Object Lock configuration of bucket '{bucket_name}': \
                         the bucket does not exist or is not accessible"
                    ));
                }

                return Ok(BucketObjectLockConfig {
                    bucket: bucket_name.to_string(),
                    configuration: None,
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the Object Lock configuration of bucket '{bucket_name}'"
                ));
            }
        };

        // An answer without a configuration must not be mistaken for a
        // bucket without Object Lock: the server reported something else.
        let configuration = response
            .object_lock_configuration()
            .map(ObjectLockConfiguration::from_sdk)
            .context("The storage backend answered without an Object Lock configuration")?;

        debug!(
            "Bucket '{}' has Object Lock configuration: {:?}",
            bucket_name, configuration
        );

        Ok(BucketObjectLockConfig {
            bucket: bucket_name.to_string(),
            configuration: Some(configuration),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::DefaultRetention;

    #[test]
    fn test_bucket_object_lock_config_serialization() {
        let config = BucketObjectLockConfig {
            bucket: "test-bucket".to_string(),
            configuration: Some(ObjectLockConfiguration {
                object_lock_enabled: true,
                default_retention: Some(DefaultRetention {
                    mode: crate::s3_client::ObjectLockRetentionMode::Compliance,
                    days: Some(30),
                    years: None,
                }),
            }),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketObjectLockConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.configuration, config.configuration);
    }

    #[test]
    fn test_bucket_object_lock_config_without_lock_serialization() {
        // "Object Lock is not enabled" is a configuration of None.
        let json = r#"{"bucket": "test-bucket", "configuration": null}"#;
        let deserialized: BucketObjectLockConfig = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert!(deserialized.configuration.is_none());
    }
}
