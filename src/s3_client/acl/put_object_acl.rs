use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use super::super::S3Client;
use super::{AclUpdate, CannedAcl};

impl S3Client {
    /// Apply a canned ACL to an object, or to one specific version of it,
    /// then read the policy back.
    ///
    /// Some S3-compatible servers answer the request with success without
    /// applying the ACL, so the policy read afterwards is what tells whether
    /// it took effect.
    pub async fn put_object_acl(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
        acl: CannedAcl,
    ) -> Result<AclUpdate> {
        info!(
            "Setting the ACL of s3://{}/{} to '{}'",
            bucket_name,
            object_key,
            acl.as_str()
        );

        let mut request = self
            .client
            .put_object_acl()
            .bucket(bucket_name)
            .key(object_key)
            .acl(acl.to_object_sdk());

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to set the ACL of s3://{bucket_name}/{object_key} to '{}'",
            acl.as_str()
        ))?;

        // The ACL was applied by now; a failed read must not turn that into
        // an error, it only leaves the effect unconfirmed.
        //
        // The two bucket-owner ACLs grant access to the bucket owner, a
        // different account, so his canonical ID is read from the bucket to
        // verify the grant; when the backend does not report it, those two
        // stay unconfirmed rather than being reported as applied.
        let bucket_owner = if matches!(
            acl,
            CannedAcl::BucketOwnerRead | CannedAcl::BucketOwnerFullControl
        ) {
            self.get_bucket_acl(bucket_name)
                .await
                .ok()
                .and_then(|acl| acl.policy.owner)
                .and_then(|owner| (!owner.id.is_empty()).then_some(owner.id))
        } else {
            None
        };

        let read_back = self
            .get_object_acl(bucket_name, object_key, version_id)
            .await
            .ok()
            .map(|acl| acl.policy);
        let confirmed = read_back
            .as_ref()
            .is_some_and(|policy| policy.confirms(acl, bucket_owner.as_deref()));
        if !confirmed {
            warn!(
                "The ACL of s3://{}/{} was accepted, but reading it back does not show it",
                bucket_name, object_key
            );
        }

        debug!(
            "ACL of s3://{}/{} set to '{}'",
            bucket_name,
            object_key,
            acl.as_str()
        );
        Ok(AclUpdate {
            requested: acl,
            read_back,
            confirmed,
        })
    }
}
