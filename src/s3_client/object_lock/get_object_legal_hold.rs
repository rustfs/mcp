use anyhow::{Context, Result};
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::{ObjectLockLegalHoldStatus, UNSET_OBJECT_LOCK_ERROR_CODE};

/// Result payload for reading the legal hold of an object version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectLegalHold {
    pub bucket: String,
    pub key: String,
    /// Version whose legal hold was read; the current version when unset.
    pub version_id: Option<String>,
    /// `None` when no legal hold is recorded for the object version.
    pub status: Option<ObjectLockLegalHoldStatus>,
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

        let response = match request.send().await {
            Ok(response) => response,
            // A version that never had a legal hold answers with a dedicated
            // error code on some S3 implementations; report that as "no legal
            // hold" instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some(UNSET_OBJECT_LOCK_ERROR_CODE) =>
            {
                return Ok(ObjectLegalHold {
                    bucket: bucket_name.to_string(),
                    key: object_key.to_string(),
                    version_id: version_id.map(|v| v.to_string()),
                    status: None,
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the legal hold of s3://{bucket_name}/{object_key}"
                ));
            }
        };

        // Other implementations answer a version without a legal hold with
        // an explicit OFF instead of an error.
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
            status,
        })
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
        };

        let json = serde_json::to_string(&hold).unwrap();
        let deserialized: ObjectLegalHold = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.status, Some(ObjectLockLegalHoldStatus::On));
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

        let off: ObjectLegalHold = serde_json::from_str(
            r#"{"bucket": "b", "key": "k", "version_id": null, "status": "OFF"}"#,
        )
        .unwrap();
        assert_eq!(off.status, Some(ObjectLockLegalHoldStatus::Off));
    }
}
