use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::super::S3Client;
use super::{AccessControlPolicy, CannedAcl};

/// Result payload for setting a canned ACL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclUpdate {
    /// The ACL that was asked for.
    pub requested: CannedAcl,
    /// The policy the storage backend reported when the ACL was read back
    /// right after it was set; `None` when that read failed.
    pub read_back: Option<AccessControlPolicy>,
    /// Whether the read-back policy shows exactly the access the ACL grants.
    pub confirmed: bool,
}

impl S3Client {
    /// Apply a canned ACL to a bucket, then read the bucket's policy back.
    ///
    /// Some S3-compatible servers answer the request with success without
    /// applying the ACL, so the policy read afterwards is what tells whether
    /// it took effect.
    pub async fn put_bucket_acl(&self, bucket_name: &str, acl: CannedAcl) -> Result<AclUpdate> {
        info!(
            "Setting the ACL of bucket '{}' to '{}'",
            bucket_name,
            acl.as_str()
        );

        // Checked before anything is sent: S3 defines no bucket-owner ACL
        // for buckets, and some servers accept one regardless.
        acl.validate_for_bucket()?;

        self.client
            .put_bucket_acl()
            .bucket(bucket_name)
            .acl(acl.to_bucket_sdk())
            .send()
            .await
            .context(format!(
                "Failed to set the ACL of bucket '{bucket_name}' to '{}'",
                acl.as_str()
            ))?;

        // The ACL was applied by now; a failed read must not turn that into
        // an error, it only leaves the effect unconfirmed.
        let read_back = self
            .bucket_acl(bucket_name)
            .await
            .ok()
            .map(|acl| acl.policy);
        let confirmed = read_back
            .as_ref()
            .is_some_and(|policy| policy.confirms(acl, None));
        if !confirmed {
            warn!(
                "The ACL of bucket '{}' was accepted, but reading it back does not show it",
                bucket_name
            );
        }

        debug!("ACL of bucket '{}' set to '{}'", bucket_name, acl.as_str());
        Ok(AclUpdate {
            requested: acl,
            read_back,
            confirmed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(confirmed: bool) -> AclUpdate {
        AclUpdate {
            requested: CannedAcl::PublicRead,
            read_back: None,
            confirmed,
        }
    }

    #[test]
    fn test_acl_update_carries_the_verdict_it_was_given() {
        assert!(update(true).confirmed);
        assert!(!update(false).confirmed);
    }
}
