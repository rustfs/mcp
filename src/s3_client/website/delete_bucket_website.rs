use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;

impl S3Client {
    /// Delete a bucket's website configuration.
    ///
    /// Sent as S3's own DELETE operation: unlike the notification and
    /// logging configurations, storage backends handle it for this resource
    /// without mistaking it for a bucket deletion.
    pub async fn delete_bucket_website(&self, bucket_name: &str) -> Result<()> {
        info!(
            "Deleting the website configuration of bucket '{}'",
            bucket_name
        );

        self.client
            .delete_bucket_website()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to delete the website configuration of bucket '{bucket_name}'"
            ))?;

        info!("Website configuration of bucket '{}' deleted", bucket_name);

        Ok(())
    }
}
