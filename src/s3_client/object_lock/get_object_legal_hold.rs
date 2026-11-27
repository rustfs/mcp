use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::{ObjectLockLegalHoldStatus, ObjectLockRead, UnsetLockReason};

/// Result payload for reading the legal hold of an object version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectLegalHold {
    pub bucket: String,
    pub key: String,
    /// Version whose legal hold was read; the current version when unset.
    pub version_id: Option<String>,
    /// `None` when no legal hold applies to the object version.
    pub status: Option<ObjectLockLegalHoldStatus>,
    /// Why no legal hold applies; `None` when a status was read.
    pub unset_reason: Option<UnsetLockReason>,
}

impl S3Client {
    /// Read the legal hold of an object, or of one specific version of it.
    pub async fn get_object_legal_hold(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<ObjectLegalHold> {
        debug!(
            "Fetching legal hold for object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .get_object_legal_hold()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let failure = format!("Failed to get the legal hold of s3://{bucket_name}/{object_key}");
        match self
            .read_object_lock_setting(request.send().await, bucket_name, &failure)
            .await?
        {
            ObjectLockRead::Answered(response) => {
                // Some implementations answer a version without a legal hold
                // with an explicit OFF instead of an error.
                let status = response
                    .legal_hold()
                    .and_then(|hold| hold.status())
                    .and_then(ObjectLockLegalHoldStatus::from_sdk);
                debug!(
                    "Object s3://{}/{} has legal hold: {:?}",
                    bucket_name, object_key, status
                );

                Ok(ObjectLegalHold {
                    bucket: bucket_name.to_string(),
                    key: object_key.to_string(),
                    version_id: version_id.map(|v| v.to_string()),
                    unset_reason: status.is_none().then_some(UnsetLockReason::NotSet),
                    status,
                })
            }
            ObjectLockRead::Unset(unset_reason) => Ok(ObjectLegalHold {
                bucket: bucket_name.to_string(),
                key: object_key.to_string(),
                version_id: version_id.map(|v| v.to_string()),
                status: None,
                unset_reason: Some(unset_reason),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_legal_hold_serialization() {
        let hold = ObjectLegalHold {
            bucket: "test-bucket".to_string(),
            key: "docs/report.pdf".to_string(),
            version_id: Some("v1".to_string()),
            status: Some(ObjectLockLegalHoldStatus::On),
            unset_reason: None,
        };

        let json = serde_json::to_string(&hold).unwrap();
        let deserialized: ObjectLegalHold = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.status, Some(ObjectLockLegalHoldStatus::On));
        assert_eq!(deserialized.unset_reason, None);
    }

    #[test]
    fn test_object_legal_hold_without_status_serialization() {
        // No recorded legal hold is reported as None, and an explicit OFF
        // stays distinguishable from it.
        let unset: ObjectLegalHold = serde_json::from_str(
            r#"{"bucket": "b", "key": "k", "version_id": null, "status": null}"#,
        )
        .unwrap();
        assert!(unset.status.is_none());
        assert_eq!(unset.unset_reason, None);

        let off: ObjectLegalHold = serde_json::from_str(
            r#"{"bucket": "b", "key": "k", "version_id": null, "status": "OFF"}"#,
        )
        .unwrap();
        assert_eq!(off.status, Some(ObjectLockLegalHoldStatus::Off));
    }
}
