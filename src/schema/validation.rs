use crate::schema::definition::{IndexDefinition, PartitionStrategy};

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

/// Validates an IndexDefinition configuration.
/// Ensures that partition constraints, retention formats, and field constraints are correct.
pub fn validate_schema(def: &IndexDefinition) -> Result<(), ValidationError> {
    // If partitioning is enabled, the timestamp field MUST exist in the fields map
    if def.partition != PartitionStrategy::None && !def.fields.contains_key(&def.timestamp_field) {
        return Err(ValidationError::MissingTimestampField(
            def.timestamp_field.clone(),
        ));
    }

    // If retention is configured, ensure it follows the correct format (e.g. 7d, 30m, 1Y)
    if let Some(ref retention) = def.retention
        && !is_valid_retention(retention)
    {
        return Err(ValidationError::InvalidRetentionFormat(retention.clone()));
    }

    Ok(())
}

/// Checks if a retention string strictly follows the numeric + unit format
pub fn is_valid_retention(retention: &str) -> bool {
    if retention.is_empty() {
        return false;
    }
    let (num_part, unit_part) = retention.split_at(retention.len() - 1);
    num_part.parse::<u64>().is_ok() && matches!(unit_part, "s" | "m" | "h" | "d" | "w" | "M" | "y")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{CompressionOption, FieldDefinition, FieldType, SchemaMode};
    use std::collections::HashMap;

    #[test]
    fn test_is_valid_retention() {
        assert!(is_valid_retention("7d"));
        assert!(is_valid_retention("14d"));
        assert!(is_valid_retention("1M"));
        assert!(is_valid_retention("12h"));
        assert!(is_valid_retention("5y"));

        assert!(!is_valid_retention("7"));
        assert!(!is_valid_retention("d"));
        assert!(!is_valid_retention("7days"));
        assert!(!is_valid_retention("-7d"));
    }

    #[test]
    fn test_validate_schema_missing_timestamp() {
        let def = IndexDefinition {
            name: "logs".to_string(),
            description: None,
            timestamp_field: "created_at".to_string(), // configured to look for 'created_at'
            mode: SchemaMode::Dynamic,
            partition: PartitionStrategy::Daily, // partitioning enabled
            retention: None,
            compression: CompressionOption::Zstd,
            fields: HashMap::new(), // 'created_at' is missing!
            settings: HashMap::new(),
        };

        let result = validate_schema(&def);
        assert!(matches!(
            result,
            Err(ValidationError::MissingTimestampField(f)) if f == "created_at"
        ));
    }

    #[test]
    fn test_validate_schema_success() {
        let mut fields = HashMap::new();
        fields.insert(
            "timestamp".to_string(),
            FieldDefinition {
                field_type: FieldType::Datetime,
                indexed: true,
                stored: false,
                fast: true,
                optional: false,
                default: None,
            },
        );

        let def = IndexDefinition {
            name: "logs".to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode: SchemaMode::Dynamic,
            partition: PartitionStrategy::Daily,
            retention: Some("30d".to_string()),
            compression: CompressionOption::Zstd,
            fields,
            settings: HashMap::new(),
        };

        assert!(validate_schema(&def).is_ok());
    }
}
