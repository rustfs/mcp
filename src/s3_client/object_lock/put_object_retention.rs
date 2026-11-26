use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::{ObjectRetention, now};

impl S3Client {
    /// Set the retention of an object, or of one specific version of it.
    ///
    /// Shortening a GOVERNANCE retention needs `bypass_governance_retention`
    /// and the `s3:BypassGovernanceRetention` permission; a COMPLIANCE
    /// retention cannot be changed until it expires.
    pub async fn put_object_retention(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
        retention: &ObjectRetention,
        bypass_governance_retention: bool,
    ) -> Result<()> {
        info!(
            "Setting retention on s3://{}/{} to {}",
            bucket_name,
            object_key,
            retention.label()
        );

        // Checked before anything is sent: the date must parse and lie ahead.
        let sdk_retention = retention.to_sdk(now())?;

        let mut request = self
            .client
            .put_object_retention()
            .bucket(bucket_name)
            .key(object_key)
            .retention(sdk_retention);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        if bypass_governance_retention {
            request = request.bypass_governance_retention(true);
        }

        request.send().await.context(format!(
            "Failed to set the retention of s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Retention of s3://{}/{} set to {}",
            bucket_name,
            object_key,
            retention.label()
        );

        Ok(())
    }
}
