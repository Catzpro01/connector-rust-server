//! Parameter schema — the single source of truth for validation AND UI.
//!
//! # Why this exists (audit A-30)
//!
//! The old roadmap treated the visual editor as "PHASE 12 — build a canvas".
//! That framing hides the actually expensive part: n8n renders each node's
//! configuration form **from a declarative schema** (`INodeProperties` +
//! `displayOptions`), not from hand-written UI per node.
//!
//! If we do not have that schema, then every node costs manual UI work and
//! "full connection parity" becomes impossible — 400 integrations x bespoke
//! forms is not a project, it is a graveyard.
//!
//! With it, the Vue editor is a **generic form renderer** and the OpenAPI
//! codegen (KERNEL-SPEC §10) can emit both the connector *and* its form at once.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Declarative description of a node's configurable parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterSchema {
    pub fields: Vec<ParameterField>,
}

impl ParameterSchema {
    pub fn empty() -> Self {
        Self { fields: Vec::new() }
    }

    pub fn field(mut self, f: ParameterField) -> Self {
        self.fields.push(f);
        self
    }

    pub fn lookup(&self, name: &str) -> Option<&ParameterField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Validate a raw parameter object against this schema.
    ///
    /// Returns every problem found rather than failing fast, so the editor can
    /// mark all invalid fields at once.
    pub fn validate(&self, params: &Value) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        let obj = match params.as_object() {
            Some(o) => o,
            None => {
                errs.push(ValidationError {
                    field: "<root>".into(),
                    message: "parameters must be a JSON object".into(),
                });
                return errs;
            }
        };

        for f in &self.fields {
            let visible = f.is_visible(params);
            let present = obj.contains_key(&f.name);

            // Hidden fields are not required — this mirrors n8n's displayOptions.
            if f.required && visible && !present {
                errs.push(ValidationError {
                    field: f.name.clone(),
                    message: "required parameter is missing".into(),
                });
                continue;
            }
            if !visible {
                continue;
            }
            if let Some(v) = obj.get(&f.name) {
                if !f.kind.accepts(v) {
                    errs.push(ValidationError {
                        field: f.name.clone(),
                        message: format!("expected {}, got {}", f.kind.type_name(), json_type(v)),
                    });
                }
            }
        }
        errs
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterField {
    pub name: String,

    #[serde(rename = "displayName")]
    pub display_name: String,

    #[serde(rename = "type")]
    pub kind: ParameterKind,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    #[serde(default)]
    pub required: bool,

    /// n8n `displayOptions`: conditional show/hide based on other fields.
    ///
    /// Example: the `body` field only appears when `method == "POST"`.
    #[serde(rename = "displayOptions", default, skip_serializing_if = "Option::is_none")]
    pub display_if: Option<DisplayCondition>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<ParameterOption>>,

    /// May this field contain `{{ expressions }}`?
    #[serde(default)]
    pub supports_expression: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,

    /// Nested fields for `FixedCollection`-style grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ParameterField>,
}

impl ParameterField {
    pub fn new(name: impl Into<String>, display_name: impl Into<String>, kind: ParameterKind) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            kind,
            default: None,
            required: false,
            display_if: None,
            options: None,
            supports_expression: false,
            description: None,
            placeholder: None,
            children: Vec::new(),
        }
    }

    pub fn with_default(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn expression(mut self) -> Self {
        self.supports_expression = true;
        self
    }

    pub fn shown_when(mut self, cond: DisplayCondition) -> Self {
        self.display_if = Some(cond);
        self
    }

    /// Is this field currently visible given the whole parameter object?
    pub fn is_visible(&self, params: &Value) -> bool {
        match &self.display_if {
            None => true,
            Some(c) => c.evaluate(params),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterOption {
    pub name: String,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Field types the generic form renderer must know how to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterKind {
    String,
    Text,
    Number,
    Boolean,
    /// Single choice from `options`
    Options,
    /// Arbitrary JSON, edited in a code pane
    Json,
    /// JavaScript body (the Code node)
    Code,
    DateTime,
    /// URL / file / etc. selector
    ResourceLocator,
    /// n8n `fixedCollection`: repeatable group of sub-fields
    FixedCollection,
    /// Nested object rendered as a collapsible section
    Collection,
    /// Credential picker — rendered from `CredentialSpec`, never a raw input
    Credentials,
}

impl ParameterKind {
    pub fn type_name(self) -> &'static str {
        match self {
            ParameterKind::String | ParameterKind::Text => "string",
            ParameterKind::Number => "number",
            ParameterKind::Boolean => "boolean",
            ParameterKind::Options => "one of the allowed options",
            ParameterKind::Json | ParameterKind::Collection | ParameterKind::FixedCollection => "object",
            ParameterKind::Code => "string (code)",
            ParameterKind::DateTime => "ISO-8601 datetime string",
            ParameterKind::ResourceLocator => "string or resource-locator object",
            ParameterKind::Credentials => "credential reference",
        }
    }

    /// Type check a raw value. Expressions are permitted where the field
    /// declares `supports_expression`; the caller strips them first.
    pub fn accepts(self, v: &Value) -> bool {
        match self {
            ParameterKind::String | ParameterKind::Text | ParameterKind::Code => v.is_string(),
            ParameterKind::Number => v.is_number(),
            ParameterKind::Boolean => v.is_boolean(),
            ParameterKind::Json | ParameterKind::Collection | ParameterKind::FixedCollection => {
                v.is_object() || v.is_array()
            }
            // Deliberately permissive: validated semantically downstream.
            ParameterKind::Options => v.is_string() || v.is_number() || v.is_boolean(),
            ParameterKind::DateTime => v.is_string() || v.is_number(),
            ParameterKind::ResourceLocator => v.is_string() || v.is_object(),
            ParameterKind::Credentials => v.is_object() || v.is_string(),
        }
    }
}

/// Conditional visibility, mirroring n8n `displayOptions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DisplayCondition {
    /// Show only when `field` equals `value`.
    Show { field: String, value: Value },
    /// Hide when `field` equals `value`.
    Hide { field: String, value: Value },
    /// Logical composition.
    All(Vec<DisplayCondition>),
    Any(Vec<DisplayCondition>),
    /// Field must be present (and non-null) / absent.
    Exists { field: String, exists: bool },
}

impl DisplayCondition {
    pub fn show(field: impl Into<String>, value: Value) -> Self {
        DisplayCondition::Show {
            field: field.into(),
            value,
        }
    }

    pub fn evaluate(&self, params: &Value) -> bool {
        match self {
            DisplayCondition::Show { field, value } => {
                params.get(field).map(|v| v == value).unwrap_or(false)
            }
            DisplayCondition::Hide { field, value } => {
                !params.get(field).map(|v| v == value).unwrap_or(false)
            }
            DisplayCondition::All(cs) => cs.iter().all(|c| c.evaluate(params)),
            DisplayCondition::Any(cs) => cs.iter().any(|c| c.evaluate(params)),
            DisplayCondition::Exists { field, exists } => {
                let present = params.get(field).map(|v| !v.is_null()).unwrap_or(false);
                present == *exists
            }
        }
    }
}

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A credential type a node may request.
///
/// Nodes declare these statically; `NodeContext::credentials` then refuses any
/// lookup not listed here (D92 — least privilege).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialSpec {
    /// e.g. `httpHeaderAuth`, `slackApi`, `postgres`
    pub kind: String,
    pub required: bool,
    /// What the credential is used for — surfaced in the UI before the user
    /// grants it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}
