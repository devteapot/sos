use std::collections::{BTreeMap, BTreeSet};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PACKAGE_FORMAT_VERSION: u32 = 4;
pub const CONTRACT_VERSION: u32 = 1;
pub const APPEARANCE_ABI_VERSION: u32 = 1;
pub const GRAPH_FORMAT_VERSION: u32 = 1;

pub const MAX_EXPERIENCE_ID_BYTES: usize = 128;
pub const MAX_NAME_BYTES: usize = 64;
pub const MAX_EXPORTS: usize = 16;
pub const MAX_DEPENDENCIES: usize = 16;
pub const MAX_DERIVATION_PARENTS: usize = 8;
pub const MAX_SCHEMA_DEPTH: usize = 8;
pub const MAX_SCHEMA_FIELDS: usize = 64;
pub const MAX_SCHEMA_LIST_ITEMS: usize = 256;
pub const MAX_BOUNDARY_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_PACKAGE_METADATA_BYTES: usize = 256 * 1024;
pub const MAX_RESOLVED_GRAPH_BYTES: usize = 256 * 1024;
pub const MAX_GRAPH_DEPTH: usize = 4;
pub const MAX_GRAPH_INSTANCES: usize = 8;
pub const MAX_GRAPH_SCENE_NODES: usize = 8_192;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{kind} `{value}` is not a valid identifier")]
    InvalidIdentifier { kind: &'static str, value: String },
    #[error("revision ID `{0}` is not a lowercase SHA-256 digest")]
    InvalidRevisionId(String),
    #[error("contract digest `{0}` is not a lowercase SHA-256 digest")]
    InvalidContractDigest(String),
    #[error("unsupported {kind} version {actual}, expected {expected}")]
    UnsupportedVersion {
        kind: &'static str,
        actual: u32,
        expected: u32,
    },
    #[error("{kind} count {actual} exceeds limit {limit}")]
    CountLimit {
        kind: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("schema at `{path}` exceeds depth {MAX_SCHEMA_DEPTH}")]
    SchemaDepth { path: String },
    #[error("schema at `{path}` is invalid: {reason}")]
    InvalidSchema { path: String, reason: String },
    #[error("value at `{path}` does not match its schema: {reason}")]
    ValueMismatch { path: String, reason: String },
    #[error("serialized boundary value has {actual} bytes, limit is {MAX_BOUNDARY_VALUE_BYTES}")]
    BoundaryValueTooLarge { actual: usize },
    #[error("contract is invalid: {0}")]
    InvalidContract(String),
    #[error("dependency `{0}` is invalid: {1}")]
    InvalidDependency(String, String),
    #[error("derivation record is invalid: {0}")]
    InvalidDerivation(String),
    #[error("appearance profile is invalid: {0}")]
    InvalidAppearance(String),
    #[error("graph is invalid: {0}")]
    InvalidGraph(String),
    #[error("canonical JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JCS serialization failed: {0}")]
    CanonicalJson(String),
    #[error("{kind} JSON is not canonical")]
    NonCanonicalJson { kind: &'static str },
    #[error("{kind} JSON has {actual} bytes, limit is {limit}")]
    WirePayloadTooLarge {
        kind: &'static str,
        actual: usize,
        limit: usize,
    },
}

macro_rules! string_id {
    ($name:ident, $kind:literal, $max:expr) => {
        #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, Error> {
                let value = value.into();
                if valid_name(&value, $max) {
                    Ok(Self(value))
                } else {
                    Err(Error::InvalidIdentifier { kind: $kind, value })
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

string_id!(ExperienceId, "experience ID", MAX_EXPERIENCE_ID_BYTES);
string_id!(ExportId, "export ID", MAX_NAME_BYTES);
string_id!(DependencyAlias, "dependency alias", MAX_NAME_BYTES);
string_id!(EventId, "event ID", MAX_NAME_BYTES);
string_id!(TokenId, "token ID", MAX_NAME_BYTES);
string_id!(GraphNodeId, "graph node ID", MAX_EXPERIENCE_ID_BYTES);
string_id!(InstanceId, "instance ID", MAX_EXPERIENCE_ID_BYTES);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RevisionId(String);

impl RevisionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if is_sha256(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidRevisionId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RevisionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ContractDigest(String);

impl ContractDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if is_sha256(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidContractDigest(value))
        }
    }

    pub fn for_contract(contract: &ExperienceContract) -> Result<Self, Error> {
        Ok(Self(canonical_sha256(contract)?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceRole {
    Ordinary,
    Shell,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValueSchema {
    Null,
    Boolean,
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<i64>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    String {
        max_bytes: usize,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        choices: BTreeSet<String>,
    },
    List {
        max_items: usize,
        items: Box<ValueSchema>,
    },
    Record {
        #[serde(default)]
        fields: BTreeMap<String, FieldSchema>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct FieldSchema {
    #[serde(default)]
    pub required: bool,
    pub value: ValueSchema,
}

impl ValueSchema {
    pub fn empty_record() -> Self {
        Self::Record {
            fields: BTreeMap::new(),
        }
    }

    pub fn validate_definition(&self) -> Result<(), Error> {
        self.validate_definition_at("$", 0)
    }

    pub fn validate_value(&self, value: &Value) -> Result<(), Error> {
        let bytes = canonical_json(value)?;
        if bytes.len() > MAX_BOUNDARY_VALUE_BYTES {
            return Err(Error::BoundaryValueTooLarge {
                actual: bytes.len(),
            });
        }
        self.validate_value_at(value, "$")
    }

    pub fn example_value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Boolean => Value::Bool(false),
            Self::Integer { minimum, maximum } => {
                Value::from(minimum.unwrap_or_else(|| maximum.unwrap_or(0).min(0)))
            }
            Self::Number { minimum, maximum } => {
                Value::from(minimum.unwrap_or_else(|| maximum.unwrap_or(0.0).min(0.0)))
            }
            Self::String { choices, .. } => choices
                .first()
                .cloned()
                .map(Value::String)
                .unwrap_or_else(|| Value::String(String::new())),
            Self::List { .. } => Value::Array(Vec::new()),
            Self::Record { fields } => Value::Object(
                fields
                    .iter()
                    .filter(|(_, field)| field.required)
                    .map(|(name, field)| (name.clone(), field.value.example_value()))
                    .collect(),
            ),
        }
    }

    fn validate_definition_at(&self, path: &str, depth: usize) -> Result<(), Error> {
        if depth > MAX_SCHEMA_DEPTH {
            return Err(Error::SchemaDepth { path: path.into() });
        }
        match self {
            Self::Null | Self::Boolean => Ok(()),
            Self::Integer { minimum, maximum } => validate_range(*minimum, *maximum, path),
            Self::Number { minimum, maximum } => {
                if minimum.is_some_and(|value| !value.is_finite())
                    || maximum.is_some_and(|value| !value.is_finite())
                {
                    return Err(invalid_schema(path, "number bounds must be finite"));
                }
                validate_range(*minimum, *maximum, path)
            }
            Self::String { max_bytes, choices } => {
                if *max_bytes == 0 || *max_bytes > MAX_BOUNDARY_VALUE_BYTES {
                    return Err(invalid_schema(path, "string byte limit is out of bounds"));
                }
                if choices.len() > MAX_SCHEMA_FIELDS
                    || choices.iter().any(|choice| choice.len() > *max_bytes)
                {
                    return Err(invalid_schema(path, "string choices exceed schema limits"));
                }
                Ok(())
            }
            Self::List { max_items, items } => {
                if *max_items > MAX_SCHEMA_LIST_ITEMS {
                    return Err(invalid_schema(path, "list item limit is too large"));
                }
                items.validate_definition_at(&format!("{path}[]"), depth + 1)
            }
            Self::Record { fields } => {
                if fields.len() > MAX_SCHEMA_FIELDS {
                    return Err(invalid_schema(path, "record has too many fields"));
                }
                for (name, field) in fields {
                    if !valid_name(name, MAX_NAME_BYTES) {
                        return Err(invalid_schema(path, &format!("invalid field `{name}`")));
                    }
                    field
                        .value
                        .validate_definition_at(&format!("{path}.{name}"), depth + 1)?;
                }
                Ok(())
            }
        }
    }

    fn validate_value_at(&self, value: &Value, path: &str) -> Result<(), Error> {
        match self {
            Self::Null if value.is_null() => Ok(()),
            Self::Boolean if value.is_boolean() => Ok(()),
            Self::Integer { minimum, maximum } => {
                let actual = value
                    .as_i64()
                    .ok_or_else(|| mismatch(path, "expected an integer"))?;
                validate_value_range(actual, *minimum, *maximum, path)
            }
            Self::Number { minimum, maximum } => {
                let actual = value
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| mismatch(path, "expected a finite number"))?;
                validate_value_range(actual, *minimum, *maximum, path)
            }
            Self::String { max_bytes, choices } => {
                let actual = value
                    .as_str()
                    .ok_or_else(|| mismatch(path, "expected a string"))?;
                if actual.len() > *max_bytes {
                    return Err(mismatch(path, "string is too long"));
                }
                if !choices.is_empty() && !choices.contains(actual) {
                    return Err(mismatch(path, "string is not an allowed choice"));
                }
                Ok(())
            }
            Self::List { max_items, items } => {
                let actual = value
                    .as_array()
                    .ok_or_else(|| mismatch(path, "expected a list"))?;
                if actual.len() > *max_items {
                    return Err(mismatch(path, "list has too many items"));
                }
                for (index, item) in actual.iter().enumerate() {
                    items.validate_value_at(item, &format!("{path}[{index}]"))?;
                }
                Ok(())
            }
            Self::Record { fields } => {
                let actual = value
                    .as_object()
                    .ok_or_else(|| mismatch(path, "expected a record"))?;
                for name in actual.keys() {
                    if !fields.contains_key(name) {
                        return Err(mismatch(path, &format!("unknown field `{name}`")));
                    }
                }
                for (name, field) in fields {
                    match actual.get(name) {
                        Some(value) => field
                            .value
                            .validate_value_at(value, &format!("{path}.{name}"))?,
                        None if field.required => {
                            return Err(mismatch(path, &format!("missing field `{name}`")));
                        }
                        None => {}
                    }
                }
                Ok(())
            }
            _ => Err(mismatch(path, "value has the wrong type")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ViewportContract {
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
}

impl ViewportContract {
    pub fn validate(&self) -> Result<(), Error> {
        if self.min_width == 0
            || self.min_height == 0
            || self.max_width < self.min_width
            || self.max_height < self.min_height
        {
            return Err(Error::InvalidContract("invalid viewport bounds".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ExperienceExport {
    pub properties: ValueSchema,
    #[serde(default)]
    pub events: BTreeMap<EventId, ValueSchema>,
    pub viewport: ViewportContract,
    pub appearance_abi: u32,
    #[serde(default)]
    pub accepts_container_appearance: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ExperienceContract {
    pub contract_version: u32,
    #[serde(default)]
    pub exports: BTreeMap<ExportId, ExperienceExport>,
}

impl ExperienceContract {
    pub fn validate(&self) -> Result<(), Error> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(Error::UnsupportedVersion {
                kind: "contract",
                actual: self.contract_version,
                expected: CONTRACT_VERSION,
            });
        }
        if self.exports.is_empty() || self.exports.len() > MAX_EXPORTS {
            return Err(Error::CountLimit {
                kind: "export",
                actual: self.exports.len(),
                limit: MAX_EXPORTS,
            });
        }
        for (id, export) in &self.exports {
            ExportId::parse(id.as_str())?;
            export.properties.validate_definition()?;
            if export.events.len() > MAX_SCHEMA_FIELDS {
                return Err(Error::InvalidContract(format!(
                    "export `{id}` has too many events"
                )));
            }
            for (event, schema) in &export.events {
                EventId::parse(event.as_str())?;
                schema.validate_definition()?;
            }
            export.viewport.validate()?;
            if export.appearance_abi != APPEARANCE_ABI_VERSION {
                return Err(Error::InvalidContract(format!(
                    "export `{id}` requires unsupported appearance ABI {}",
                    export.appearance_abi
                )));
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<ContractDigest, Error> {
        self.validate()?;
        ContractDigest::for_contract(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyPolicy {
    Locked,
    Tracked,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BoundaryGrant {
    #[serde(default)]
    pub properties: BTreeSet<String>,
    #[serde(default)]
    pub events: BTreeSet<EventId>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DependencyBinding {
    pub experience_id: ExperienceId,
    pub revision_id: RevisionId,
    pub export_id: ExportId,
    pub contract_digest: ContractDigest,
    pub policy: DependencyPolicy,
    #[serde(default)]
    pub grant: BoundaryGrant,
}

impl DependencyBinding {
    pub fn validate(&self, alias: &DependencyAlias) -> Result<(), Error> {
        DependencyAlias::parse(alias.as_str())?;
        ExperienceId::parse(self.experience_id.as_str())?;
        RevisionId::parse(self.revision_id.as_str())?;
        ExportId::parse(self.export_id.as_str())?;
        ContractDigest::parse(self.contract_digest.as_str())?;
        if self
            .grant
            .properties
            .iter()
            .any(|name| !valid_name(name, MAX_NAME_BYTES))
        {
            return Err(Error::InvalidDependency(
                alias.to_string(),
                "grant contains an invalid property name".into(),
            ));
        }
        for event in &self.grant.events {
            EventId::parse(event.as_str())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DerivationKind {
    Original,
    Fork,
    Remix,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DerivationParent {
    pub experience_id: ExperienceId,
    pub revision_id: RevisionId,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DerivationRecord {
    pub kind: DerivationKind,
    #[serde(default)]
    pub parents: Vec<DerivationParent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl DerivationRecord {
    pub fn validate(&self) -> Result<(), Error> {
        if self.parents.len() > MAX_DERIVATION_PARENTS {
            return Err(Error::CountLimit {
                kind: "derivation parent",
                actual: self.parents.len(),
                limit: MAX_DERIVATION_PARENTS,
            });
        }
        let valid_count = match self.kind {
            DerivationKind::Original => self.parents.is_empty(),
            DerivationKind::Fork => self.parents.len() == 1,
            DerivationKind::Remix => self.parents.len() >= 2,
        };
        if !valid_count {
            return Err(Error::InvalidDerivation(
                "kind does not match the number of parents".into(),
            ));
        }
        let mut unique = BTreeSet::new();
        for parent in &self.parents {
            ExperienceId::parse(parent.experience_id.as_str())?;
            RevisionId::parse(parent.revision_id.as_str())?;
            if !unique.insert((parent.experience_id.clone(), parent.revision_id.clone())) {
                return Err(Error::InvalidDerivation(
                    "parent list contains a duplicate".into(),
                ));
            }
        }
        if self
            .request_sha256
            .as_deref()
            .is_some_and(|digest| !is_sha256(digest))
        {
            return Err(Error::InvalidDerivation(
                "request digest is not a lowercase SHA-256 digest".into(),
            ));
        }
        if self
            .rationale
            .as_ref()
            .is_some_and(|value| value.len() > 4096)
        {
            return Err(Error::InvalidDerivation("rationale is too long".into()));
        }
        match self.kind {
            DerivationKind::Original
                if self.request_sha256.is_some() || self.rationale.is_some() =>
            {
                return Err(Error::InvalidDerivation(
                    "original experience cannot carry derivation request metadata".into(),
                ));
            }
            DerivationKind::Fork | DerivationKind::Remix
                if self.request_sha256.is_none()
                    || self
                        .rationale
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty()) =>
            {
                return Err(Error::InvalidDerivation(
                    "fork and remix require a request digest and rationale".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Contrast {
    Standard,
    High,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TypographyToken {
    pub family: String,
    pub size_milli_points: u32,
    pub weight: u16,
    pub line_height_milli: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceProfile {
    pub abi_version: u32,
    pub generation: u64,
    pub scheme: ColorScheme,
    pub contrast: Contrast,
    pub text_scale_milli: u16,
    pub reduce_motion: bool,
    #[serde(default)]
    pub colors: BTreeMap<TokenId, String>,
    #[serde(default)]
    pub spacing: BTreeMap<TokenId, u16>,
    #[serde(default)]
    pub radii: BTreeMap<TokenId, u16>,
    #[serde(default)]
    pub typography: BTreeMap<TokenId, TypographyToken>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContainerAppearance {
    #[serde(default)]
    pub colors: BTreeMap<TokenId, String>,
    #[serde(default)]
    pub spacing: BTreeMap<TokenId, u16>,
    #[serde(default)]
    pub radii: BTreeMap<TokenId, u16>,
}

impl ContainerAppearance {
    pub fn validate(&self) -> Result<(), Error> {
        let count = self.colors.len() + self.spacing.len() + self.radii.len();
        if count > MAX_SCHEMA_FIELDS {
            return Err(Error::CountLimit {
                kind: "container appearance token",
                actual: count,
                limit: MAX_SCHEMA_FIELDS,
            });
        }
        for (token, color) in &self.colors {
            TokenId::parse(token.as_str())?;
            if !valid_rgba(color) {
                return Err(Error::InvalidAppearance(format!(
                    "container color token `{token}` is not #RRGGBBAA"
                )));
            }
        }
        for token in self.spacing.keys().chain(self.radii.keys()) {
            TokenId::parse(token.as_str())?;
        }
        let bytes = canonical_json(self)?;
        if bytes.len() > MAX_BOUNDARY_VALUE_BYTES {
            return Err(Error::BoundaryValueTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(())
    }
}

impl Default for AppearanceProfile {
    fn default() -> Self {
        Self {
            abi_version: APPEARANCE_ABI_VERSION,
            generation: 0,
            scheme: ColorScheme::Dark,
            contrast: Contrast::Standard,
            text_scale_milli: 1000,
            reduce_motion: false,
            colors: BTreeMap::new(),
            spacing: BTreeMap::new(),
            radii: BTreeMap::new(),
            typography: BTreeMap::new(),
        }
    }
}

impl AppearanceProfile {
    pub fn validate(&self) -> Result<(), Error> {
        if self.abi_version != APPEARANCE_ABI_VERSION {
            return Err(Error::UnsupportedVersion {
                kind: "appearance ABI",
                actual: self.abi_version,
                expected: APPEARANCE_ABI_VERSION,
            });
        }
        if !(500..=3000).contains(&self.text_scale_milli) {
            return Err(Error::InvalidAppearance(
                "text scale must be between 500 and 3000".into(),
            ));
        }
        let token_count =
            self.colors.len() + self.spacing.len() + self.radii.len() + self.typography.len();
        if token_count > MAX_SCHEMA_FIELDS * 4 {
            return Err(Error::CountLimit {
                kind: "appearance token",
                actual: token_count,
                limit: MAX_SCHEMA_FIELDS * 4,
            });
        }
        for (token, color) in &self.colors {
            TokenId::parse(token.as_str())?;
            if !valid_rgba(color) {
                return Err(Error::InvalidAppearance(format!(
                    "color token `{token}` is not #RRGGBBAA"
                )));
            }
        }
        for token in self.spacing.keys().chain(self.radii.keys()) {
            TokenId::parse(token.as_str())?;
        }
        for (token, typography) in &self.typography {
            TokenId::parse(token.as_str())?;
            if typography.family.is_empty()
                || typography.family.len() > 128
                || !(1_000..=512_000).contains(&typography.size_milli_points)
                || !(1..=1000).contains(&typography.weight)
                || !(500..=4000).contains(&typography.line_height_milli)
            {
                return Err(Error::InvalidAppearance(format!(
                    "typography token `{token}` is outside its bounds"
                )));
            }
        }
        let bytes = canonical_json(self)?;
        if bytes.len() > MAX_BOUNDARY_VALUE_BYTES {
            return Err(Error::BoundaryValueTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PackageMetadata {
    pub format_version: u32,
    pub experience_id: ExperienceId,
    pub role: ExperienceRole,
    pub contract: ExperienceContract,
    #[serde(default)]
    pub dependencies: BTreeMap<DependencyAlias, DependencyBinding>,
    pub derivation: DerivationRecord,
}

impl PackageMetadata {
    pub fn validate(&self) -> Result<(), Error> {
        if self.format_version != PACKAGE_FORMAT_VERSION {
            return Err(Error::UnsupportedVersion {
                kind: "package",
                actual: self.format_version,
                expected: PACKAGE_FORMAT_VERSION,
            });
        }
        ExperienceId::parse(self.experience_id.as_str())?;
        self.contract.validate()?;
        self.derivation.validate()?;
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return Err(Error::CountLimit {
                kind: "dependency",
                actual: self.dependencies.len(),
                limit: MAX_DEPENDENCIES,
            });
        }
        for (alias, dependency) in &self.dependencies {
            dependency.validate(alias)?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let package: Self = decode_canonical_json(bytes, "package", MAX_PACKAGE_METADATA_BYTES)?;
        package.validate()?;
        Ok(package)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResolvedGraphNode {
    pub experience_id: ExperienceId,
    pub revision_id: RevisionId,
    pub export_id: ExportId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency: Option<DependencyAlias>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResolvedGraph {
    pub format_version: u32,
    pub root: GraphNodeId,
    pub nodes: BTreeMap<GraphNodeId, ResolvedGraphNode>,
}

impl ResolvedGraph {
    pub fn validate(&self) -> Result<(), Error> {
        if self.format_version != GRAPH_FORMAT_VERSION {
            return Err(Error::UnsupportedVersion {
                kind: "graph",
                actual: self.format_version,
                expected: GRAPH_FORMAT_VERSION,
            });
        }
        if self.nodes.is_empty() || self.nodes.len() > MAX_GRAPH_INSTANCES {
            return Err(Error::CountLimit {
                kind: "graph instance",
                actual: self.nodes.len(),
                limit: MAX_GRAPH_INSTANCES,
            });
        }
        if !self.nodes.contains_key(&self.root) {
            return Err(Error::InvalidGraph("root node is missing".into()));
        }
        let mut experience_revisions = BTreeMap::new();
        for (id, node) in &self.nodes {
            GraphNodeId::parse(id.as_str())?;
            ExperienceId::parse(node.experience_id.as_str())?;
            RevisionId::parse(node.revision_id.as_str())?;
            ExportId::parse(node.export_id.as_str())?;
            if let Some(existing) =
                experience_revisions.insert(node.experience_id.clone(), node.revision_id.clone())
            {
                if existing != node.revision_id {
                    return Err(Error::InvalidGraph(format!(
                        "experience `{}` appears at more than one revision",
                        node.experience_id
                    )));
                }
            }
            if id == &self.root {
                if node.parent.is_some() || node.dependency.is_some() {
                    return Err(Error::InvalidGraph(
                        "root node cannot have a parent or dependency alias".into(),
                    ));
                }
            } else if node.parent.is_none() || node.dependency.is_none() {
                return Err(Error::InvalidGraph(format!(
                    "node `{id}` lacks its parent binding"
                )));
            }
            let mut depth = 0;
            let mut cursor = Some(id);
            let mut visited = BTreeSet::new();
            while let Some(current) = cursor {
                if !visited.insert(current.clone()) {
                    return Err(Error::InvalidGraph(format!(
                        "cycle contains node `{current}`"
                    )));
                }
                let current_node = self.nodes.get(current).ok_or_else(|| {
                    Error::InvalidGraph(format!("parent node `{current}` is missing"))
                })?;
                cursor = current_node.parent.as_ref();
                if cursor.is_some() {
                    depth += 1;
                    if depth > MAX_GRAPH_DEPTH {
                        return Err(Error::InvalidGraph(format!(
                            "node `{id}` exceeds graph depth {MAX_GRAPH_DEPTH}"
                        )));
                    }
                }
            }
            if !visited.contains(&self.root) {
                return Err(Error::InvalidGraph(format!(
                    "node `{id}` is disconnected from the root"
                )));
            }
        }
        Ok(())
    }

    pub fn id(&self) -> Result<String, Error> {
        self.validate()?;
        canonical_sha256(self)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let graph: Self = decode_canonical_json(bytes, "resolved graph", MAX_RESOLVED_GRAPH_BYTES)?;
        graph.validate()?;
        Ok(graph)
    }
}

pub fn decode_canonical_json<T>(bytes: &[u8], kind: &'static str, limit: usize) -> Result<T, Error>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.len() > limit {
        return Err(Error::WirePayloadTooLarge {
            kind,
            actual: bytes.len(),
            limit,
        });
    }
    let decoded = serde_json::from_slice(bytes)?;
    if canonical_json(&decoded)? != bytes {
        return Err(Error::NonCanonicalJson { kind });
    }
    Ok(decoded)
}

pub fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    serde_jcs::to_vec(value).map_err(|error| Error::CanonicalJson(error.to_string()))
}

pub fn canonical_sha256<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    Ok(hex_sha256(&canonical_json(value)?))
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_name(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_rgba(value: &str) -> bool {
    value.len() == 9
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_range<T>(minimum: Option<T>, maximum: Option<T>, path: &str) -> Result<(), Error>
where
    T: PartialOrd,
{
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum > maximum) {
        Err(invalid_schema(path, "minimum exceeds maximum"))
    } else {
        Ok(())
    }
}

fn validate_value_range<T>(
    actual: T,
    minimum: Option<T>,
    maximum: Option<T>,
    path: &str,
) -> Result<(), Error>
where
    T: PartialOrd,
{
    if minimum.is_some_and(|minimum| actual < minimum)
        || maximum.is_some_and(|maximum| actual > maximum)
    {
        Err(mismatch(path, "number is outside its allowed range"))
    } else {
        Ok(())
    }
}

fn invalid_schema(path: &str, reason: &str) -> Error {
    Error::InvalidSchema {
        path: path.into(),
        reason: reason.into(),
    }
}

fn mismatch(path: &str, reason: &str) -> Error {
    Error::ValueMismatch {
        path: path.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(byte: char) -> RevisionId {
        RevisionId::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn contract() -> ExperienceContract {
        ExperienceContract {
            contract_version: CONTRACT_VERSION,
            exports: BTreeMap::from([(
                ExportId::parse("summary").unwrap(),
                ExperienceExport {
                    properties: ValueSchema::Record {
                        fields: BTreeMap::from([(
                            "title".into(),
                            FieldSchema {
                                required: true,
                                value: ValueSchema::String {
                                    max_bytes: 64,
                                    choices: BTreeSet::new(),
                                },
                            },
                        )]),
                    },
                    events: BTreeMap::from([(
                        EventId::parse("open").unwrap(),
                        ValueSchema::empty_record(),
                    )]),
                    viewport: ViewportContract {
                        min_width: 160,
                        min_height: 96,
                        max_width: 1920,
                        max_height: 1080,
                    },
                    appearance_abi: APPEARANCE_ABI_VERSION,
                    accepts_container_appearance: false,
                },
            )]),
        }
    }

    #[test]
    fn canonical_json_sorts_object_keys_recursively() {
        let value = serde_json::json!({"z": {"b": 2, "a": 1}, "a": [3, 2, 1]});
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            r#"{"a":[3,2,1],"z":{"a":1,"b":2}}"#
        );
        assert_eq!(
            canonical_sha256(&value).unwrap(),
            "f613a572f53e0e577f557af7e41633d7c30546ae97e1e20b8ad0dbea7118d7a6"
        );
    }

    #[test]
    fn closed_record_rejects_unknown_and_missing_fields() {
        let contract = contract();
        let schema = &contract.exports.values().next().unwrap().properties;
        schema
            .validate_value(&serde_json::json!({"title": "Agenda"}))
            .unwrap();
        assert!(matches!(
            schema.validate_value(&serde_json::json!({"title": "Agenda", "secret": 1})),
            Err(Error::ValueMismatch { .. })
        ));
        assert!(matches!(
            schema.validate_value(&serde_json::json!({})),
            Err(Error::ValueMismatch { .. })
        ));
    }

    #[test]
    fn contract_digest_is_stable() {
        let contract = contract();
        contract.validate().unwrap();
        assert_eq!(contract.digest().unwrap(), contract.digest().unwrap());
    }

    #[test]
    fn generated_examples_respect_negative_numeric_bounds() {
        let integer = ValueSchema::Integer {
            minimum: None,
            maximum: Some(-4),
        };
        let number = ValueSchema::Number {
            minimum: None,
            maximum: Some(-0.5),
        };
        integer.validate_value(&integer.example_value()).unwrap();
        number.validate_value(&number.example_value()).unwrap();
    }

    #[test]
    fn package_rejects_derivation_mismatch() {
        let metadata = PackageMetadata {
            format_version: PACKAGE_FORMAT_VERSION,
            experience_id: ExperienceId::parse("agenda").unwrap(),
            role: ExperienceRole::Ordinary,
            contract: contract(),
            dependencies: BTreeMap::new(),
            derivation: DerivationRecord {
                kind: DerivationKind::Fork,
                parents: vec![],
                request_sha256: None,
                rationale: None,
            },
        };
        assert!(matches!(
            metadata.validate(),
            Err(Error::InvalidDerivation(_))
        ));
    }

    #[test]
    fn graph_rejects_cycles_and_disconnected_nodes() {
        let root = GraphNodeId::parse("root").unwrap();
        let child = GraphNodeId::parse("child").unwrap();
        let mut graph = ResolvedGraph {
            format_version: GRAPH_FORMAT_VERSION,
            root: root.clone(),
            nodes: BTreeMap::from([
                (
                    root.clone(),
                    ResolvedGraphNode {
                        experience_id: ExperienceId::parse("dashboard").unwrap(),
                        revision_id: revision('a'),
                        export_id: ExportId::parse("main").unwrap(),
                        parent: None,
                        dependency: None,
                    },
                ),
                (
                    child.clone(),
                    ResolvedGraphNode {
                        experience_id: ExperienceId::parse("agenda").unwrap(),
                        revision_id: revision('b'),
                        export_id: ExportId::parse("summary").unwrap(),
                        parent: Some(root.clone()),
                        dependency: Some(DependencyAlias::parse("agenda").unwrap()),
                    },
                ),
            ]),
        };
        graph.validate().unwrap();
        assert_eq!(graph.id().unwrap().len(), 64);

        graph.nodes.get_mut(&root).unwrap().parent = Some(child);
        assert!(matches!(graph.validate(), Err(Error::InvalidGraph(_))));
    }

    #[test]
    fn appearance_rejects_untyped_colors() {
        let mut appearance = AppearanceProfile {
            abi_version: APPEARANCE_ABI_VERSION,
            generation: 1,
            scheme: ColorScheme::Dark,
            contrast: Contrast::Standard,
            text_scale_milli: 1000,
            reduce_motion: false,
            colors: BTreeMap::from([(
                TokenId::parse("surface.primary").unwrap(),
                "#101820ff".into(),
            )]),
            spacing: BTreeMap::new(),
            radii: BTreeMap::new(),
            typography: BTreeMap::new(),
        };
        appearance.validate().unwrap();
        appearance
            .colors
            .values_mut()
            .next()
            .unwrap()
            .clone_from(&"red".into());
        assert!(matches!(
            appearance.validate(),
            Err(Error::InvalidAppearance(_))
        ));
    }
}
