use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::WebsiteConfiguration;

impl S3Client {
    /// Set the website configuration of a bucket, replacing the current one.
    pub async fn put_bucket_website(
        &self,
        bucket_name: &str,
        configuration: &WebsiteConfiguration,
    ) -> Result<()> {
        info!(
            "Setting the website configuration of bucket '{}'",
            bucket_name
        );

        // Checked before anything is sent, so that S3 never sees a
        // configuration it cannot serve.
        let sdk_configuration = configuration.to_sdk()?;

        self.client
            .put_bucket_website()
            .bucket(bucket_name)
            .website_configuration(sdk_configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the website configuration of bucket '{bucket_name}'"
            ))?;

        info!("Website configuration of bucket '{}' replaced", bucket_name);

        Ok(())
    }
}
