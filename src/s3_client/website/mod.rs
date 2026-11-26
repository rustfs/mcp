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

//! Static website configuration of buckets.

mod delete_bucket_website;
mod get_bucket_website;
mod put_bucket_website;

pub use get_bucket_website::BucketWebsiteConfig;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Protocol a website redirects to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum WebsiteProtocol {
    Http,
    Https,
}

impl WebsiteProtocol {
    /// Upper-case name used in requests and tool output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
        }
    }

    /// Map the SDK enum, dropping values this crate does not model.
    fn from_sdk(protocol: &aws_sdk_s3::types::Protocol) -> Option<Self> {
        match protocol {
            aws_sdk_s3::types::Protocol::Http => Some(Self::Http),
            aws_sdk_s3::types::Protocol::Https => Some(Self::Https),
            _ => None,
        }
    }

    fn to_sdk(self) -> aws_sdk_s3::types::Protocol {
        match self {
            Self::Http => aws_sdk_s3::types::Protocol::Http,
            Self::Https => aws_sdk_s3::types::Protocol::Https,
        }
    }
}

/// Instead of serving documents from the bucket, redirect every request
/// of the website to one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RedirectAllRequestsTo {
    #[schemars(description = "Host every request is redirected to, e.g. 'www.example.com'")]
    pub host_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional protocol of the redirect, 'HTTP' or 'HTTPS' (default: the protocol of the original request)"
    )]
    pub protocol: Option<WebsiteProtocol>,
}

/// Redirects requests whose key starts with a given prefix to another
/// prefix, keeping the rest of the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RoutingRule {
    #[schemars(description = "Key prefix the rule applies to, e.g. 'docs/'")]
    pub key_prefix_equals: String,
    #[schemars(description = "Prefix that replaces it in the redirected key, e.g. 'documents/'")]
    pub replace_key_prefix_with: String,
}

/// Static website configuration of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebsiteConfiguration {
    /// Suffix of the key served when a request ends with `/`, e.g.
    /// `index.html`. Required unless `redirect_all_requests_to` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Suffix of the key served when a request ends with '/', e.g. 'index.html'; required unless redirect_all_requests_to is set"
    )]
    pub index_document_suffix: Option<String>,
    /// Key of the document served for errors, e.g. `errors/404.html`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional key of the document served for errors, e.g. 'errors/404.html'"
    )]
    pub error_document_key: Option<String>,
    /// Redirects every request to one host instead of serving documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional host that every request is redirected to instead of serving documents; cannot be combined with the other settings"
    )]
    pub redirect_all_requests_to: Option<RedirectAllRequestsTo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(
        description = "Optional rules that redirect requests whose key starts with a given prefix"
    )]
    pub routing_rules: Vec<RoutingRule>,
}

impl WebsiteConfiguration {
    /// Copy the SDK values of a response; fails when a routing rule carries
    /// a condition or a redirect this crate does not model.
    fn from_sdk(
        index_document: Option<&aws_sdk_s3::types::IndexDocument>,
        error_document: Option<&aws_sdk_s3::types::ErrorDocument>,
        redirect_all_requests_to: Option<&aws_sdk_s3::types::RedirectAllRequestsTo>,
        sdk_routing_rules: &[aws_sdk_s3::types::RoutingRule],
    ) -> Result<Self> {
        let mut routing_rules = Vec::new();
        for rule in sdk_routing_rules {
            let condition = rule
                .condition()
                .context("The website configuration holds a routing rule without a condition")?;
            let redirect = rule
                .redirect()
                .context("The website configuration holds a routing rule without a redirect")?;
            // The other members of a condition or a redirect cannot be
            // shown faithfully here.
            let unsupported = condition.http_error_code_returned_equals().is_some()
                || redirect.host_name().is_some()
                || redirect.http_redirect_code().is_some()
                || redirect.protocol().is_some()
                || redirect.replace_key_with().is_some();
            if unsupported {
                anyhow::bail!(
                    "The website configuration holds a routing rule this crate does not model"
                );
            }
            routing_rules.push(RoutingRule {
                key_prefix_equals: condition
                    .key_prefix_equals()
                    .unwrap_or_default()
                    .to_string(),
                replace_key_prefix_with: redirect
                    .replace_key_prefix_with()
                    .unwrap_or_default()
                    .to_string(),
            });
        }

        Ok(Self {
            index_document_suffix: index_document
                .map(|document| document.suffix())
                .filter(|suffix| !suffix.is_empty())
                .map(|suffix| suffix.to_string()),
            error_document_key: error_document
                .map(|document| document.key())
                .filter(|key| !key.is_empty())
                .map(|key| key.to_string()),
            redirect_all_requests_to: match redirect_all_requests_to {
                Some(redirect) => {
                    let protocol = match redirect.protocol() {
                        None => None,
                        // A protocol the SDK does not model cannot be shown
                        // faithfully.
                        Some(protocol) => Some(
                            WebsiteProtocol::from_sdk(protocol).context(
                                "The website configuration redirects to a protocol this crate does not model",
                            )?,
                        ),
                    };
                    Some(RedirectAllRequestsTo {
                        host_name: redirect.host_name().to_string(),
                        protocol,
                    })
                }
                None => None,
            },
            routing_rules,
        })
    }

    /// Build the SDK value for a valid configuration.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::WebsiteConfiguration> {
        validate_website_configuration(self)?;

        let mut configuration = aws_sdk_s3::types::WebsiteConfiguration::builder();
        if let Some(suffix) = &self.index_document_suffix {
            configuration = configuration.index_document(
                aws_sdk_s3::types::IndexDocument::builder()
                    .suffix(suffix)
                    .build()
                    .context("Failed to build the index document")?,
            );
        }
        if let Some(key) = &self.error_document_key {
            configuration = configuration.error_document(
                aws_sdk_s3::types::ErrorDocument::builder()
                    .key(key)
                    .build()
                    .context("Failed to build the error document")?,
            );
        }
        if let Some(redirect) = &self.redirect_all_requests_to {
            let mut builder =
                aws_sdk_s3::types::RedirectAllRequestsTo::builder().host_name(&redirect.host_name);
            if let Some(protocol) = redirect.protocol {
                builder = builder.protocol(protocol.to_sdk());
            }
            configuration = configuration
                .redirect_all_requests_to(builder.build().context("Failed to build the redirect")?);
        }
        for rule in &self.routing_rules {
            configuration = configuration.routing_rules(
                aws_sdk_s3::types::RoutingRule::builder()
                    .condition(
                        aws_sdk_s3::types::Condition::builder()
                            .key_prefix_equals(&rule.key_prefix_equals)
                            .build(),
                    )
                    .redirect(
                        aws_sdk_s3::types::Redirect::builder()
                            .replace_key_prefix_with(&rule.replace_key_prefix_with)
                            .build(),
                    )
                    .build(),
            );
        }

        Ok(configuration.build())
    }

    /// The lines describing the configuration for tool output, without the
    /// routing rules, which are listed separately.
    pub fn summary(&self) -> String {
        let mut text = String::new();
        if let Some(suffix) = &self.index_document_suffix {
            text.push_str(&format!("**Index Document:** {suffix}\n"));
        }
        if let Some(key) = &self.error_document_key {
            text.push_str(&format!("**Error Document:** {key}\n"));
        }
        if let Some(redirect) = &self.redirect_all_requests_to {
            match redirect.protocol {
                Some(protocol) => text.push_str(&format!(
                    "**Redirect All Requests To:** {}://{}\n",
                    protocol.as_str().to_lowercase(),
                    redirect.host_name
                )),
                None => text.push_str(&format!(
                    "**Redirect All Requests To:** {}\n",
                    redirect.host_name
                )),
            }
        }
        text
    }
}

/// Reject a website configuration S3 cannot serve: nothing to serve or
/// redirect, a redirect mixed with documents, or an empty name.
pub fn validate_website_configuration(configuration: &WebsiteConfiguration) -> Result<()> {
    let has_documents = configuration.index_document_suffix.is_some()
        || configuration.error_document_key.is_some()
        || !configuration.routing_rules.is_empty();

    if let Some(redirect) = &configuration.redirect_all_requests_to {
        if redirect.host_name.trim().is_empty() {
            anyhow::bail!("The redirect host name must not be empty");
        }
        if has_documents {
            anyhow::bail!(
                "A website configuration that redirects all requests cannot also set an index document, an error document, or routing rules"
            );
        }
        return Ok(());
    }

    let Some(suffix) = &configuration.index_document_suffix else {
        anyhow::bail!(
            "A website configuration needs an index document (index_document_suffix) unless it redirects all requests"
        );
    };
    // S3 serves the suffix appended to a `/`; a path would not work.
    if suffix.is_empty() || suffix.contains('/') {
        anyhow::bail!(
            "The index document suffix '{}' must be a single name such as 'index.html', not a path",
            suffix
        );
    }
    if configuration.error_document_key.as_deref() == Some("") {
        anyhow::bail!("The error document key must not be empty; omit it to serve no error page");
    }
    for (index, rule) in configuration.routing_rules.iter().enumerate() {
        // An empty key_prefix_equals is a valid S3 match-all prefix, e.g.
        // prepending one prefix to every key; only the replacement may not
        // be empty.
        if rule.replace_key_prefix_with.is_empty() {
            anyhow::bail!(
                "Routing rule {} needs replace_key_prefix_with (key_prefix_equals may be empty to match every key)",
                index + 1
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Map an SDK value the way the getter maps a response.
    fn mapped(value: &aws_sdk_s3::types::WebsiteConfiguration) -> Result<WebsiteConfiguration> {
        WebsiteConfiguration::from_sdk(
            value.index_document(),
            value.error_document(),
            value.redirect_all_requests_to(),
            value.routing_rules(),
        )
    }

    fn documents() -> WebsiteConfiguration {
        WebsiteConfiguration {
            index_document_suffix: Some("index.html".to_string()),
            error_document_key: Some("errors/404.html".to_string()),
            redirect_all_requests_to: None,
            routing_rules: vec![RoutingRule {
                key_prefix_equals: "docs/".to_string(),
                replace_key_prefix_with: "documents/".to_string(),
            }],
        }
    }

    fn redirect() -> WebsiteConfiguration {
        WebsiteConfiguration {
            index_document_suffix: None,
            error_document_key: None,
            redirect_all_requests_to: Some(RedirectAllRequestsTo {
                host_name: "www.example.com".to_string(),
                protocol: Some(WebsiteProtocol::Https),
            }),
            routing_rules: Vec::new(),
        }
    }

    #[test]
    fn test_website_protocol_serialization() {
        assert_eq!(
            serde_json::to_string(&WebsiteProtocol::Https).unwrap(),
            r#""HTTPS""#
        );
        let protocol: WebsiteProtocol = serde_json::from_str(r#""HTTP""#).unwrap();
        assert_eq!(protocol, WebsiteProtocol::Http);
        assert_eq!(WebsiteProtocol::Https.as_str(), "HTTPS");
        // S3 compares the name exactly.
        assert!(serde_json::from_str::<WebsiteProtocol>(r#""https""#).is_err());
    }

    #[test]
    fn test_website_protocol_sdk_round_trip() {
        for protocol in [WebsiteProtocol::Http, WebsiteProtocol::Https] {
            assert_eq!(
                WebsiteProtocol::from_sdk(&protocol.to_sdk()),
                Some(protocol)
            );
        }
        let unknown = aws_sdk_s3::types::Protocol::from("future-protocol");
        assert_eq!(WebsiteProtocol::from_sdk(&unknown), None);
    }

    #[test]
    fn test_website_configuration_serialization() {
        let original = documents();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            json,
            r#"{"index_document_suffix":"index.html","error_document_key":"errors/404.html","routing_rules":[{"key_prefix_equals":"docs/","replace_key_prefix_with":"documents/"}]}"#
        );
        let deserialized: WebsiteConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);

        // Fields left out and fields sent as null mean the same.
        let json = r#"{
            "index_document_suffix": "index.html",
            "error_document_key": null,
            "redirect_all_requests_to": null,
            "routing_rules": []
        }"#;
        let deserialized: WebsiteConfiguration = serde_json::from_str(json).unwrap();
        assert_eq!(
            deserialized,
            WebsiteConfiguration {
                index_document_suffix: Some("index.html".to_string()),
                error_document_key: None,
                redirect_all_requests_to: None,
                routing_rules: Vec::new()
            }
        );

        let json = serde_json::to_string(&redirect()).unwrap();
        assert_eq!(
            json,
            r#"{"redirect_all_requests_to":{"host_name":"www.example.com","protocol":"HTTPS"}}"#
        );
    }

    #[test]
    fn test_website_configuration_sdk_round_trip() {
        for expected in [documents(), redirect()] {
            assert_eq!(mapped(&expected.to_sdk().unwrap()).unwrap(), expected);
        }

        // A redirect without a protocol keeps its unset protocol.
        let bare = WebsiteConfiguration {
            redirect_all_requests_to: Some(RedirectAllRequestsTo {
                host_name: "example.com".to_string(),
                protocol: None,
            }),
            ..redirect()
        };
        assert_eq!(mapped(&bare.to_sdk().unwrap()).unwrap(), bare);
    }

    #[test]
    fn test_website_configuration_from_sdk_reports_unmodelled_values() {
        // A routing rule with any other condition or redirect member cannot
        // be shown faithfully.
        let rich = aws_sdk_s3::types::WebsiteConfiguration::builder()
            .index_document(
                aws_sdk_s3::types::IndexDocument::builder()
                    .suffix("index.html")
                    .build()
                    .unwrap(),
            )
            .routing_rules(
                aws_sdk_s3::types::RoutingRule::builder()
                    .condition(
                        aws_sdk_s3::types::Condition::builder()
                            .key_prefix_equals("docs/")
                            .http_error_code_returned_equals("404")
                            .build(),
                    )
                    .redirect(
                        aws_sdk_s3::types::Redirect::builder()
                            .replace_key_prefix_with("documents/")
                            .build(),
                    )
                    .build(),
            )
            .build();
        let error = mapped(&rich).unwrap_err();
        assert!(error.to_string().contains("does not model"), "{error}");

        // A protocol the SDK does not model is reported as such instead of
        // being dropped silently.
        let exotic = aws_sdk_s3::types::WebsiteConfiguration::builder()
            .redirect_all_requests_to(
                aws_sdk_s3::types::RedirectAllRequestsTo::builder()
                    .host_name("example.com")
                    .protocol(aws_sdk_s3::types::Protocol::from("future-protocol"))
                    .build()
                    .unwrap(),
            )
            .build();
        assert!(mapped(&exotic).is_err());
    }

    #[test]
    fn test_validate_website_configuration_accepts_valid_configurations() {
        assert!(validate_website_configuration(&documents()).is_ok());
        assert!(validate_website_configuration(&redirect()).is_ok());
        assert!(
            validate_website_configuration(&WebsiteConfiguration {
                index_document_suffix: Some("index.html".to_string()),
                error_document_key: None,
                redirect_all_requests_to: None,
                routing_rules: Vec::new()
            })
            .is_ok()
        );
    }

    #[test]
    fn test_validate_website_configuration_rejects_unserving_configurations() {
        let empty = WebsiteConfiguration {
            index_document_suffix: None,
            error_document_key: Some("errors/404.html".to_string()),
            redirect_all_requests_to: None,
            routing_rules: Vec::new(),
        };
        let error = validate_website_configuration(&empty).unwrap_err();
        assert!(
            error.to_string().contains("needs an index document"),
            "{error}"
        );

        for suffix in ["", "pages/index.html", "/index.html"] {
            let invalid = WebsiteConfiguration {
                index_document_suffix: Some(suffix.to_string()),
                ..empty.clone()
            };
            let error = validate_website_configuration(&invalid).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("suffix '{suffix}' must be a single name")),
                "{error}"
            );
        }

        let invalid = WebsiteConfiguration {
            error_document_key: Some(String::new()),
            ..documents()
        };
        let error = validate_website_configuration(&invalid).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("error document key must not be empty")
        );

        let invalid = WebsiteConfiguration {
            routing_rules: vec![RoutingRule {
                key_prefix_equals: "docs/".to_string(),
                replace_key_prefix_with: String::new(),
            }],
            ..documents()
        };
        let error = validate_website_configuration(&invalid).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Routing rule 1 needs replace_key_prefix_with"),
            "{error}"
        );

        // An empty matching prefix matches every key and is valid.
        let match_all = WebsiteConfiguration {
            routing_rules: vec![RoutingRule {
                key_prefix_equals: String::new(),
                replace_key_prefix_with: "archive/".to_string(),
            }],
            ..documents()
        };
        assert!(validate_website_configuration(&match_all).is_ok());
        assert_eq!(
            WebsiteConfiguration::from_sdk(
                Some(
                    &aws_sdk_s3::types::IndexDocument::builder()
                        .suffix("index.html")
                        .build()
                        .unwrap()
                ),
                None,
                None,
                &[aws_sdk_s3::types::RoutingRule::builder()
                    .condition(
                        aws_sdk_s3::types::Condition::builder()
                            .key_prefix_equals("")
                            .build(),
                    )
                    .redirect(
                        aws_sdk_s3::types::Redirect::builder()
                            .replace_key_prefix_with("archive/")
                            .build(),
                    )
                    .build()],
            )
            .unwrap()
            .routing_rules,
            vec![RoutingRule {
                key_prefix_equals: String::new(),
                replace_key_prefix_with: "archive/".to_string(),
            }],
            "the getter must return a rule S3 accepted, so it can be put back"
        );
    }

    #[test]
    fn test_validate_website_configuration_keeps_the_redirect_alone() {
        let mut mixed = redirect();
        mixed.error_document_key = Some("errors/404.html".to_string());
        let error = validate_website_configuration(&mixed).unwrap_err();
        assert!(error.to_string().contains("cannot also set"), "{error}");

        let mut nameless = redirect();
        nameless.redirect_all_requests_to = Some(RedirectAllRequestsTo {
            host_name: "  ".to_string(),
            protocol: None,
        });
        let error = validate_website_configuration(&nameless).unwrap_err();
        assert!(error.to_string().contains("host name must not be empty"));
    }

    #[test]
    fn test_website_configuration_summary() {
        let summary = documents().summary();
        assert_eq!(
            summary,
            "**Index Document:** index.html\n**Error Document:** errors/404.html\n"
        );
        assert_eq!(
            redirect().summary(),
            "**Redirect All Requests To:** https://www.example.com\n"
        );
    }
}
