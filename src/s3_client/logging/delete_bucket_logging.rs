use anyhow::{Context, Result};
use aws_sdk_s3::types::BucketLoggingStatus;
use tracing::info;

use super::super::S3Client;

impl S3Client {
    /// Disable server access logging for a bucket.
    ///
    /// Sent as an empty `put_bucket_logging` request rather than S3's own
    /// DELETE operation: S3 treats both alike, while some S3-compatible
    /// servers have no handler for the DELETE - RustFS 1.0.6 answers it by
    /// deleting an empty bucket.
    pub async fn delete_bucket_logging(&self, bucket_name: &str) -> Result<()> {
        info!("Disabling access logging of bucket '{}'", bucket_name);

        self.client
            .put_bucket_logging()
            .bucket(bucket_name)
            .bucket_logging_status(BucketLoggingStatus::builder().build())
            .send()
            .await
            .context(format!(
                "Failed to remove the logging configuration of bucket '{bucket_name}'"
            ))?;

        info!("Access logging of bucket '{}' disabled", bucket_name);

        Ok(())
    }
}
