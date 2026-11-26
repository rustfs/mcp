use anyhow::{Context, Result};
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::{NotificationRule, rules_from_sdk};

/// Error code S3 answers with when a bucket has no notification configuration.
const NO_NOTIFICATION_ERROR_CODE: &str = "NoSuchNotificationConfiguration";

/// Result payload for reading a bucket's notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketNotificationConfig {
    pub bucket: String,
    /// Empty when the bucket delivers no notifications.
    pub rules: Vec<NotificationRule>,
}

impl S3Client {
    /// Read a bucket's notification configuration.
    pub async fn get_bucket_notification(
        &self,
        bucket_name: &str,
    ) -> Result<BucketNotificationConfig> {
        debug!(
            "Fetching notification configuration for bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_bucket_notification_configuration()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket that was never configured answers with a dedicated
            // error code on AWS; report that as "no rules" instead of a
            // failure. Other implementations answer with an empty
            // configuration, which the mapping below turns into no rules.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some(NO_NOTIFICATION_ERROR_CODE) =>
            {
                return Ok(BucketNotificationConfig {
                    bucket: bucket_name.to_string(),
                    rules: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the notification configuration of bucket '{bucket_name}'"
                ));
            }
        };

        // The presence of an EventBridge configuration alone means
        // EventBridge delivery is on. This crate cannot show or edit it, so
        // reporting the bucket as rule-less - or handing out rules JSON that
        // would be put back without the setting - would silently disable
        // EventBridge.
        if response.event_bridge_configuration().is_some() {
            anyhow::bail!(
                "Bucket '{bucket_name}' also has an EventBridge notification configuration, \
                 which this tool cannot show or edit: read and change it with the storage \
                 backend's own tooling. put_bucket_notification would replace the whole \
                 configuration and disable EventBridge"
            );
        }

        let rules = rules_from_sdk(
            response.lambda_function_configurations(),
            response.queue_configurations(),
            response.topic_configurations(),
        )
        .with_context(|| {
            format!("Failed to read the notification configuration of bucket '{bucket_name}'")
        })?;

        debug!(
            "Found {} notification rule(s) for bucket '{}'",
            rules.len(),
            bucket_name
        );

        Ok(BucketNotificationConfig {
            bucket: bucket_name.to_string(),
            rules,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_notification_config_serialization() {
        let config = BucketNotificationConfig {
            bucket: "test-bucket".to_string(),
            rules: vec![NotificationRule {
                destination: super::super::NotificationDestination::Queue,
                arn: "arn:rustfs:sqs::primary:webhook".to_string(),
                events: vec!["s3:ObjectCreated:*".to_string()],
                prefix: Some("images/".to_string()),
                suffix: None,
                id: Some("image-uploads".to_string()),
            }],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketNotificationConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.rules, config.rules);
    }

    #[test]
    fn test_bucket_notification_config_without_rules_serialization() {
        // The "nothing is delivered" state is an empty rule list.
        let json = r#"{"bucket": "test-bucket", "rules": []}"#;
        let deserialized: BucketNotificationConfig = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert!(deserialized.rules.is_empty());
    }
}
