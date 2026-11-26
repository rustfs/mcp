use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::{NotificationRule, sdk_configuration};

impl S3Client {
    /// Replace a bucket's notification configuration with the given rules.
    pub async fn put_bucket_notification(
        &self,
        bucket_name: &str,
        rules: &[NotificationRule],
    ) -> Result<()> {
        info!(
            "Setting the notification configuration of bucket '{}' to {} rule(s)",
            bucket_name,
            rules.len()
        );

        let configuration = sdk_configuration(rules)?;

        self.client
            .put_bucket_notification_configuration()
            .bucket(bucket_name)
            .notification_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the notification configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Notification configuration of bucket '{}' replaced with {} rule(s)",
            bucket_name,
            rules.len()
        );

        Ok(())
    }
}
