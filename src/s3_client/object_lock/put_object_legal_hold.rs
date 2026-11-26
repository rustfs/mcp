use anyhow::{Context, Result};
use tracing::info;

use super::super::S3Client;
use super::ObjectLockLegalHoldStatus;

impl S3Client {
    /// Set the legal hold of an object, or of one specific version of it.
    pub async fn put_object_legal_hold(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
        status: ObjectLockLegalHoldStatus,
    ) -> Result<()> {
        info!(
            "Setting legal hold on s3://{}/{} to {}",
            bucket_name,
            object_key,
            status.as_str()
        );

        let legal_hold = aws_sdk_s3::types::ObjectLockLegalHold::builder()
            .status(status.to_sdk())
            .build();

        let mut request = self
            .client
            .put_object_legal_hold()
            .bucket(bucket_name)
            .key(object_key)
            .legal_hold(legal_hold);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to set the legal hold of s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Legal hold of s3://{}/{} set to {}",
            bucket_name,
            object_key,
            status.as_str()
        );

        Ok(())
    }
}
