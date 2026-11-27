use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::{ObjectLockRead, ObjectRetention, UnsetLockReason};

/// Result payload for reading the retention of an object version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectRetentionInfo {
    pub bucket: String,
    pub key: String,
    /// Version whose retention was read; the current version when unset.
    pub version_id: Option<String>,
    /// `None` when no retention period applies to the object version.
    pub retention: Option<ObjectRetention>,
    /// Why no retention period applies; `None` when one was read.
    pub unset_reason: Option<UnsetLockReason>,
}

impl S3Client {
    /// Read the retention of an object, or of one specific version of it.
    pub async fn get_object_retention(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<ObjectRetentionInfo> {
        debug!(
            "Fetching retention for object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .get_object_retention()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let failure = format!("Failed to get the retention of s3://{bucket_name}/{object_key}");
        match self
            .read_object_lock_setting(request.send().await, bucket_name, &failure)
            .await?
        {
            ObjectLockRead::Answered(response) => {
                // Some implementations answer an unretained version with an
                // empty retention instead of an error.
                let retention = response.retention().and_then(ObjectRetention::from_sdk);
                debug!(
                    "Object s3://{}/{} has retention: {:?}",
                    bucket_name, object_key, retention
                );

                Ok(ObjectRetentionInfo {
                    bucket: bucket_name.to_string(),
                    key: object_key.to_string(),
                    version_id: version_id.map(|v| v.to_string()),
                    unset_reason: retention.is_none().then_some(UnsetLockReason::NotSet),
                    retention,
                })
            }
            ObjectLockRead::Unset(unset_reason) => Ok(ObjectRetentionInfo {
                bucket: bucket_name.to_string(),
                key: object_key.to_string(),
                version_id: version_id.map(|v| v.to_string()),
                retention: None,
                unset_reason: Some(unset_reason),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::ObjectLockRetentionMode;

    #[test]
    fn test_object_retention_info_serialization() {
        let info = ObjectRetentionInfo {
            bucket: "test-bucket".to_string(),
            key: "docs/report.pdf".to_string(),
            version_id: Some("v1".to_string()),
            retention: Some(ObjectRetention {
                mode: ObjectLockRetentionMode::Compliance,
                retain_until: "2030-01-01T00:00:00Z".to_string(),
            }),
            unset_reason: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ObjectRetentionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.version_id.as_deref(), Some("v1"));
        assert_eq!(deserialized.retention, info.retention);
        assert_eq!(deserialized.unset_reason, None);
    }

    #[test]
    fn test_object_retention_info_without_retention_serialization() {
        // A version without a retention period is reported as None, with
        // the reason stating whether the version or the bucket is the cause.
        let json = r#"{
            "bucket": "test-bucket",
            "key": "docs/report.pdf",
            "version_id": null,
            "retention": null,
            "unset_reason": "bucket_not_enabled"
        }"#;
        let deserialized: ObjectRetentionInfo = serde_json::from_str(json).unwrap();

        assert!(deserialized.version_id.is_none());
        assert!(deserialized.retention.is_none());
        assert_eq!(
            deserialized.unset_reason,
            Some(UnsetLockReason::BucketNotEnabled)
        );
        assert_eq!(
            serde_json::to_string(&UnsetLockReason::NotSet).unwrap(),
            r#""not_set""#
        );
    }
}
