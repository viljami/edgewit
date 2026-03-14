use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::schema::validation::{ValidationError, validate_schema};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SchemaMode {
    Strict,
    DropUnmapped,
    #[default]
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PartitionStrategy {
    #[default]
    None,
    Daily,
    Hourly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CompressionOption {
    None,
    #[default]
    Zstd,
    Lz4,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Keyword,
    Datetime,
    Integer,
    Float,
    Boolean,
    Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct FieldDefinition {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub indexed: bool,
    #[serde(default)]
    pub stored: bool,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct IndexDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_timestamp_field")]
    pub timestamp_field: String,
    #[serde(default)]
    pub mode: SchemaMode,
    #[serde(default)]
    pub partition: PartitionStrategy,
    pub retention: Option<String>,
    #[serde(default)]
    pub compression: CompressionOption,
    pub fields: HashMap<String, FieldDefinition>,
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

fn default_timestamp_field() -> String {
    "timestamp".to_string()
}

impl IndexDefinition {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ValidationError> {
        let content = fs::read_to_string(path)?;
        let def: IndexDefinition = serde_yaml::from_str(&content)?;
        validate_schema(&def)?;
        Ok(def)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_schema(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = r#"
name: logs
description: application log events
timestamp_field: timestamp
mode: drop_unmapped
partition: daily
retention: 7d
compression: zstd
fields:
  timestamp:
    type: datetime
    indexed: true
    fast: true
  level:
    type: keyword
  message:
    type: text
"#;
        let def: IndexDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.name, "logs");
        assert_eq!(def.timestamp_field, "timestamp");
        assert_eq!(def.mode, SchemaMode::DropUnmapped);
        assert_eq!(def.partition, PartitionStrategy::Daily);
        assert_eq!(def.retention, Some("7d".to_string()));
        assert_eq!(def.compression, CompressionOption::Zstd);
        assert!(def.fields.contains_key("timestamp"));

        assert!(def.validate().is_ok());
    }
}
