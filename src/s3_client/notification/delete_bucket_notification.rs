use anyhow::{Context, Result};
use aws_sdk_s3::types::NotificationConfiguration;
use tracing::info;

use super::super::S3Client;

impl S3Client {
    /// Remove every notification rule of a bucket.
    ///
    /// Sent as an empty `put_bucket_notification_configuration` request
    /// rather than S3's own DELETE operation: S3 treats both alike, while
    /// some S3-compatible servers have no handler for the DELETE - RustFS
    /// 1.0.6 answers it by deleting an empty bucket.
    pub async fn delete_bucket_notification(&self, bucket_name: &str) -> Result<()> {
        info!(
            "Removing the notification configuration of bucket '{}'",
            bucket_name
        );

        self.client
            .put_bucket_notification_configuration()
            .bucket(bucket_name)
            .notification_configuration(NotificationConfiguration::builder().build())
            .send()
            .await
            .context(format!(
                "Failed to remove the notification configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Notification configuration of bucket '{}' removed",
            bucket_name
        );

        Ok(())
    }
}
