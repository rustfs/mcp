use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::{LoggingConfiguration, validate_logging_configuration};

impl S3Client {
    /// Enable server access logging for a bucket.
    pub async fn put_bucket_logging(
        &self,
        bucket_name: &str,
        configuration: &LoggingConfiguration,
    ) -> Result<()> {
        info!(
            "Enabling access logging of bucket '{}' into bucket '{}' with prefix '{:?}'",
            bucket_name, configuration.target_bucket, configuration.target_prefix
        );

        // Checked before anything is sent, so that S3 never receives a
        // configuration it cannot apply.
        validate_logging_configuration(configuration)?;

        // The request replaces the whole configuration: one that also
        // carries settings this tool does not model (log-object access
        // grants or a partitioned key format) would silently lose them, so
        // such a bucket is refused instead.
        let current = self.get_bucket_logging(bucket_name).await?;
        if !current.unshown_settings.is_empty() {
            anyhow::bail!(
                "The current logging configuration of bucket '{bucket_name}' also holds {} \
                 (settings this tool does not show): put_bucket_logging replaces the whole \
                 configuration and would drop them. Manage the logging of the bucket with \
                 the storage backend's own tooling, or remove the configuration first \
                 (delete_bucket_logging) and set it up anew",
                current.unshown_settings.join(" and ")
            );
        }

        // S3 requires the prefix element even when it is empty, and an
        // empty prefix means the root of the target bucket.
        let enabled = aws_sdk_s3::types::LoggingEnabled::builder()
            .target_bucket(&configuration.target_bucket)
            .target_prefix(configuration.target_prefix.as_deref().unwrap_or_default())
            .build()
            .context("Failed to build the logging configuration")?;
        let status = aws_sdk_s3::types::BucketLoggingStatus::builder()
            .logging_enabled(enabled)
            .build();

        self.client
            .put_bucket_logging()
            .bucket(bucket_name)
            .bucket_logging_status(status)
            .send()
            .await
            .context(format!(
                "Failed to set the logging configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Access logging of bucket '{}' enabled into bucket '{}'",
            bucket_name, configuration.target_bucket
        );

        Ok(())
    }
}
