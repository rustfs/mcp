use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::LoggingConfiguration;

/// Result payload for reading a bucket's logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketLoggingConfig {
    pub bucket: String,
    /// `None` when access logging is not enabled for the bucket.
    pub configuration: Option<LoggingConfiguration>,
    /// Settings the configuration carries that this model does not show,
    /// e.g. log-object access grants; `put_bucket_logging` replaces the
    /// whole configuration and would drop them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unshown_settings: Vec<String>,
}

impl S3Client {
    /// Read a bucket's logging configuration.
    ///
    /// A bucket without logging is returned as a configuration of `None`,
    /// which is how S3 reports it: with an empty status, not with an error.
    pub async fn get_bucket_logging(&self, bucket_name: &str) -> Result<BucketLoggingConfig> {
        debug!(
            "Fetching logging configuration for bucket '{}'",
            bucket_name
        );

        let response = self
            .client
            .get_bucket_logging()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to get the logging configuration of bucket '{bucket_name}'"
            ))?;

        // The target bucket and prefix are always shown; settings this
        // crate does not model are named in `unshown_settings` instead of
        // failing the whole read, so callers still see where the logs go.
        let (configuration, unshown_settings) = match response.logging_enabled() {
            Some(enabled) => (
                Some(LoggingConfiguration {
                    target_bucket: enabled.target_bucket().to_string(),
                    target_prefix: (!enabled.target_prefix().is_empty())
                        .then(|| enabled.target_prefix().to_string()),
                }),
                unshown_settings(enabled),
            ),
            None => (None, Vec::new()),
        };

        debug!(
            "Bucket '{}' has logging configuration: {:?} ({} unshown setting(s))",
            bucket_name,
            configuration,
            unshown_settings.len()
        );

        Ok(BucketLoggingConfig {
            bucket: bucket_name.to_string(),
            configuration,
            unshown_settings,
        })
    }
}

/// Settings a logging configuration carries that this crate does not
/// model, named for the getter's warning and for `put_bucket_logging`'s
/// refusal. `SimplePrefix` is not among them: it is the default naming and
/// equivalent to no explicit format.
fn unshown_settings(enabled: &aws_sdk_s3::types::LoggingEnabled) -> Vec<String> {
    let mut unshown = Vec::new();
    if !enabled.target_grants().is_empty() {
        unshown.push("log-object access grants (TargetGrants)".to_string());
    }
    // The format is a struct with optional members: SimplePrefix is the
    // default naming and needs no separate setting, PartitionedPrefix is
    // not modelled.
    if enabled
        .target_object_key_format()
        .is_some_and(|format| format.partitioned_prefix().is_some())
    {
        unshown.push("a partitioned log-object key format (TargetObjectKeyFormat)".to_string());
    }
    unshown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unshown_settings_names_what_the_model_does_not_show() {
        // A plain configuration, and the default SimplePrefix naming, have
        // nothing to report.
        let plain = aws_sdk_s3::types::LoggingEnabled::builder()
            .target_bucket("my-logs")
            .target_prefix("logs/")
            .build()
            .unwrap();
        assert!(unshown_settings(&plain).is_empty());
        let simple = aws_sdk_s3::types::LoggingEnabled::builder()
            .target_bucket("my-logs")
            .target_prefix("logs/")
            .target_object_key_format(
                aws_sdk_s3::types::TargetObjectKeyFormat::builder()
                    .simple_prefix(aws_sdk_s3::types::SimplePrefix::builder().build())
                    .build(),
            )
            .build()
            .unwrap();
        assert!(unshown_settings(&simple).is_empty());

        // Access grants and a partitioned key format are named.
        let granted = aws_sdk_s3::types::LoggingEnabled::builder()
            .target_bucket("my-logs")
            .target_prefix("logs/")
            .target_grants(
                aws_sdk_s3::types::TargetGrant::builder()
                    .grantee(
                        aws_sdk_s3::types::Grantee::builder()
                            .r#type(aws_sdk_s3::types::Type::CanonicalUser)
                            .id("owner-id")
                            .build()
                            .unwrap(),
                    )
                    .permission(aws_sdk_s3::types::BucketLogsPermission::FullControl)
                    .build(),
            )
            .build()
            .unwrap();
        assert_eq!(
            unshown_settings(&granted),
            vec!["log-object access grants (TargetGrants)".to_string()]
        );

        let partitioned = aws_sdk_s3::types::LoggingEnabled::builder()
            .target_bucket("my-logs")
            .target_prefix("logs/")
            .target_object_key_format(
                aws_sdk_s3::types::TargetObjectKeyFormat::builder()
                    .partitioned_prefix(
                        aws_sdk_s3::types::PartitionedPrefix::builder()
                            .partition_date_source(
                                aws_sdk_s3::types::PartitionDateSource::EventTime,
                            )
                            .build(),
                    )
                    .build(),
            )
            .build()
            .unwrap();
        assert_eq!(
            unshown_settings(&partitioned),
            vec!["a partitioned log-object key format (TargetObjectKeyFormat)".to_string()]
        );
    }

    #[test]
    fn test_bucket_logging_config_serialization() {
        let config = BucketLoggingConfig {
            bucket: "test-bucket".to_string(),
            configuration: Some(LoggingConfiguration {
                target_bucket: "my-logs".to_string(),
                target_prefix: Some("logs/".to_string()),
            }),
            unshown_settings: vec!["log-object access grants (TargetGrants)".to_string()],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketLoggingConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.configuration, config.configuration);
        assert_eq!(deserialized.unshown_settings, config.unshown_settings);
    }

    #[test]
    fn test_bucket_logging_config_without_logging_serialization() {
        // "Logging is off" is a configuration of None, and an omitted
        // unshown list means there is nothing to warn about.
        let json = r#"{"bucket": "test-bucket", "configuration": null}"#;
        let deserialized: BucketLoggingConfig = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert!(deserialized.configuration.is_none());
        assert!(deserialized.unshown_settings.is_empty());
    }
}
