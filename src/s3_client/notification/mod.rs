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

//! Bucket event notifications.

mod delete_bucket_notification;
mod get_bucket_notification;
mod put_bucket_notification;

pub use get_bucket_notification::BucketNotificationConfig;

use anyhow::{Context, Result};
use aws_sdk_s3::types::{
    Event, FilterRule, FilterRuleName, LambdaFunctionConfiguration, NotificationConfiguration,
    NotificationConfigurationFilter, QueueConfiguration, S3KeyFilter, TopicConfiguration,
};
use serde::{Deserialize, Serialize};

/// Largest identifier S3 accepts for one notification rule.
const MAX_NOTIFICATION_ID_LENGTH: usize = 255;

/// Where S3 delivers the events of a notification rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum NotificationDestination {
    /// AWS Lambda function.
    #[serde(rename = "lambda")]
    Lambda,
    /// SQS queue.
    #[serde(rename = "queue")]
    Queue,
    /// SNS topic.
    #[serde(rename = "topic")]
    Topic,
}

impl NotificationDestination {
    /// Lower-case name used in requests and log lines.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lambda => "lambda",
            Self::Queue => "queue",
            Self::Topic => "topic",
        }
    }

    /// Display name for tool output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Lambda => "Lambda function",
            Self::Queue => "SQS queue",
            Self::Topic => "SNS topic",
        }
    }
}

/// One notification rule: which object events are delivered where, for
/// which keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationRule {
    #[schemars(
        description = "Where the events are delivered: 'lambda' (AWS Lambda function), 'queue' (SQS queue), or 'topic' (SNS topic)"
    )]
    pub destination: NotificationDestination,
    #[schemars(
        description = "ARN of the destination. On AWS the ARN of the Lambda function, SQS queue, or SNS topic; on RustFS the ARN of a configured target, in the form arn:rustfs:sqs::<id>:<name> (e.g. arn:rustfs:sqs::primary:webhook)"
    )]
    pub arn: String,
    #[schemars(
        description = "S3 events that trigger a notification, e.g. [\"s3:ObjectCreated:*\"] or [\"s3:ObjectRemoved:Delete\"]"
    )]
    pub events: Vec<String>,
    /// Only keys that start with this prefix trigger the rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional key prefix the rule is limited to, e.g. 'images/' (defaults to every key)"
    )]
    pub prefix: Option<String>,
    /// Only keys that end with this suffix trigger the rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional key suffix the rule is limited to, e.g. '.jpg' (defaults to every key)"
    )]
    pub suffix: Option<String>,
    /// Optional unique identifier of the rule (S3 allows up to 255 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional unique identifier of the rule, e.g. 'image-uploads' (S3 allows up to 255 characters)"
    )]
    pub id: Option<String>,
}

impl NotificationRule {
    /// Name used in tool output and error messages: the ID when the rule has
    /// one, otherwise its 1-based position in the list it was submitted in.
    pub fn label(&self, index: usize) -> String {
        match &self.id {
            Some(id) => format!("rule '{id}'"),
            None => format!("rule {}", index + 1),
        }
    }

    /// The rule as one line for tool output.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{}: {} for {}",
            self.destination.label(),
            self.arn,
            self.events.join(", ")
        );
        match (&self.prefix, &self.suffix) {
            (Some(prefix), Some(suffix)) => {
                text.push_str(&format!(
                    " (keys with prefix '{prefix}' and suffix '{suffix}')"
                ));
            }
            (Some(prefix), None) => text.push_str(&format!(" (keys with prefix '{prefix}')")),
            (None, Some(suffix)) => text.push_str(&format!(" (keys with suffix '{suffix}')")),
            (None, None) => {}
        }
        text
    }

    fn sdk_events(&self) -> Vec<Event> {
        self.events
            .iter()
            .map(|event| Event::from(event.as_str()))
            .collect()
    }

    /// The key filter of the rule; `None` when it applies to every key.
    fn sdk_filter(&self) -> Option<NotificationConfigurationFilter> {
        let mut rules = Vec::new();
        if let Some(ref prefix) = self.prefix {
            rules.push(
                FilterRule::builder()
                    .name(FilterRuleName::Prefix)
                    .value(encode_key_filter(prefix))
                    .build(),
            );
        }
        if let Some(ref suffix) = self.suffix {
            rules.push(
                FilterRule::builder()
                    .name(FilterRuleName::Suffix)
                    .value(encode_key_filter(suffix))
                    .build(),
            );
        }
        if rules.is_empty() {
            return None;
        }
        Some(
            NotificationConfigurationFilter::builder()
                .key(S3KeyFilter::builder().set_filter_rules(Some(rules)).build())
                .build(),
        )
    }

    /// Map one SDK configuration; fails when its key filter carries a rule
    /// this crate does not model.
    fn from_sdk(
        destination: NotificationDestination,
        id: Option<&str>,
        arn: &str,
        events: &[Event],
        filter: Option<&NotificationConfigurationFilter>,
    ) -> Result<Self> {
        let (prefix, suffix) = key_filter(filter)?;
        Ok(Self {
            destination,
            arn: arn.to_string(),
            events: events
                .iter()
                .map(|event| event.as_str().to_string())
                .collect(),
            prefix,
            suffix,
            id: id.map(|id| id.to_string()),
        })
    }
}

/// Encode a literal key-filter value for the wire. S3 reads an encoded `+`
/// as a space, so a literal `+` must travel as `%2B`, a space as `+`, and a
/// `%` as `%25`; everything else outside S3's safe set travels as `%XX`
/// ([S3's filter encoding requirement](https://docs.aws.amazon.com/AmazonS3/latest/userguide/notification-how-to-filtering.html)).
fn encode_key_filter(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Decode a key-filter value from the wire, the inverse of
/// [`encode_key_filter`]. A value the storage backend sent that cannot be
/// decoded is an error rather than a silently different prefix or suffix.
fn decode_key_filter(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let hex = bytes
                    .get(index + 1..index + 3)
                    .context("The notification filter ends with an incomplete % escape")?;
                let hex = std::str::from_utf8(hex)
                    .context("The notification filter holds a malformed % escape")?;
                let byte = u8::from_str_radix(hex, 16)
                    .context("The notification filter holds a malformed % escape")?;
                decoded.push(byte);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).context("The notification filter is not valid UTF-8 once decoded")
}

/// Prefix and suffix of an SDK key filter.
fn key_filter(
    filter: Option<&NotificationConfigurationFilter>,
) -> Result<(Option<String>, Option<String>)> {
    let mut prefix = None;
    let mut suffix = None;
    let rules: &[FilterRule] = filter
        .and_then(|filter| filter.key())
        .map(|key| key.filter_rules())
        .unwrap_or(&[]);
    for rule in rules {
        match (rule.name(), rule.value()) {
            (Some(FilterRuleName::Prefix), Some(value)) => {
                prefix = Some(decode_key_filter(value)?);
            }
            (Some(FilterRuleName::Suffix), Some(value)) => {
                suffix = Some(decode_key_filter(value)?);
            }
            _ => anyhow::bail!("The notification filter holds a rule this crate does not model"),
        }
    }
    Ok((prefix, suffix))
}

/// Check a rule list before it is sent to S3, naming the first rule that
/// breaks a requirement.
pub fn validate_notification_rules(rules: &[NotificationRule]) -> Result<()> {
    if rules.is_empty() {
        anyhow::bail!(
            "At least one notification rule is required; to remove the configuration \
             call delete_bucket_notification instead"
        );
    }

    let mut ids: Vec<&str> = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        if !is_arn(&rule.arn) {
            anyhow::bail!(
                "Notification {} has an invalid ARN '{}': use the ARN of a Lambda function, SQS queue, or SNS topic on AWS, or of a configured target on RustFS (arn:rustfs:sqs::<id>:<name>)",
                rule.label(index),
                rule.arn
            );
        }
        if rule.events.is_empty() {
            anyhow::bail!(
                "Notification {} lists no events; add at least one such as s3:ObjectCreated:* or s3:ObjectRemoved:Delete",
                rule.label(index)
            );
        }
        for (field, value) in [("prefix", &rule.prefix), ("suffix", &rule.suffix)] {
            if value.as_deref() == Some("") {
                anyhow::bail!(
                    "Notification {} has an empty {field}; omit it to match every key",
                    rule.label(index)
                );
            }
        }
        if let Some(event) = rule.events.iter().find(|event| !is_s3_event(event)) {
            anyhow::bail!(
                "Notification {} lists '{}', which is not an S3 event name; use names such as s3:ObjectCreated:* or s3:ObjectRemoved:Delete",
                rule.label(index),
                event
            );
        }
        if let Some(id) = &rule.id {
            if id.chars().count() > MAX_NOTIFICATION_ID_LENGTH {
                anyhow::bail!(
                    "Notification {} has an ID of {} characters; S3 allows at most {MAX_NOTIFICATION_ID_LENGTH}",
                    rule.label(index),
                    id.chars().count()
                );
            }
            if ids.contains(&id.as_str()) {
                anyhow::bail!("Notification rule IDs must be unique, but '{id}' is used twice");
            }
            ids.push(id);
        }
    }

    Ok(())
}

/// Whether `value` is shaped like an ARN: `arn:<partition>:<service>`, then
/// the region, account, and resource fields. The region and the account may
/// be empty, as they are in the target names of RustFS
/// (`arn:rustfs:sqs::<id>:<name>`).
fn is_arn(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() >= 6
        && parts[0] == "arn"
        && !parts[1].is_empty()
        && !parts[2].is_empty()
        && !parts[5].is_empty()
}

/// Whether `event` is shaped like an S3 event name, `s3:<Type>` or
/// `s3:<Type>:<Detail>`. The exact names differ between storage backends and
/// gain new ones over time, so only the shape is checked: both components
/// must be present, nonempty, and free of whitespace, and separators may not
/// repeat (`s3:ObjectCreated:` and `s3:ObjectCreated::Put` are not names any
/// backend defines).
fn is_s3_event(event: &str) -> bool {
    let Some(rest) = event.strip_prefix("s3:") else {
        return false;
    };
    let mut components = rest.split(':');
    let (Some(kind), detail, extra) = (components.next(), components.next(), components.next())
    else {
        return false;
    };
    if extra.is_some() || kind.is_empty() || kind.chars().any(char::is_whitespace) {
        return false;
    }
    detail.is_none_or(|detail| !detail.is_empty() && !detail.chars().any(char::is_whitespace))
}

/// Build the request body of `put_bucket_notification_configuration`,
/// which groups the rules by destination.
fn sdk_configuration(rules: &[NotificationRule]) -> Result<NotificationConfiguration> {
    validate_notification_rules(rules)?;

    let mut configuration = NotificationConfiguration::builder();
    let mut lambdas = Vec::new();
    let mut queues = Vec::new();
    let mut topics = Vec::new();

    for rule in rules {
        match rule.destination {
            NotificationDestination::Lambda => {
                lambdas.push(
                    LambdaFunctionConfiguration::builder()
                        .set_id(rule.id.clone())
                        .lambda_function_arn(&rule.arn)
                        .set_events(Some(rule.sdk_events()))
                        .set_filter(rule.sdk_filter())
                        .build()
                        .context("Failed to build the Lambda notification rule")?,
                );
            }
            NotificationDestination::Queue => {
                queues.push(
                    QueueConfiguration::builder()
                        .set_id(rule.id.clone())
                        .queue_arn(&rule.arn)
                        .set_events(Some(rule.sdk_events()))
                        .set_filter(rule.sdk_filter())
                        .build()
                        .context("Failed to build the queue notification rule")?,
                );
            }
            NotificationDestination::Topic => {
                topics.push(
                    TopicConfiguration::builder()
                        .set_id(rule.id.clone())
                        .topic_arn(&rule.arn)
                        .set_events(Some(rule.sdk_events()))
                        .set_filter(rule.sdk_filter())
                        .build()
                        .context("Failed to build the topic notification rule")?,
                );
            }
        }
    }

    if !lambdas.is_empty() {
        configuration = configuration.set_lambda_function_configurations(Some(lambdas));
    }
    if !queues.is_empty() {
        configuration = configuration.set_queue_configurations(Some(queues));
    }
    if !topics.is_empty() {
        configuration = configuration.set_topic_configurations(Some(topics));
    }

    Ok(configuration.build())
}

/// Map the configurations of a response back into rules: the Lambda
/// configurations first, then the queues, then the topics.
fn rules_from_sdk(
    lambdas: &[LambdaFunctionConfiguration],
    queues: &[QueueConfiguration],
    topics: &[TopicConfiguration],
) -> Result<Vec<NotificationRule>> {
    let mut rules = Vec::new();
    for lambda in lambdas {
        rules.push(NotificationRule::from_sdk(
            NotificationDestination::Lambda,
            lambda.id(),
            lambda.lambda_function_arn(),
            lambda.events(),
            lambda.filter(),
        )?);
    }
    for queue in queues {
        rules.push(NotificationRule::from_sdk(
            NotificationDestination::Queue,
            queue.id(),
            queue.queue_arn(),
            queue.events(),
            queue.filter(),
        )?);
    }
    for topic in topics {
        rules.push(NotificationRule::from_sdk(
            NotificationDestination::Topic,
            topic.id(),
            topic.topic_arn(),
            topic.events(),
            topic.filter(),
        )?);
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEBHOOK: &str = "arn:rustfs:sqs::primary:webhook";

    /// The rules a response carries, in the order the getter maps them.
    fn rules_of(configuration: &NotificationConfiguration) -> Result<Vec<NotificationRule>> {
        rules_from_sdk(
            configuration.lambda_function_configurations(),
            configuration.queue_configurations(),
            configuration.topic_configurations(),
        )
    }

    fn rule(id: Option<&str>, events: &[&str]) -> NotificationRule {
        NotificationRule {
            destination: NotificationDestination::Queue,
            arn: WEBHOOK.to_string(),
            events: events.iter().map(|event| event.to_string()).collect(),
            prefix: None,
            suffix: None,
            id: id.map(|id| id.to_string()),
        }
    }

    #[test]
    fn test_notification_destination_serialization() {
        assert_eq!(
            serde_json::to_string(&NotificationDestination::Lambda).unwrap(),
            r#""lambda""#
        );
        assert_eq!(
            serde_json::to_string(&NotificationDestination::Queue).unwrap(),
            r#""queue""#
        );
        assert_eq!(
            serde_json::to_string(&NotificationDestination::Topic).unwrap(),
            r#""topic""#
        );

        let destination: NotificationDestination = serde_json::from_str(r#""topic""#).unwrap();
        assert_eq!(destination, NotificationDestination::Topic);
        assert!(serde_json::from_str::<NotificationDestination>(r#""sns""#).is_err());
    }

    #[test]
    fn test_notification_destination_text() {
        assert_eq!(NotificationDestination::Lambda.as_str(), "lambda");
        assert_eq!(NotificationDestination::Lambda.label(), "Lambda function");
        assert_eq!(NotificationDestination::Queue.label(), "SQS queue");
        assert_eq!(NotificationDestination::Topic.label(), "SNS topic");
    }

    #[test]
    fn test_notification_rule_serialization() {
        let original = NotificationRule {
            destination: NotificationDestination::Queue,
            arn: WEBHOOK.to_string(),
            events: vec!["s3:ObjectCreated:*".to_string()],
            prefix: Some("images/".to_string()),
            suffix: Some(".jpg".to_string()),
            id: Some("image-uploads".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            json,
            r#"{"destination":"queue","arn":"arn:rustfs:sqs::primary:webhook","events":["s3:ObjectCreated:*"],"prefix":"images/","suffix":".jpg","id":"image-uploads"}"#
        );
        let deserialized: NotificationRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);

        // The optional fields may be left out or sent as null.
        let json = r#"{
            "destination": "lambda",
            "arn": "arn:aws:lambda:us-east-1:111122223333:function:notify",
            "events": ["s3:ObjectCreated:Put", "s3:ObjectRemoved:*"],
            "prefix": null,
            "suffix": null,
            "id": null
        }"#;
        let deserialized: NotificationRule = serde_json::from_str(json).unwrap();
        assert_eq!(
            deserialized,
            NotificationRule {
                destination: NotificationDestination::Lambda,
                arn: "arn:aws:lambda:us-east-1:111122223333:function:notify".to_string(),
                events: vec![
                    "s3:ObjectCreated:Put".to_string(),
                    "s3:ObjectRemoved:*".to_string()
                ],
                prefix: None,
                suffix: None,
                id: None,
            }
        );
    }

    #[test]
    fn test_notification_rule_requires_destination_arn_and_events() {
        for json in [
            r#"{"arn": "arn:x", "events": ["s3:ObjectCreated:*"]}"#,
            r#"{"destination": "queue", "events": ["s3:ObjectCreated:*"]}"#,
            r#"{"destination": "queue", "arn": "arn:x"}"#,
            r#"{"destination": "sns", "arn": "arn:x", "events": ["s3:ObjectCreated:*"]}"#,
        ] {
            assert!(
                serde_json::from_str::<NotificationRule>(json).is_err(),
                "{json}"
            );
        }
    }

    #[test]
    fn test_notification_rule_label_and_summary() {
        let named = rule(Some("image-uploads"), &["s3:ObjectCreated:Put"]);
        assert_eq!(named.label(3), "rule 'image-uploads'");
        let bare = rule(None, &["s3:ObjectCreated:*"]);
        assert_eq!(bare.label(0), "rule 1");

        assert_eq!(
            bare.summary(),
            "SQS queue: arn:rustfs:sqs::primary:webhook for s3:ObjectCreated:*"
        );
        for (prefix, suffix, expected) in [
            (
                Some("images/"),
                Some(".jpg"),
                " (keys with prefix 'images/' and suffix '.jpg')",
            ),
            (Some("images/"), None, " (keys with prefix 'images/')"),
            (None, Some(".jpg"), " (keys with suffix '.jpg')"),
        ] {
            let mut filtered = rule(None, &["s3:ObjectCreated:Put"]);
            filtered.prefix = prefix.map(str::to_string);
            filtered.suffix = suffix.map(str::to_string);
            assert_eq!(
                filtered.summary(),
                format!("SQS queue: {WEBHOOK} for s3:ObjectCreated:Put{expected}")
            );
        }
    }

    #[test]
    fn test_is_arn_accepts_the_arn_shapes_of_aws_and_s3_compatible_servers() {
        assert!(is_arn("arn:rustfs:sqs::primary:webhook"));
        assert!(is_arn("arn:minio:sqs::1:webhook"));
        assert!(is_arn("arn:aws:sqs:us-east-1:111122223333:my-queue"));
        // A resource may itself contain colons, as in a Lambda alias.
        assert!(is_arn(
            "arn:aws:lambda:us-east-1:111122223333:function:notify:prod"
        ));
    }

    #[test]
    fn test_key_filter_encoding_round_trips_literal_values() {
        // The case the review names: a literal plus must not reach the wire
        // unchanged, where S3 would read it as a space.
        assert_eq!(encode_key_filter("reports+2026/"), "reports%2B2026/");
        assert_eq!(encode_key_filter("a b/"), "a+b/");
        assert_eq!(encode_key_filter("50%"), "50%25");
        assert_eq!(encode_key_filter("images/"), "images/");
        assert_eq!(encode_key_filter(""), "");

        for value in ["reports+2026/", "a b/", "50%", "images/", "", "文/déc.xml"] {
            assert_eq!(
                decode_key_filter(&encode_key_filter(value)).unwrap(),
                value,
                "{value}"
            );
        }

        // The wire forms decode back to the literals.
        assert_eq!(
            decode_key_filter("reports%2B2026/").unwrap(),
            "reports+2026/"
        );
        assert_eq!(decode_key_filter("a+b/").unwrap(), "a b/");
        assert_eq!(decode_key_filter("images/").unwrap(), "images/");
    }

    #[test]
    fn test_decode_key_filter_rejects_values_it_cannot_decode() {
        for wire in ["trailing%2", "%zz", "100%", "%FF"] {
            let error = decode_key_filter(wire).unwrap_err().to_string();
            assert!(error.contains("notification filter"), "{wire}: {error}");
        }
    }

    #[test]
    fn test_validate_notification_rules_accepts_valid_rules() {
        let valid = vec![
            rule(
                Some("created"),
                &["s3:ObjectCreated:*", "s3:ObjectCreated:Put"],
            ),
            rule(None, &["s3:ObjectRemoved:Delete"]),
            NotificationRule {
                destination: NotificationDestination::Topic,
                arn: "arn:aws:sns:us-east-1:111122223333:my-topic".to_string(),
                events: vec!["s3:Replication:OperationFailedReplication".to_string()],
                prefix: Some("logs/".to_string()),
                suffix: None,
                id: Some("x".repeat(MAX_NOTIFICATION_ID_LENGTH)),
            },
        ];
        assert!(validate_notification_rules(&valid).is_ok());
    }

    #[test]
    fn test_validate_notification_rules_rejects_empty_filters() {
        let mut invalid = rule(None, &["s3:ObjectCreated:*"]);
        invalid.prefix = Some(String::new());
        let error = validate_notification_rules(&[invalid])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 1 has an empty prefix"), "{error}");

        let mut invalid = rule(None, &["s3:ObjectCreated:*"]);
        invalid.suffix = Some(String::new());
        let error = validate_notification_rules(&[invalid])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 1 has an empty suffix"), "{error}");
    }

    #[test]
    fn test_validate_notification_rules_rejects_empty_list() {
        let error = validate_notification_rules(&[]).unwrap_err().to_string();
        assert!(
            error.contains("At least one notification rule is required"),
            "{error}"
        );
        assert!(error.contains("delete_bucket_notification"), "{error}");
    }

    #[test]
    fn test_validate_notification_rules_rejects_invalid_arns() {
        for arn in [
            "",
            "webhook",
            "arn",
            "arn:",
            "arn:aws",
            "arn::sqs::primary:webhook",
            "arn:rustfs:::webhook",
            "arn:rustfs:sqs::primary:",
            "https://example.com/hook",
        ] {
            let mut invalid = rule(None, &["s3:ObjectCreated:*"]);
            invalid.arn = arn.to_string();
            let error = validate_notification_rules(&[invalid])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(&format!("has an invalid ARN '{arn}'")),
                "{error}"
            );
        }
    }

    #[test]
    fn test_validate_notification_rules_rejects_unusable_events() {
        let error = validate_notification_rules(&[rule(None, &[])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 1 lists no events"), "{error}");

        for event in [
            "",
            "ObjectCreated:*",
            "s3-object-created",
            "s3:",
            "s3::Put",
            "s3:ObjectCreated:",
            "s3:ObjectCreated::Put",
            "s3: ",
            "s3:Created Put",
        ] {
            let error = validate_notification_rules(&[rule(None, &[event])])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(&format!("lists '{event}', which is not an S3 event name")),
                "{error}"
            );
        }

        // A valid event next to an invalid one does not hide it.
        let error = validate_notification_rules(&[rule(None, &["s3:ObjectCreated:*", "oops"])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("lists 'oops'"), "{error}");

        // Unknown but correctly shaped future events stay acceptable.
        assert!(
            validate_notification_rules(&[rule(None, &["s3:FutureEvent:Done"])]).is_ok(),
            "unknown names are a backend matter, the shape is what is checked"
        );
    }

    #[test]
    fn test_validate_notification_rules_rejects_long_and_duplicate_ids() {
        let long = rule(
            Some(&"i".repeat(MAX_NOTIFICATION_ID_LENGTH + 1)),
            &["s3:ObjectCreated:*"],
        );
        let error = validate_notification_rules(&[long])
            .unwrap_err()
            .to_string();
        assert!(error.contains("ID of 256 characters"), "{error}");

        // The limit is counted in characters, not bytes.
        // The limit is counted in characters, not bytes.
        let wide = "文".repeat(MAX_NOTIFICATION_ID_LENGTH);
        assert!(validate_notification_rules(&[rule(Some(&wide), &["s3:ObjectCreated:*"])]).is_ok());

        let error = validate_notification_rules(&[
            rule(Some("same"), &["s3:ObjectCreated:*"]),
            rule(Some("same"), &["s3:ObjectRemoved:*"]),
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("'same' is used twice"), "{error}");
    }

    #[test]
    fn test_sdk_configuration_groups_rules_by_destination() {
        let rules = vec![
            NotificationRule {
                destination: NotificationDestination::Queue,
                arn: WEBHOOK.to_string(),
                events: vec!["s3:ObjectCreated:Put".to_string()],
                prefix: Some("images/".to_string()),
                suffix: Some(".jpg".to_string()),
                id: Some("q1".to_string()),
            },
            NotificationRule {
                destination: NotificationDestination::Lambda,
                arn: "arn:aws:lambda:us-east-1:111122223333:function:notify".to_string(),
                events: vec!["s3:ObjectRemoved:*".to_string()],
                prefix: None,
                suffix: None,
                id: None,
            },
            NotificationRule {
                destination: NotificationDestination::Topic,
                arn: "arn:aws:sns:us-east-1:111122223333:my-topic".to_string(),
                events: vec!["s3:ObjectCreated:*".to_string()],
                prefix: None,
                suffix: None,
                id: Some("t1".to_string()),
            },
        ];

        // Reading the request body back with the getters' mapping must
        // return the rules exactly as they were given.
        let configuration = sdk_configuration(&rules).unwrap();
        assert_eq!(configuration.lambda_function_configurations().len(), 1);
        assert_eq!(configuration.queue_configurations().len(), 1);
        assert_eq!(configuration.topic_configurations().len(), 1);
        let mapped = rules_of(&configuration).unwrap();
        assert_eq!(mapped.len(), rules.len());
        for rule in &rules {
            assert!(mapped.contains(rule), "{rule:?}");
        }
        // The getter lists Lambda rules first, then queues, then topics.
        assert_eq!(mapped[0].destination, NotificationDestination::Lambda);
        assert_eq!(mapped[1].destination, NotificationDestination::Queue);
        assert_eq!(mapped[2].destination, NotificationDestination::Topic);
    }

    #[test]
    fn test_sdk_configuration_refuses_invalid_rules() {
        let mut invalid = rule(None, &["s3:ObjectCreated:*"]);
        invalid.events.clear();
        let error = sdk_configuration(&[invalid]).unwrap_err().to_string();
        assert!(error.to_string().contains("lists no events"), "{error}");
    }

    #[test]
    fn test_rules_from_sdk_keeps_event_names_it_does_not_model() {
        // An event name the SDK does not know arrives as its `Unknown`
        // variant; keep the raw name instead of failing.
        let configuration = NotificationConfiguration::builder()
            .queue_configurations(
                QueueConfiguration::builder()
                    .queue_arn(WEBHOOK)
                    .events(Event::from("s3:FutureEvent:Done"))
                    .build()
                    .unwrap(),
            )
            .build();

        let rules = rules_of(&configuration).unwrap();
        assert_eq!(rules[0].events, vec!["s3:FutureEvent:Done".to_string()]);
        assert_eq!(rules[0].destination, NotificationDestination::Queue);
    }

    #[test]
    fn test_rules_from_sdk_reads_filters_and_reports_unknown_rules() {
        let key_filter = |rules: Vec<FilterRule>| {
            NotificationConfigurationFilter::builder()
                .key(S3KeyFilter::builder().set_filter_rules(Some(rules)).build())
                .build()
        };
        let filtered = NotificationConfiguration::builder()
            .topic_configurations(
                TopicConfiguration::builder()
                    .id("t1")
                    .topic_arn(WEBHOOK)
                    .events(Event::S3ObjectCreated)
                    .filter(key_filter(vec![
                        FilterRule::builder()
                            .name(FilterRuleName::Suffix)
                            .value(".log")
                            .build(),
                        FilterRule::builder()
                            .name(FilterRuleName::Prefix)
                            .value("logs/")
                            .build(),
                    ]))
                    .build()
                    .unwrap(),
            )
            .build();
        assert_eq!(
            rules_of(&filtered).unwrap()[0],
            NotificationRule {
                destination: NotificationDestination::Topic,
                arn: WEBHOOK.to_string(),
                events: vec!["s3:ObjectCreated:*".to_string()],
                prefix: Some("logs/".to_string()),
                suffix: Some(".log".to_string()),
                id: Some("t1".to_string()),
            }
        );

        // A filter rule this crate does not model cannot be shown faithfully.
        let unmodelled = NotificationConfiguration::builder()
            .topic_configurations(
                TopicConfiguration::builder()
                    .topic_arn(WEBHOOK)
                    .events(Event::S3ObjectCreated)
                    .filter(key_filter(vec![
                        FilterRule::builder()
                            .name(FilterRuleName::from("middle"))
                            .value("x")
                            .build(),
                    ]))
                    .build()
                    .unwrap(),
            )
            .build();
        let error = rules_of(&unmodelled).unwrap_err().to_string();
        assert!(error.to_string().contains("does not model"), "{error}");
    }
}
