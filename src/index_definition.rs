use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaMode {
    Strict,
    DropUnmapped,
    Dynamic,
}

impl Default for SchemaMode {
    fn default() -> Self {
        SchemaMode::Dynamic
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PartitionStrategy {
    None,
    Daily,
    Hourly,
    Monthly,
}

impl Default for PartitionStrategy {
    fn default() -> Self {
        PartitionStrategy::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionOption {
    None,
    Zstd,
    Lz4,
}

impl Default for CompressionOption {
    fn default() -> Self {
        CompressionOption::Zstd
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("timestamp field '{0}' is required for partitioning but missing from fields")]
    MissingTimestampField(String),
    #[error("retention format invalid: {0}")]
    InvalidRetentionFormat(String),
    #[error("failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("failed to parse yaml: {0}")]
    YamlError(#[from] serde_yaml::Error),
}

impl IndexDefinition {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ValidationError> {
        let content = fs::read_to_string(path)?;
        let def: IndexDefinition = serde_yaml::from_str(&content)?;
        def.validate()?;
        Ok(def)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.partition != PartitionStrategy::None {
            if !self.fields.contains_key(&self.timestamp_field) {
                return Err(ValidationError::MissingTimestampField(
                    self.timestamp_field.clone(),
                ));
            }
        }

        if let Some(ref retention) = self.retention {
            if !is_valid_retention(retention) {
                return Err(ValidationError::InvalidRetentionFormat(retention.clone()));
            }
        }

        Ok(())
    }
}

fn is_valid_retention(retention: &str) -> bool {
    if retention.is_empty() {
        return false;
    }
    let (num_part, unit_part) = retention.split_at(retention.len() - 1);
    num_part.parse::<u64>().is_ok() && matches!(unit_part, "s" | "m" | "h" | "d" | "w" | "M" | "Y")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_retention() {
        assert!(is_valid_retention("7d"));
        assert!(is_valid_retention("14d"));
        assert!(is_valid_retention("1M"));
        assert!(is_valid_retention("12h"));

        assert!(!is_valid_retention("7"));
        assert!(!is_valid_retention("d"));
        assert!(!is_valid_retention("7days"));
        assert!(!is_valid_retention("-7d"));
    }

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

    #[test]
    fn test_missing_timestamp_field_when_partitioned() {
        let yaml = r#"
name: logs
timestamp_field: created_at
partition: daily
fields:
  message:
    type: text
"#;
        let def: IndexDefinition = serde_yaml::from_str(yaml).unwrap();
        let result = def.validate();
        assert!(matches!(
            result,
            Err(ValidationError::MissingTimestampField(_))
        ));
    }
}
