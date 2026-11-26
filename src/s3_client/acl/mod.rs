// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Access control lists (ACLs) of buckets and objects, set as canned ACLs.

mod get_bucket_acl;
mod get_object_acl;
mod put_bucket_acl;
mod put_object_acl;

pub use get_bucket_acl::BucketAcl;
pub use get_object_acl::ObjectAcl;
pub use put_bucket_acl::AclUpdate;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Group of everyone, anonymous requests included.
const ALL_USERS_URI: &str = "http://acs.amazonaws.com/groups/global/AllUsers";
/// Group of every authenticated account.
const AUTHENTICATED_USERS_URI: &str = "http://acs.amazonaws.com/groups/global/AuthenticatedUsers";

/// A canned ACL: a predefined set of grants that is applied as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum CannedAcl {
    /// Only the owner may access the resource.
    #[serde(rename = "private")]
    Private,
    /// Everyone, anonymous requests included, may read the resource.
    #[serde(rename = "public-read")]
    PublicRead,
    /// Everyone, anonymous requests included, may read, overwrite, and delete the resource.
    #[serde(rename = "public-read-write")]
    PublicReadWrite,
    /// Any authenticated account may read the resource.
    #[serde(rename = "authenticated-read")]
    AuthenticatedRead,
    /// Objects only: the bucket owner may read the object.
    #[serde(rename = "bucket-owner-read")]
    BucketOwnerRead,
    /// Objects only: the bucket owner gets full control of the object.
    #[serde(rename = "bucket-owner-full-control")]
    BucketOwnerFullControl,
}

impl CannedAcl {
    /// Lower-case name used in requests and log lines.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::PublicRead => "public-read",
            Self::PublicReadWrite => "public-read-write",
            Self::AuthenticatedRead => "authenticated-read",
            Self::BucketOwnerRead => "bucket-owner-read",
            Self::BucketOwnerFullControl => "bucket-owner-full-control",
        }
    }

    /// Who may access the resource besides its owner.
    pub fn effect(&self) -> &'static str {
        match self {
            Self::Private => "Only the owner may access the resource.",
            Self::PublicRead => "Everyone, anonymous requests included, may read the resource.",
            Self::PublicReadWrite => {
                "Everyone, anonymous requests included, may read, overwrite, and delete the \
                 resource."
            }
            Self::AuthenticatedRead => "Any authenticated account may read the resource.",
            Self::BucketOwnerRead => {
                "The bucket owner may read the object; only the object owner controls it."
            }
            Self::BucketOwnerFullControl => "The bucket owner gets full control of the object.",
        }
    }

    /// Whether the ACL is defined for buckets; the two bucket-owner ACLs
    /// exist for objects only.
    pub fn is_bucket_acl(&self) -> bool {
        !matches!(self, Self::BucketOwnerRead | Self::BucketOwnerFullControl)
    }

    /// Refuse an ACL S3 does not define for buckets.
    pub fn validate_for_bucket(&self) -> Result<()> {
        if self.is_bucket_acl() {
            return Ok(());
        }
        anyhow::bail!(
            "The '{}' ACL can only be applied to an object; for a bucket use private, \
             public-read, public-read-write, or authenticated-read",
            self.as_str()
        )
    }

    fn to_bucket_sdk(self) -> aws_sdk_s3::types::BucketCannedAcl {
        match self {
            Self::Private => aws_sdk_s3::types::BucketCannedAcl::Private,
            Self::PublicRead => aws_sdk_s3::types::BucketCannedAcl::PublicRead,
            Self::PublicReadWrite => aws_sdk_s3::types::BucketCannedAcl::PublicReadWrite,
            Self::AuthenticatedRead => aws_sdk_s3::types::BucketCannedAcl::AuthenticatedRead,
            // Rejected by `validate_for_bucket` before a request is built.
            Self::BucketOwnerRead | Self::BucketOwnerFullControl => {
                aws_sdk_s3::types::BucketCannedAcl::Private
            }
        }
    }

    fn to_object_sdk(self) -> aws_sdk_s3::types::ObjectCannedAcl {
        match self {
            Self::Private => aws_sdk_s3::types::ObjectCannedAcl::Private,
            Self::PublicRead => aws_sdk_s3::types::ObjectCannedAcl::PublicRead,
            Self::PublicReadWrite => aws_sdk_s3::types::ObjectCannedAcl::PublicReadWrite,
            Self::AuthenticatedRead => aws_sdk_s3::types::ObjectCannedAcl::AuthenticatedRead,
            Self::BucketOwnerRead => aws_sdk_s3::types::ObjectCannedAcl::BucketOwnerRead,
            Self::BucketOwnerFullControl => {
                aws_sdk_s3::types::ObjectCannedAcl::BucketOwnerFullControl
            }
        }
    }

    /// The grants to predefined groups that S3 stores for this ACL. The
    /// owner's own full control is not listed: it belongs to every canned
    /// ACL and cannot tell them apart.
    fn expected_group_grants(self) -> &'static [(&'static str, AclPermission)] {
        const READ: AclPermission = AclPermission::Read;
        const WRITE: AclPermission = AclPermission::Write;
        match self {
            Self::PublicRead => &[(ALL_USERS_URI, READ)],
            Self::PublicReadWrite => &[(ALL_USERS_URI, READ), (ALL_USERS_URI, WRITE)],
            Self::AuthenticatedRead => &[(AUTHENTICATED_USERS_URI, READ)],
            Self::Private | Self::BucketOwnerRead | Self::BucketOwnerFullControl => &[],
        }
    }
}

/// Whether a grant names exactly `grantee`, ignoring a display name the
/// storage backend may attach to the owner's grant.
fn grant_names(grant: &AclGrantee, grantee: &AclGrantee) -> bool {
    match (grant, grantee) {
        (
            AclGrantee::User {
                id: given,
                display_name: _,
            },
            AclGrantee::User { id: expected, .. },
        ) => given == expected,
        (AclGrantee::Group { uri: given }, AclGrantee::Group { uri: expected }) => {
            given == expected
        }
        (AclGrantee::Email { email: given }, AclGrantee::Email { email: expected }) => {
            given == expected
        }
        _ => false,
    }
}

/// What a grant allows its grantee to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AclPermission {
    FullControl,
    Read,
    Write,
    ReadAcp,
    WriteAcp,
}

impl AclPermission {
    /// Upper-case name used in tool output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullControl => "FULL_CONTROL",
            Self::Read => "READ",
            Self::Write => "WRITE",
            Self::ReadAcp => "READ_ACP",
            Self::WriteAcp => "WRITE_ACP",
        }
    }

    /// Map the SDK enum; `None` for a value this crate does not model.
    fn from_sdk(permission: &aws_sdk_s3::types::Permission) -> Option<Self> {
        match permission {
            aws_sdk_s3::types::Permission::FullControl => Some(Self::FullControl),
            aws_sdk_s3::types::Permission::Read => Some(Self::Read),
            aws_sdk_s3::types::Permission::Write => Some(Self::Write),
            aws_sdk_s3::types::Permission::ReadAcp => Some(Self::ReadAcp),
            aws_sdk_s3::types::Permission::WriteAcp => Some(Self::WriteAcp),
            _ => None,
        }
    }
}

/// Who receives a permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AclGrantee {
    /// An account, identified by its canonical user ID.
    User {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },
    /// A predefined group of accounts, named by its URI.
    Group { uri: String },
    /// An account given by email address (a legacy form).
    Email { email: String },
}

impl AclGrantee {
    /// Map the SDK values by the grantee type S3 reports for them.
    fn from_sdk(grantee: &aws_sdk_s3::types::Grantee) -> Self {
        match grantee.r#type() {
            aws_sdk_s3::types::Type::Group => Self::Group {
                uri: grantee.uri().unwrap_or_default().to_string(),
            },
            aws_sdk_s3::types::Type::AmazonCustomerByEmail => Self::Email {
                email: grantee.email_address().unwrap_or_default().to_string(),
            },
            _ => Self::User {
                id: grantee.id().unwrap_or_default().to_string(),
                display_name: grantee.display_name().map(|name| name.to_string()),
            },
        }
    }

    /// Whether the storage backend named the grantee. A grant whose grantee
    /// carries no ID, URI, or email tells nothing about who may access the
    /// resource.
    pub fn is_identified(&self) -> bool {
        match self {
            Self::User { id, .. } => !id.is_empty(),
            Self::Group { uri } => !uri.is_empty(),
            Self::Email { email } => !email.is_empty(),
        }
    }

    /// Display name for tool output.
    pub fn label(&self) -> String {
        match self {
            Self::User {
                id,
                display_name: Some(name),
            } => format!("{name} ({id})"),
            Self::User { id, .. } if id.is_empty() => "not reported by the storage backend".into(),
            Self::User { id, .. } => id.clone(),
            Self::Group { uri } if uri == ALL_USERS_URI => "Everyone".into(),
            Self::Group { uri } if uri == AUTHENTICATED_USERS_URI => "Authenticated users".into(),
            Self::Group { uri } if uri.is_empty() => "not reported by the storage backend".into(),
            Self::Group { uri } => uri.clone(),
            Self::Email { email } => email.clone(),
        }
    }
}

/// One permission given to one grantee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AclGrant {
    pub grantee: AclGrantee,
    pub permission: AclPermission,
}

/// Owner of a bucket or object version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AclOwner {
    /// Canonical user ID of the owning account.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// The access control policy of a bucket or object version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccessControlPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<AclOwner>,
    pub grants: Vec<AclGrant>,
}

impl AccessControlPolicy {
    /// Copy the SDK values; fails when a grant carries no grantee or no
    /// permission, which cannot be represented here.
    fn from_sdk(
        owner: Option<&aws_sdk_s3::types::Owner>,
        grants: &[aws_sdk_s3::types::Grant],
    ) -> Result<Self> {
        let mut mapped = Vec::new();
        for grant in grants {
            let grantee = grant
                .grantee()
                .context("The ACL holds a grant without a grantee")?;
            let permission = grant
                .permission()
                .and_then(AclPermission::from_sdk)
                .context("The ACL holds a grant without a known permission")?;
            mapped.push(AclGrant {
                grantee: AclGrantee::from_sdk(grantee),
                permission,
            });
        }

        Ok(Self {
            owner: owner.map(|owner| AclOwner {
                id: owner.id().unwrap_or_default().to_string(),
                display_name: owner.display_name().map(|name| name.to_string()),
            }),
            grants: mapped,
        })
    }

    /// Whether every grant names its grantee.
    pub fn grantees_are_identified(&self) -> bool {
        self.grants
            .iter()
            .all(|grant| grant.grantee.is_identified())
    }

    /// Whether the policy is exactly what `acl` produces: every grantee is
    /// named, the owner (or, for the two object-only bucket-owner ACLs, the
    /// bucket owner given as `bucket_owner`) holds the required permission,
    /// the ACL's group grants are present, and nothing else is. A storage
    /// backend that answers ACL requests without applying them fails this
    /// check, as does access that should have been revoked but remains.
    ///
    /// `bucket_owner` is only needed for the bucket-owner ACLs; when the
    /// backend does not report the bucket owner's canonical ID, they stay
    /// unconfirmed rather than being reported as applied.
    pub fn confirms(&self, acl: CannedAcl, bucket_owner: Option<&str>) -> bool {
        let Ok(expected) = self.expected_grants(acl, bucket_owner) else {
            return false;
        };
        if !self.grantees_are_identified() {
            return false;
        }

        // Multiset comparison: the read-back must carry exactly the expected
        // grants, no fewer and no others.
        let mut actual: Vec<&AclGrant> = self.grants.iter().collect();
        'expected: for (grantee, permission) in &expected {
            for position in 0..actual.len() {
                if actual[position].permission == *permission
                    && grant_names(&actual[position].grantee, grantee)
                {
                    actual.swap_remove(position);
                    continue 'expected;
                }
            }
            return false;
        }
        actual.is_empty()
    }

    /// The grants a canned ACL produces, which the read-back must match
    /// exactly; fails when the identity a check would need is unknown.
    fn expected_grants(
        &self,
        acl: CannedAcl,
        bucket_owner: Option<&str>,
    ) -> Result<Vec<(AclGrantee, AclPermission)>> {
        let mut expected = Vec::new();
        match acl {
            CannedAcl::BucketOwnerRead | CannedAcl::BucketOwnerFullControl => {
                // The object owner keeps FULL_CONTROL.
                let owner = self.owner.as_ref().and_then(|owner| {
                    (!owner.id.is_empty()).then_some(owner.id.clone())
                })
                .context(
                    "the resource owner's canonical ID is unknown, so its FULL_CONTROL grant cannot be verified",
                )?;
                expected.push((
                    AclGrantee::User {
                        id: owner.clone(),
                        display_name: None,
                    },
                    AclPermission::FullControl,
                ));
                // The grantee added by the ACL is the bucket owner, a
                // different account whose canonical ID only the caller can
                // supply; when the backend does not report it, the ACL stays
                // unconfirmed rather than being reported as applied. The
                // same account as the object owner is already covered by the
                // FULL_CONTROL grant.
                let permission = if acl == CannedAcl::BucketOwnerRead {
                    AclPermission::Read
                } else {
                    AclPermission::FullControl
                };
                let bucket_owner = bucket_owner.filter(|id| !id.is_empty()).context(
                    "the bucket owner's canonical ID is unknown, so the grant to it cannot be verified",
                )?;
                if bucket_owner != owner {
                    expected.push((
                        AclGrantee::User {
                            id: bucket_owner.to_string(),
                            display_name: None,
                        },
                        permission,
                    ));
                }
            }
            _ => {
                let owner = self.owner.as_ref().and_then(|owner| {
                    (!owner.id.is_empty()).then_some(owner.id.clone())
                })
                .context(
                    "the resource owner's canonical ID is unknown, so its FULL_CONTROL grant cannot be verified",
                )?;
                expected.push((
                    AclGrantee::User {
                        id: owner,
                        display_name: None,
                    },
                    AclPermission::FullControl,
                ));
            }
        }
        for (uri, permission) in acl.expected_group_grants() {
            expected.push((AclGrantee::Group { uri: (*uri).into() }, *permission));
        }
        Ok(expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::{Grant, Grantee, Owner, Permission, Type};

    fn grant(kind: Type, permission: Permission) -> Grant {
        match kind {
            Type::Group => group_grant(ALL_USERS_URI, permission),
            _ => grant_to("owner-id", permission),
        }
    }

    fn group_grant(uri: &str, permission: Permission) -> Grant {
        let grantee = Grantee::builder()
            .r#type(Type::Group)
            .uri(uri)
            .build()
            .unwrap();
        Grant::builder()
            .grantee(grantee)
            .permission(permission)
            .build()
    }

    fn grant_to(id: &str, permission: Permission) -> Grant {
        let grantee = Grantee::builder()
            .r#type(Type::CanonicalUser)
            .id(id)
            .display_name("rustfs")
            .build()
            .unwrap();
        Grant::builder()
            .grantee(grantee)
            .permission(permission)
            .build()
    }

    fn policy(grants: Vec<Grant>) -> AccessControlPolicy {
        AccessControlPolicy::from_sdk(Some(&Owner::builder().id("owner-id").build()), &grants)
            .unwrap()
    }

    #[test]
    fn test_canned_acl_serialization() {
        for (acl, json) in [
            (CannedAcl::Private, r#""private""#),
            (CannedAcl::PublicRead, r#""public-read""#),
            (CannedAcl::PublicReadWrite, r#""public-read-write""#),
            (CannedAcl::AuthenticatedRead, r#""authenticated-read""#),
            (CannedAcl::BucketOwnerRead, r#""bucket-owner-read""#),
            (
                CannedAcl::BucketOwnerFullControl,
                r#""bucket-owner-full-control""#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&acl).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<CannedAcl>(json).unwrap(),
                acl,
                "{json}"
            );
        }
        assert!(serde_json::from_str::<CannedAcl>(r#""everyone""#).is_err());
        assert!(serde_json::from_str::<CannedAcl>(r#""Public-Read""#).is_err());
    }

    #[test]
    fn test_canned_acl_text() {
        assert_eq!(CannedAcl::PublicRead.as_str(), "public-read");
        assert!(CannedAcl::PublicRead.effect().contains("anonymous"));
        assert!(CannedAcl::PublicReadWrite.effect().contains("delete"));
        assert!(
            CannedAcl::AuthenticatedRead
                .effect()
                .contains("authenticated")
        );
        assert!(CannedAcl::Private.effect().contains("Only the owner"));
    }

    #[test]
    fn test_canned_acl_validate_for_bucket() {
        for acl in [
            CannedAcl::Private,
            CannedAcl::PublicRead,
            CannedAcl::PublicReadWrite,
            CannedAcl::AuthenticatedRead,
        ] {
            assert!(acl.is_bucket_acl());
            assert!(acl.validate_for_bucket().is_ok(), "{acl:?}");
        }

        // The bucket-owner ACLs exist for objects only.
        for acl in [
            CannedAcl::BucketOwnerRead,
            CannedAcl::BucketOwnerFullControl,
        ] {
            assert!(!acl.is_bucket_acl());
            let error = acl.validate_for_bucket().unwrap_err().to_string();
            assert!(
                error.contains(&format!(
                    "'{}' ACL can only be applied to an object",
                    acl.as_str()
                )),
                "{error}"
            );
        }
    }

    #[test]
    fn test_canned_acl_sdk_round_trip() {
        for acl in [CannedAcl::Private, CannedAcl::PublicReadWrite] {
            let name = acl.as_str();
            assert_eq!(
                aws_sdk_s3::types::BucketCannedAcl::from(name).as_str(),
                name
            );
            assert_eq!(acl.to_bucket_sdk().as_str(), name);
        }
        for acl in [
            CannedAcl::Private,
            CannedAcl::PublicRead,
            CannedAcl::PublicReadWrite,
            CannedAcl::AuthenticatedRead,
            CannedAcl::BucketOwnerRead,
            CannedAcl::BucketOwnerFullControl,
        ] {
            assert_eq!(acl.to_object_sdk().as_str(), acl.as_str());
        }
    }

    #[test]
    fn test_acl_permission_serialization() {
        assert_eq!(
            serde_json::to_string(&AclPermission::FullControl).unwrap(),
            r#""FULL_CONTROL""#
        );
        assert_eq!(
            serde_json::to_string(&AclPermission::ReadAcp).unwrap(),
            r#""READ_ACP""#
        );
        let permission: AclPermission = serde_json::from_str(r#""WRITE""#).unwrap();
        assert_eq!(permission, AclPermission::Write);
        assert_eq!(AclPermission::Write.as_str(), "WRITE");
    }

    #[test]
    fn test_acl_grantee_label() {
        assert_eq!(
            AclGrantee::User {
                id: "owner-id".into(),
                display_name: Some("rustfs".into())
            }
            .label(),
            "rustfs (owner-id)"
        );
        assert_eq!(
            AclGrantee::User {
                id: "owner-id".into(),
                display_name: None
            }
            .label(),
            "owner-id"
        );
        assert_eq!(
            AclGrantee::Group {
                uri: ALL_USERS_URI.into()
            }
            .label(),
            "Everyone"
        );
        assert_eq!(
            AclGrantee::Group {
                uri: AUTHENTICATED_USERS_URI.into()
            }
            .label(),
            "Authenticated users"
        );
        assert_eq!(
            AclGrantee::Email {
                email: "a@example.com".into()
            }
            .label(),
            "a@example.com"
        );
    }

    #[test]
    fn test_acl_grantee_is_identified() {
        assert!(
            AclGrantee::User {
                id: "owner-id".into(),
                display_name: None
            }
            .is_identified()
        );
        assert!(
            AclGrantee::Group {
                uri: ALL_USERS_URI.into()
            }
            .is_identified()
        );
        // A grantee the storage backend left empty tells nothing.
        assert!(
            !AclGrantee::User {
                id: String::new(),
                display_name: None
            }
            .is_identified()
        );
        assert!(!AclGrantee::Group { uri: String::new() }.is_identified());
        assert!(
            !AclGrantee::Email {
                email: String::new()
            }
            .is_identified()
        );
    }

    #[test]
    fn test_access_control_policy_from_sdk() {
        let grants = vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
        ];
        let owner = Owner::builder()
            .id("owner-id")
            .display_name("rustfs")
            .build();

        let mapped = AccessControlPolicy::from_sdk(Some(&owner), &grants).unwrap();
        assert_eq!(
            mapped.owner,
            Some(AclOwner {
                id: "owner-id".into(),
                display_name: Some("rustfs".into())
            })
        );
        assert_eq!(mapped.grants.len(), 2);
        assert_eq!(
            mapped.grants[1],
            AclGrant {
                grantee: AclGrantee::Group {
                    uri: ALL_USERS_URI.into()
                },
                permission: AclPermission::Read
            }
        );

        // A grant without a grantee or without a permission cannot be shown.
        let permissionless = Grant::builder()
            .grantee(
                Grantee::builder()
                    .r#type(Type::Group)
                    .uri(ALL_USERS_URI)
                    .build()
                    .unwrap(),
            )
            .build();
        assert!(AccessControlPolicy::from_sdk(None, &[permissionless]).is_err());
    }

    #[test]
    fn test_access_control_policy_serialization() {
        let original = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
        ]);
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            json,
            r#"{"owner":{"id":"owner-id"},"grants":[{"grantee":{"kind":"user","id":"owner-id","display_name":"rustfs"},"permission":"FULL_CONTROL"},{"grantee":{"kind":"group","uri":"http://acs.amazonaws.com/groups/global/AllUsers"},"permission":"READ"}]}"#
        );
        let deserialized: AccessControlPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_confirms_accepts_a_policy_that_shows_exactly_the_acl() {
        // private: the owner's FULL_CONTROL and nothing else.
        assert!(
            policy(vec![grant(Type::CanonicalUser, Permission::FullControl)])
                .confirms(CannedAcl::Private, None)
        );
        // public-read: the owner's FULL_CONTROL plus everyone's READ.
        assert!(
            policy(vec![
                grant(Type::CanonicalUser, Permission::FullControl),
                grant(Type::Group, Permission::Read),
            ])
            .confirms(CannedAcl::PublicRead, None)
        );
        // Grants may arrive in any order.
        assert!(
            policy(vec![
                grant(Type::Group, Permission::Read),
                grant(Type::CanonicalUser, Permission::FullControl),
            ])
            .confirms(CannedAcl::PublicRead, None)
        );
    }

    #[test]
    fn test_confirms_rejects_access_that_should_have_been_revoked() {
        // An unchanged public-read policy must not confirm private.
        let public_read = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
        ]);
        assert!(!public_read.confirms(CannedAcl::Private, None));
        // Neither may an empty grant list.
        assert!(!policy(Vec::new()).confirms(CannedAcl::Private, None));
        // Nor public-read-write over a policy that still carries its write
        // grant.
        let public_read_write = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
            grant(Type::Group, Permission::Write),
        ]);
        assert!(!public_read_write.confirms(CannedAcl::PublicRead, None));
        assert!(!public_read_write.confirms(CannedAcl::Private, None));
    }

    #[test]
    fn test_confirms_rejects_a_missing_group_grant() {
        let private = policy(vec![grant(Type::CanonicalUser, Permission::FullControl)]);
        assert!(!private.confirms(CannedAcl::PublicRead, None));
        assert!(!private.confirms(CannedAcl::AuthenticatedRead, None));

        // public-read-write needs the write grant as well as the read grant.
        let read_only = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
        ]);
        assert!(!read_only.confirms(CannedAcl::PublicReadWrite, None));
        assert!(!read_only.confirms(CannedAcl::AuthenticatedRead, None));

        // A wrong group does not stand in for the expected one:
        // authenticated users reading is not everyone reading.
        let authenticated_read = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            group_grant(AUTHENTICATED_USERS_URI, Permission::Read),
        ]);
        assert!(!authenticated_read.confirms(CannedAcl::PublicRead, None));
        assert!(authenticated_read.confirms(CannedAcl::AuthenticatedRead, None));

        // The owner's FULL_CONTROL grant is part of every canned ACL.
        let ownerless = policy(vec![grant(Type::Group, Permission::Read)]);
        assert!(!ownerless.confirms(CannedAcl::PublicRead, None));
        assert!(!ownerless.confirms(CannedAcl::Private, None));

        // Extra user grants are extras, not part of a canned ACL.
        let extra_user = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant(Type::Group, Permission::Read),
            grant(Type::CanonicalUser, Permission::Read),
        ]);
        assert!(!extra_user.confirms(CannedAcl::PublicRead, None));
    }

    #[test]
    fn test_confirms_rejects_unnamed_grantees() {
        // RustFS 1.0.6 answers every ACL read with a full-control grant whose
        // grantee carries no ID.
        let unnamed = policy(vec![
            Grant::builder()
                .grantee(
                    Grantee::builder()
                        .r#type(Type::CanonicalUser)
                        .build()
                        .unwrap(),
                )
                .permission(Permission::FullControl)
                .build(),
        ]);
        assert!(!unnamed.grantees_are_identified());
        assert!(!unnamed.confirms(CannedAcl::Private, None));
    }

    #[test]
    fn test_confirms_verifies_bucket_owner_acls_against_the_bucket_owner() {
        let owner_read = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant_to("bucket-owner-id", Permission::Read),
        ]);
        assert!(owner_read.confirms(CannedAcl::BucketOwnerRead, Some("bucket-owner-id")));
        // The grant must reach the bucket owner, not just anybody.
        assert!(!owner_read.confirms(CannedAcl::BucketOwnerRead, Some("someone-else")));
        // Without the bucket owner's canonical ID the grant cannot be
        // verified, so the update stays unconfirmed instead of being
        // reported as applied.
        assert!(!owner_read.confirms(CannedAcl::BucketOwnerRead, None));

        let owner_full_control = policy(vec![
            grant(Type::CanonicalUser, Permission::FullControl),
            grant_to("bucket-owner-id", Permission::FullControl),
        ]);
        assert!(
            owner_full_control.confirms(CannedAcl::BucketOwnerFullControl, Some("bucket-owner-id"))
        );
        assert!(
            !owner_read.confirms(CannedAcl::BucketOwnerFullControl, Some("bucket-owner-id")),
            "READ is not the FULL_CONTROL the ACL grants"
        );
        // An unchanged private policy must not confirm the bucket-owner ACLs.
        assert!(
            !policy(vec![grant(Type::CanonicalUser, Permission::FullControl)])
                .confirms(CannedAcl::BucketOwnerRead, Some("bucket-owner-id"))
        );

        // When the bucket owner is the object owner, the owner's
        // FULL_CONTROL grant alone covers both ACLs.
        let own_bucket = policy(vec![grant(Type::CanonicalUser, Permission::FullControl)]);
        assert!(own_bucket.confirms(CannedAcl::BucketOwnerRead, Some("owner-id")));
        assert!(
            own_bucket.confirms(CannedAcl::BucketOwnerFullControl, Some("owner-id")),
            "FULL_CONTROL covers the READ the ACL would grant"
        );
        assert!(
            !own_bucket.confirms(CannedAcl::BucketOwnerRead, Some("someone-else")),
            "a different bucket owner still needs his own grant"
        );
    }
}
