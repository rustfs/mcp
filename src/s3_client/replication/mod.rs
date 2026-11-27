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

//! Bucket replication rules: which object versions are copied to which
//! other bucket.

mod delete_bucket_replication;
mod get_bucket_replication;
mod put_bucket_replication;
mod rule;
mod validate;

pub use delete_bucket_replication::ReplicationRemoval;
pub use get_bucket_replication::BucketReplicationConfig;
pub use rule::{ReplicationDestination, ReplicationRule, ReplicationRuleStatus};

/// Maximum number of rules in one replication configuration.
pub const MAX_REPLICATION_RULES: usize = 1000;

/// The replication tools against an endpoint that answers the way the
/// backends do, so that the real SDK delivers their answers.
#[cfg(test)]
mod backend_tests {
    use super::*;
    use crate::s3_client::fake_s3::{Answer, FakeS3, OK, RecordedRequest};
    use crate::s3_client::{
        ReplicationDestination, ReplicationRule, ReplicationRuleStatus, S3Client,
    };
    use std::collections::BTreeMap;

    const ROLE: &str = "arn:aws:iam::123456789012:role/replicator";

    /// What AWS S3, MinIO, and RustFS answer for a bucket without a
    /// replication configuration.
    fn no_replication() -> Answer {
        Answer::error(
            404,
            "ReplicationConfigurationNotFoundError",
            "The replication configuration was not found",
        )
    }

    /// A configuration whose second rule carries settings this tool does not
    /// model, and whose third selects by a lone tag.
    const RICH_CONFIGURATION: &str = "<ReplicationConfiguration>\
        <Role>arn:minio:replication::peer:source</Role>\
        <Rule><ID>backup</ID><Priority>10</Priority><Status>Enabled</Status>\
        <Filter><And><Prefix>logs/</Prefix>\
        <Tag><Key>team</Key><Value>storage</Value></Tag></And></Filter>\
        <DeleteMarkerReplication><Status>Enabled</Status></DeleteMarkerReplication>\
        <ExistingObjectReplication><Status>Disabled</Status></ExistingObjectReplication>\
        <Destination><Bucket>arn:aws:s3:::backup</Bucket><StorageClass>STANDARD_IA</StorageClass>\
        <Metrics><Status>Enabled</Status>\
        <EventThreshold><Minutes>15</Minutes></EventThreshold></Metrics>\
        <ReplicationTime><Status>Enabled</Status><Time><Minutes>15</Minutes></Time></ReplicationTime>\
        </Destination></Rule>\
        <Rule><ID>monitored</ID><Status>Enabled</Status>\
        <Filter><Prefix>docs/</Prefix></Filter>\
        <Destination><Bucket>arn:aws:s3:::archive</Bucket>\
        <ReplicationTime><Status>Enabled</Status><Time><Minutes>15</Minutes></Time></ReplicationTime>\
        </Destination></Rule>\
        <Rule><ID>tagged</ID><Status>Disabled</Status>\
        <Filter><Tag><Key>env</Key><Value>prod</Value></Tag></Filter>\
        <Destination><Bucket>arn:aws:s3:::archive</Bucket></Destination></Rule>\
        </ReplicationConfiguration>";

    /// A configuration that survives a removal, as the rules of a site
    /// replication do on RustFS.
    const SURVIVING_CONFIGURATION: &str = "<ReplicationConfiguration>\
        <Role>arn:rustfs:replication::peer:source</Role>\
        <Rule><ID>site-repl-peer</ID><Status>Enabled</Status>\
        <Filter><Prefix>docs/</Prefix></Filter>\
        <Destination><Bucket>arn:rustfs:replication::peer:source</Bucket></Destination></Rule>\
        </ReplicationConfiguration>";

    /// A minimal valid rule: copy everything to the backup bucket.
    fn rule(id: &str, priority: i32) -> ReplicationRule {
        ReplicationRule {
            id: Some(id.to_string()),
            status: ReplicationRuleStatus::Enabled,
            priority: Some(priority),
            prefix: None,
            tags: None,
            delete_marker_replication: None,
            existing_object_replication: None,
            destination: ReplicationDestination {
                bucket: "arn:aws:s3:::backup".to_string(),
                storage_class: None,
            },
        }
    }

    /// The error and everything behind it, the way a tool response shows it.
    fn shown(error: anyhow::Error) -> String {
        format!("{error:#}")
    }

    /// The method of each request, and whether it addressed the replication
    /// subresource.
    fn shape(requests: &[RecordedRequest]) -> Vec<(&str, bool)> {
        requests
            .iter()
            .map(|request| (request.method.as_str(), request.has("replication")))
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_reports_a_bucket_without_replication() {
        let endpoint = FakeS3::start(|_| no_replication());
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let config = client.get_bucket_replication("plain").await.unwrap();
        assert_eq!(config.bucket, "plain");
        assert_eq!(config.role, None);
        assert!(config.rules.is_empty());
        assert!(config.unmodeled_settings.is_empty());
        assert_eq!(shape(&endpoint.requests()), [("GET", true)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_maps_the_configuration_and_names_unmodeled_settings() {
        let endpoint = FakeS3::start(|_| Answer::ok(RICH_CONFIGURATION));
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let config = client.get_bucket_replication("rich").await.unwrap();
        assert_eq!(
            config.role.as_deref(),
            Some("arn:minio:replication::peer:source")
        );
        assert_eq!(config.rules.len(), 3);

        let backup = &config.rules[0];
        assert_eq!(backup.id.as_deref(), Some("backup"));
        assert_eq!(backup.priority, Some(10));
        assert_eq!(backup.prefix.as_deref(), Some("logs/"));
        assert_eq!(
            backup.tags,
            Some(BTreeMap::from([(
                "team".to_string(),
                "storage".to_string()
            )]))
        );
        assert_eq!(backup.delete_marker_replication, Some(true));
        assert_eq!(backup.existing_object_replication, Some(false));
        assert_eq!(
            backup.destination,
            ReplicationDestination {
                bucket: "arn:aws:s3:::backup".to_string(),
                storage_class: Some("STANDARD_IA".to_string()),
            }
        );

        // A lone tag is read back as a tag, and a status of "Disabled" as
        // disabled.
        let tagged = &config.rules[2];
        assert_eq!(tagged.status, ReplicationRuleStatus::Disabled);
        assert!(tagged.prefix.is_none());
        assert_eq!(
            tagged.tags,
            Some(BTreeMap::from([("env".to_string(), "prod".to_string())]))
        );

        // Only the two rules carrying settings this tool cannot write back
        // are named, each by what it carries.
        assert_eq!(
            config.unmodeled_settings,
            [
                "rule 'backup': replication metrics, replication time control",
                "rule 'monitored': replication time control"
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_keeps_other_failures() {
        let endpoint = FakeS3::start(|request| match request.bucket.as_str() {
            "absent" => Answer::error(404, "NoSuchBucket", "The specified bucket does not exist"),
            "denied" => Answer::error(403, "AccessDenied", "Access Denied"),
            _ => Answer::error(500, "InternalError", "We encountered an internal error."),
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let error = shown(client.get_bucket_replication("absent").await.unwrap_err());
        assert!(
            error.starts_with("Failed to get the replication configuration of bucket 'absent'"),
            "{error}"
        );
        assert!(error.contains("NoSuchBucket"), "{error}");

        let error = shown(client.get_bucket_replication("denied").await.unwrap_err());
        assert!(error.contains("AccessDenied"), "{error}");
        assert!(client.get_bucket_replication("broken").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_sends_the_configuration() {
        let endpoint = FakeS3::start(|request| {
            if request.method == "GET" {
                no_replication()
            } else {
                OK
            }
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let mut backup = rule("backup", 10);
        backup.prefix = Some("logs/".to_string());
        backup.tags = Some(BTreeMap::from([(
            "team".to_string(),
            "storage".to_string(),
        )]));
        backup.delete_marker_replication = Some(true);
        backup.destination.storage_class = Some("STANDARD_IA".to_string());
        client
            .put_bucket_replication("source", ROLE, &[backup, rule("docs", 20)])
            .await
            .unwrap();

        // The bucket's own configuration is read first, then the bucket is
        // confirmed, and only then is the configuration sent.
        let requests = endpoint.requests();
        assert_eq!(
            shape(&requests),
            [("GET", true), ("HEAD", false), ("PUT", true),]
        );
        assert!(requests.iter().all(|request| request.bucket == "source"));

        let body = requests[2].body_text();
        for expected in [
            "<Role>arn:aws:iam::123456789012:role/replicator</Role>",
            "<ID>backup</ID>",
            "<Priority>10</Priority>",
            "<Status>Enabled</Status>",
            "<Prefix>logs/</Prefix>",
            "<Key>team</Key>",
            "<Value>storage</Value>",
            "<DeleteMarkerReplication><Status>Enabled</Status></DeleteMarkerReplication>",
            "<Bucket>arn:aws:s3:::backup</Bucket>",
            "<StorageClass>STANDARD_IA</StorageClass>",
            "<ID>docs</ID>",
            "<Priority>20</Priority>",
        ] {
            assert!(body.contains(expected), "missing {expected} in {body}");
        }
        // Nothing this tool does not model is invented on the way out.
        assert!(!body.contains("<Metrics>"), "{body}");
        assert!(!body.contains("<ExistingObjectReplication>"), "{body}");
        assert!(!body.contains("<And>docs"), "{body}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_refuses_to_drop_unmodeled_settings() {
        let endpoint = FakeS3::start(|_| Answer::ok(RICH_CONFIGURATION));
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let error = shown(
            client
                .put_bucket_replication("rich", ROLE, &[rule("mine", 1)])
                .await
                .unwrap_err(),
        );
        assert!(
            error.contains("The current replication configuration of bucket 'rich' also holds rule 'backup': replication metrics, replication time control; rule 'monitored': replication time control"),
            "{error}"
        );
        assert!(error.contains("would drop them"), "{error}");

        // Nothing was sent: neither a confirmation of the bucket nor the
        // replacement.
        assert_eq!(shape(&endpoint.requests()), [("GET", true)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_sends_nothing_for_an_invalid_configuration() {
        let endpoint = FakeS3::start(|_| OK);
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // Two rules that both count as priority 0.
        let error = shown(
            client
                .put_bucket_replication("source", ROLE, &[rule("a", 0), rule("b", 0)])
                .await
                .unwrap_err(),
        );
        assert!(
            error.contains("the priority 0 is already used by rule 'a'"),
            "{error}"
        );

        // The destination is the source bucket itself.
        let mut itself = rule("self", 1);
        itself.destination.bucket = "arn:aws:s3:::source".to_string();
        assert!(
            client
                .put_bucket_replication("source", ROLE, &[itself])
                .await
                .is_err()
        );
        assert!(endpoint.requests().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_checks_the_bucket_before_replacing_anything() {
        // A backend that answers "no replication configuration" for a bucket
        // that does not exist would otherwise take the configuration.
        let endpoint = FakeS3::start(|request| {
            if request.method == "HEAD" {
                Answer::error(404, "NoSuchBucket", "The specified bucket does not exist")
            } else {
                no_replication()
            }
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let error = shown(
            client
                .put_bucket_replication("ghost", ROLE, &[rule("a", 1)])
                .await
                .unwrap_err(),
        );
        assert!(
            error.contains("the bucket does not exist or is not accessible"),
            "{error}"
        );
        assert_eq!(
            shape(&endpoint.requests()),
            [("GET", true), ("HEAD", false)]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_removal_reports_what_the_backend_kept() {
        let endpoint =
            FakeS3::start(
                |request| match (request.method.as_str(), request.bucket.as_str()) {
                    ("DELETE", "denied") => Answer::error(403, "AccessDenied", "Access Denied"),
                    ("DELETE", _) => OK,
                    (_, "gone") => no_replication(),
                    (_, "site") => Answer::ok(SURVIVING_CONFIGURATION),
                    (_, "unreadable") => {
                        Answer::error(500, "InternalError", "We encountered an internal error.")
                    }
                    _ => OK,
                },
            );
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // Nothing is left: what AWS S3 and MinIO answer after the removal.
        assert_eq!(
            client.delete_bucket_replication("gone").await.unwrap(),
            ReplicationRemoval::Removed
        );

        // RustFS keeps the rules of a site replication across the request.
        let ReplicationRemoval::RulesRemain(config) =
            client.delete_bucket_replication("site").await.unwrap()
        else {
            panic!("the surviving rules are reported");
        };
        assert_eq!(config.bucket, "site");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id.as_deref(), Some("site-repl-peer"));

        // The configuration could not be read back: the request went
        // through, and the text says why nothing was confirmed.
        let ReplicationRemoval::Unconfirmed(reason) = client
            .delete_bucket_replication("unreadable")
            .await
            .unwrap()
        else {
            panic!("the failed read-back is reported as such");
        };
        assert!(reason.contains("InternalError"), "{reason}");

        // Every removal read the configuration back afterwards.
        assert_eq!(
            shape(&endpoint.requests()),
            [
                ("DELETE", true),
                ("GET", true),
                ("DELETE", true),
                ("GET", true),
                ("DELETE", true),
                ("GET", true),
            ]
        );

        // A refused removal is an error, and nothing is read back.
        let error = shown(
            client
                .delete_bucket_replication("denied")
                .await
                .unwrap_err(),
        );
        assert!(error.contains("AccessDenied"), "{error}");
        assert_eq!(endpoint.requests().len(), 7);
    }
}
