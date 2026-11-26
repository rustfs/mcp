use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::AccessControlPolicy;

/// Result payload for reading a bucket's access control policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketAcl {
    pub bucket: String,
    pub policy: AccessControlPolicy,
}

impl S3Client {
    /// Read a bucket's access control policy: its owner and grants.
    pub async fn get_bucket_acl(&self, bucket_name: &str) -> Result<BucketAcl> {
        debug!("Fetching the ACL of bucket '{}'", bucket_name);

        self.bucket_acl(bucket_name).await
    }

    /// The access control policy of a bucket, read back through `GET ?acl`.
    pub(super) async fn bucket_acl(&self, bucket_name: &str) -> Result<BucketAcl> {
        let response = self
            .client
            .get_bucket_acl()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!("Failed to get the ACL of bucket '{bucket_name}'"))?;

        let policy = AccessControlPolicy::from_sdk(response.owner(), response.grants())
            .with_context(|| format!("Failed to read the ACL of bucket '{bucket_name}'"))?;

        debug!(
            "Bucket '{}' has {} ACL grant(s)",
            bucket_name,
            policy.grants.len()
        );

        Ok(BucketAcl {
            bucket: bucket_name.to_string(),
            policy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::{AclGrant, AclGrantee, AclPermission};

    #[test]
    fn test_bucket_acl_serialization() {
        let acl = BucketAcl {
            bucket: "test-bucket".to_string(),
            policy: AccessControlPolicy {
                owner: None,
                grants: vec![AclGrant {
                    grantee: AclGrantee::User {
                        id: "owner-id".to_string(),
                        display_name: None,
                    },
                    permission: AclPermission::FullControl,
                }],
            },
        };

        let json = serde_json::to_string(&acl).unwrap();
        let deserialized: BucketAcl = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.policy, acl.policy);
    }
}
