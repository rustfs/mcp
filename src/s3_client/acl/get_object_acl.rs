use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::AccessControlPolicy;

/// Result payload for reading an object version's access control policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectAcl {
    pub bucket: String,
    pub key: String,
    /// Version whose ACL was read; the current version when unset.
    pub version_id: Option<String>,
    pub policy: AccessControlPolicy,
}

impl S3Client {
    /// Read the access control policy of an object, or of one specific
    /// version of it: its owner and grants.
    pub async fn get_object_acl(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<ObjectAcl> {
        debug!(
            "Fetching the ACL of object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .get_object_acl()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let response = request.send().await.context(format!(
            "Failed to get the ACL of s3://{bucket_name}/{object_key}"
        ))?;

        let policy = AccessControlPolicy::from_sdk(response.owner(), response.grants())
            .with_context(|| {
                format!("Failed to read the ACL of s3://{bucket_name}/{object_key}")
            })?;

        debug!(
            "Object s3://{}/{} has {} ACL grant(s)",
            bucket_name,
            object_key,
            policy.grants.len()
        );

        Ok(ObjectAcl {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            version_id: version_id.map(|v| v.to_string()),
            policy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_acl_serialization() {
        let acl = ObjectAcl {
            bucket: "test-bucket".to_string(),
            key: "docs/report.pdf".to_string(),
            version_id: Some("v1".to_string()),
            policy: AccessControlPolicy {
                owner: None,
                grants: Vec::new(),
            },
        };

        let json = serde_json::to_string(&acl).unwrap();
        let deserialized: ObjectAcl = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.version_id.as_deref(), Some("v1"));
        assert!(deserialized.policy.grants.is_empty());
    }
}
