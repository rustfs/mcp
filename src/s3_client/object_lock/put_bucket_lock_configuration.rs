use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::DefaultRetention;

impl S3Client {
    /// Set the default retention applied to every new object version of a
    /// bucket, or remove it with `None`.
    ///
    /// S3 only accepts this for a bucket that was created with Object Lock
    /// enabled; it answers `InvalidBucketState` for any other bucket.
    pub async fn put_bucket_lock_configuration(
        &self,
        bucket_name: &str,
        default_retention: Option<&DefaultRetention>,
    ) -> Result<()> {
        info!(
            "Setting the default retention of bucket '{}' to {}",
            bucket_name,
            default_retention
                .map(DefaultRetention::label)
                .as_deref()
                .unwrap_or("none")
        );

        // Enforce the documented prerequisite instead of relying on the
        // backend to refuse: on AWS the request would irreversibly enable
        // Object Lock (and permanently enable versioning) for a versioned
        // bucket that does not have it, even when it was only meant to
        // remove a default retention.
        let current = self.get_bucket_lock_configuration(bucket_name).await?;
        if !current
            .configuration
            .as_ref()
            .is_some_and(|configuration| configuration.object_lock_enabled)
        {
            anyhow::bail!(
                "Bucket '{bucket_name}' does not have Object Lock enabled: S3 cannot accept \
                 a lock configuration for it (on AWS the request would irreversibly enable \
                 Object Lock and permanently enable versioning). To lock a bucket, create \
                 a new one with object_lock_enabled set to true"
            );
        }

        let configuration = sdk_configuration(default_retention)?;

        self.client
            .put_object_lock_configuration()
            .bucket(bucket_name)
            .object_lock_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the Object Lock configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Default retention of bucket '{}' set to {}",
            bucket_name,
            default_retention
                .map(DefaultRetention::label)
                .as_deref()
                .unwrap_or("none")
        );

        Ok(())
    }
}

/// Build the request body of `put_object_lock_configuration`, which always
/// enables Object Lock and carries the default retention as its only rule; a
/// body without a rule removes the default retention.
fn sdk_configuration(
    default_retention: Option<&DefaultRetention>,
) -> Result<aws_sdk_s3::types::ObjectLockConfiguration> {
    let rule = match default_retention {
        Some(default_retention) => Some(
            aws_sdk_s3::types::ObjectLockRule::builder()
                .default_retention(default_retention.to_sdk()?)
                .build(),
        ),
        None => None,
    };

    Ok(aws_sdk_s3::types::ObjectLockConfiguration::builder()
        .object_lock_enabled(aws_sdk_s3::types::ObjectLockEnabled::Enabled)
        .set_rule(rule)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::{ObjectLockConfiguration, ObjectLockRetentionMode};

    #[test]
    fn test_sdk_configuration_keeps_the_default_retention() {
        let expected = ObjectLockConfiguration {
            object_lock_enabled: true,
            default_retention: Some(DefaultRetention {
                mode: ObjectLockRetentionMode::Governance,
                days: None,
                years: Some(7),
            }),
        };

        // Reading the request body back with the getter's mapping must
        // return the configuration that was asked for.
        let configuration = sdk_configuration(expected.default_retention.as_ref()).unwrap();
        assert_eq!(ObjectLockConfiguration::from_sdk(&configuration), expected);
    }

    #[test]
    fn test_sdk_configuration_without_a_rule_removes_the_default_retention() {
        let configuration = sdk_configuration(None).unwrap();

        assert_eq!(
            ObjectLockConfiguration::from_sdk(&configuration),
            ObjectLockConfiguration {
                object_lock_enabled: true,
                default_retention: None,
            }
        );
    }

    #[test]
    fn test_sdk_configuration_refuses_an_invalid_period() {
        let error = sdk_configuration(Some(&DefaultRetention {
            mode: ObjectLockRetentionMode::Compliance,
            days: Some(1),
            years: Some(1),
        }))
        .unwrap_err();

        assert!(error.to_string().contains("cannot use both"), "{error}");
    }
}
