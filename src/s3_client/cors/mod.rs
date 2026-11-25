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

//! Bucket CORS (cross-origin resource sharing) configuration.

mod delete_bucket_cors;
mod get_bucket_cors;
mod put_bucket_cors;

pub use get_bucket_cors::BucketCorsConfig;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// HTTP methods S3 accepts in a CORS rule; they are case-sensitive.
const ALLOWED_METHODS: [&str; 5] = ["GET", "PUT", "POST", "DELETE", "HEAD"];

/// Largest `MaxAgeSeconds` S3 accepts: one day.
const MAX_MAX_AGE_SECONDS: i32 = 86400;

/// One cross-origin resource sharing rule of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CorsRule {
    /// Optional unique identifier of the rule (S3 allows up to 255 characters).
    #[serde(default)]
    #[schemars(
        description = "Optional unique identifier of the rule, e.g. 'app-uploads' (S3 allows up to 255 characters)"
    )]
    pub id: Option<String>,
    /// HTTP methods the rule allows, e.g. `["GET", "PUT", "POST"]`.
    #[schemars(
        description = "HTTP methods the rule allows; each one must be exactly GET, PUT, POST, DELETE, or HEAD (case-sensitive), e.g. [\"GET\", \"PUT\", \"POST\"]"
    )]
    pub allowed_methods: Vec<String>,
    /// Origins allowed to call the bucket, e.g. `["https://app.example.com"]` or `["*"]`.
    #[schemars(
        description = "Origins allowed to call the bucket, e.g. [\"https://app.example.com\"] or [\"*\"] for any origin"
    )]
    pub allowed_origins: Vec<String>,
    /// Request headers allowed in preflight requests, e.g. `["Content-Type", "X-Amz-Date"]` or `["*"]`.
    #[serde(default)]
    #[schemars(
        description = "Optional request headers allowed in preflight requests, e.g. [\"Content-Type\", \"X-Amz-Date\"] or [\"*\"] (defaults to none)"
    )]
    pub allowed_headers: Vec<String>,
    /// Response headers browsers may read, e.g. `["ETag"]`.
    #[serde(default)]
    #[schemars(
        description = "Optional response headers browsers may read, e.g. [\"ETag\"] (defaults to none)"
    )]
    pub expose_headers: Vec<String>,
    /// Seconds browsers may cache preflight responses (0-86400).
    #[serde(default)]
    #[schemars(
        description = "Optional number of seconds browsers may cache preflight responses, 0-86400 (e.g. 3000)"
    )]
    pub max_age_seconds: Option<i32>,
}

impl CorsRule {
    /// Map the SDK type, reading every field of the rule.
    fn from_sdk(rule: &aws_sdk_s3::types::CorsRule) -> Self {
        Self {
            id: rule.id().map(|id| id.to_string()),
            allowed_methods: rule.allowed_methods().to_vec(),
            allowed_origins: rule.allowed_origins().to_vec(),
            allowed_headers: rule.allowed_headers().to_vec(),
            expose_headers: rule.expose_headers().to_vec(),
            max_age_seconds: rule.max_age_seconds(),
        }
    }

    /// Build the SDK request type for this rule.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::CorsRule> {
        aws_sdk_s3::types::CorsRule::builder()
            .set_id(self.id.clone())
            .set_allowed_methods(Some(self.allowed_methods.clone()))
            .set_allowed_origins(Some(self.allowed_origins.clone()))
            .set_allowed_headers(Some(self.allowed_headers.clone()))
            .set_expose_headers(Some(self.expose_headers.clone()))
            .set_max_age_seconds(self.max_age_seconds)
            .build()
            .context("Failed to build the CORS rule")
    }

    /// Name used in tool output and error messages: the ID when the rule has
    /// one, otherwise its 1-based position in the list it was submitted in.
    pub fn label(&self, index: usize) -> String {
        match &self.id {
            Some(id) => format!("rule '{id}'"),
            None => format!("rule {}", index + 1),
        }
    }
}

/// Check a rule list before it is sent to S3, naming the first rule that
/// breaks a requirement.
pub fn validate_cors_rules(rules: &[CorsRule]) -> Result<()> {
    if rules.is_empty() {
        anyhow::bail!(
            "At least one CORS rule is required; to remove the configuration \
             call delete_bucket_cors instead"
        );
    }

    for (index, rule) in rules.iter().enumerate() {
        if rule.allowed_methods.is_empty() {
            anyhow::bail!(
                "CORS {} has no allowed methods; add at least one of GET, PUT, POST, DELETE, HEAD",
                rule.label(index)
            );
        }
        if let Some(method) = rule
            .allowed_methods
            .iter()
            .find(|method| !ALLOWED_METHODS.contains(&method.as_str()))
        {
            anyhow::bail!(
                "CORS {} has an unsupported allowed method '{}'; use only GET, PUT, POST, DELETE, HEAD (case-sensitive)",
                rule.label(index),
                method
            );
        }
        if rule.allowed_origins.is_empty() {
            anyhow::bail!(
                "CORS {} has no allowed origins; add at least one origin such as 'https://app.example.com' or '*'",
                rule.label(index)
            );
        }
        if let Some(max_age_seconds) = rule.max_age_seconds
            && !(0..=MAX_MAX_AGE_SECONDS).contains(&max_age_seconds)
        {
            anyhow::bail!(
                "CORS {} has max_age_seconds {}, which is outside the allowed range of 0 to {MAX_MAX_AGE_SECONDS} seconds",
                rule.label(index),
                max_age_seconds
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_rule() -> CorsRule {
        CorsRule {
            id: Some("app-uploads".to_string()),
            allowed_methods: vec!["GET".to_string(), "PUT".to_string(), "POST".to_string()],
            allowed_origins: vec!["https://app.example.com".to_string()],
            allowed_headers: vec!["Content-Type".to_string(), "X-Amz-Date".to_string()],
            expose_headers: vec!["ETag".to_string()],
            max_age_seconds: Some(3000),
        }
    }

    fn minimal_rule() -> CorsRule {
        rule(&["GET"], &["*"])
    }

    fn rule(methods: &[&str], origins: &[&str]) -> CorsRule {
        CorsRule {
            id: None,
            allowed_methods: methods.iter().map(|method| method.to_string()).collect(),
            allowed_origins: origins.iter().map(|origin| origin.to_string()).collect(),
            allowed_headers: Vec::new(),
            expose_headers: Vec::new(),
            max_age_seconds: None,
        }
    }

    #[test]
    fn test_cors_rule_serialization_round_trip() {
        let json = serde_json::to_string(&full_rule()).unwrap();
        let deserialized: CorsRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, full_rule());

        let json = serde_json::to_string(&minimal_rule()).unwrap();
        let deserialized: CorsRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, minimal_rule());
    }

    #[test]
    fn test_cors_rule_optional_fields_default() {
        let deserialized: CorsRule =
            serde_json::from_str(r#"{"allowed_methods": ["GET"], "allowed_origins": ["*"]}"#)
                .unwrap();
        assert_eq!(deserialized.id, None);
        assert_eq!(deserialized.allowed_methods, vec!["GET".to_string()]);
        assert_eq!(deserialized.allowed_origins, vec!["*".to_string()]);
        assert!(deserialized.allowed_headers.is_empty());
        assert!(deserialized.expose_headers.is_empty());
        assert_eq!(deserialized.max_age_seconds, None);

        // Explicit nulls mean the same as leaving the optional fields out.
        let deserialized: CorsRule = serde_json::from_str(
            r#"{
            "id": null,
            "allowed_methods": ["GET"],
            "allowed_origins": ["*"],
            "allowed_headers": [],
            "expose_headers": [],
            "max_age_seconds": null
        }"#,
        )
        .unwrap();
        assert_eq!(deserialized, minimal_rule());
    }

    #[test]
    fn test_cors_rule_requires_methods_and_origins() {
        assert!(serde_json::from_str::<CorsRule>(r#"{"allowed_origins": ["*"]}"#).is_err());
        assert!(serde_json::from_str::<CorsRule>(r#"{"allowed_methods": ["GET"]}"#).is_err());
    }

    #[test]
    fn test_cors_rule_from_sdk_reads_every_field() {
        let sdk = aws_sdk_s3::types::CorsRule::builder()
            .id("public-read")
            .allowed_methods("GET")
            .allowed_methods("HEAD")
            .allowed_origins("https://app.example.com")
            .allowed_origins("https://www.example.com")
            .allowed_headers("Content-Type")
            .expose_headers("ETag")
            .max_age_seconds(600)
            .build()
            .unwrap();

        assert_eq!(
            CorsRule::from_sdk(&sdk),
            CorsRule {
                id: Some("public-read".to_string()),
                allowed_methods: vec!["GET".to_string(), "HEAD".to_string()],
                allowed_origins: vec![
                    "https://app.example.com".to_string(),
                    "https://www.example.com".to_string()
                ],
                allowed_headers: vec!["Content-Type".to_string()],
                expose_headers: vec!["ETag".to_string()],
                max_age_seconds: Some(600),
            }
        );
    }

    #[test]
    fn test_cors_rule_sdk_round_trip() {
        // The getter maps the SDK type and the setter maps back; both
        // directions must keep every field.
        for expected in [full_rule(), minimal_rule()] {
            let sdk = expected.to_sdk().unwrap();
            assert_eq!(CorsRule::from_sdk(&sdk), expected);
        }

        let sdk = aws_sdk_s3::types::CorsRule::builder()
            .allowed_methods("GET")
            .allowed_origins("*")
            .build()
            .unwrap();
        assert_eq!(CorsRule::from_sdk(&sdk), minimal_rule());
    }

    #[test]
    fn test_cors_rule_label() {
        // Rules are named by ID, falling back to the 1-based position.
        assert_eq!(full_rule().label(3), "rule 'app-uploads'");
        assert_eq!(minimal_rule().label(0), "rule 1");
        assert_eq!(minimal_rule().label(4), "rule 5");
    }

    #[test]
    fn test_validate_cors_rules_accepts_valid_rules() {
        assert!(validate_cors_rules(&[minimal_rule()]).is_ok());
        assert!(validate_cors_rules(&[full_rule(), minimal_rule()]).is_ok());

        // Every supported method, and both ends of the max age range.
        let mut rule = rule(&["GET", "PUT", "POST", "DELETE", "HEAD"], &["*"]);
        rule.max_age_seconds = Some(0);
        assert!(validate_cors_rules(&[rule.clone()]).is_ok());

        rule.max_age_seconds = Some(MAX_MAX_AGE_SECONDS);
        assert!(validate_cors_rules(&[rule]).is_ok());
    }

    #[test]
    fn test_validate_cors_rules_rejects_empty_list() {
        let error = validate_cors_rules(&[]).unwrap_err().to_string();
        assert!(
            error.contains("At least one CORS rule is required"),
            "{error}"
        );
        assert!(error.contains("delete_bucket_cors"), "{error}");
    }

    #[test]
    fn test_validate_cors_rules_rejects_missing_methods() {
        let error = validate_cors_rules(&[rule(&[], &["*"])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 1 has no allowed methods"), "{error}");
    }

    #[test]
    fn test_validate_cors_rules_rejects_unsupported_methods() {
        // S3 compares methods exactly, so lower-case and wildcards fail too.
        for method in ["PATCH", "get", "Get", "*", ""] {
            let error = validate_cors_rules(&[rule(&[method], &["*"])])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(&format!("has an unsupported allowed method '{method}'")),
                "{error}"
            );
        }

        // The unsupported method is reported even after valid ones.
        let error = validate_cors_rules(&[rule(&["GET", "PATCH", "PUT"], &["*"])])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unsupported allowed method 'PATCH'"),
            "{error}"
        );
    }

    #[test]
    fn test_validate_cors_rules_rejects_missing_origins() {
        let error = validate_cors_rules(&[rule(&["GET"], &[])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 1 has no allowed origins"), "{error}");
    }

    #[test]
    fn test_validate_cors_rules_rejects_out_of_range_max_age() {
        for max_age_seconds in [-1, MAX_MAX_AGE_SECONDS + 1] {
            let mut rule = minimal_rule();
            rule.max_age_seconds = Some(max_age_seconds);
            let error = validate_cors_rules(&[rule]).unwrap_err().to_string();
            assert!(
                error.contains(&format!(
                    "has max_age_seconds {max_age_seconds}, which is outside the allowed range"
                )),
                "{error}"
            );
        }
    }

    #[test]
    fn test_validate_cors_rules_names_the_offending_rule() {
        // By ID when the rule has one.
        let mut invalid = minimal_rule();
        invalid.id = Some("public-read".to_string());
        invalid.max_age_seconds = Some(99999);
        let error = validate_cors_rules(&[full_rule(), invalid])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 'public-read'"), "{error}");

        // Otherwise by its 1-based position in the submitted list.
        let error = validate_cors_rules(&[full_rule(), rule(&["PATCH"], &["*"])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("rule 2 has"), "{error}");
        assert!(!error.contains("rule 1"), "{error}");
    }
}
