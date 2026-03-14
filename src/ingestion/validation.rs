use crate::schema::definition::{FieldType, IndexDefinition, SchemaMode};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum IngestionValidationError {
    #[error("document must be a json object")]
    NotAnObject,
    #[error("unknown field '{0}' is not allowed in strict mode")]
    UnknownField(String),
    #[error("missing required field '{0}'")]
    MissingRequiredField(String),
    #[error("type mismatch for field '{field}': expected {expected_type:?}")]
    TypeMismatch {
        field: String,
        expected_type: FieldType,
    },
}

/// Validates an incoming JSON document against an IndexDefinition schema.
/// It applies defaults, enforces strictness rules based on SchemaMode, and type-checks fields.
pub fn validate_and_process_document(
    def: &IndexDefinition,
    document: Value,
) -> Result<Value, IngestionValidationError> {
    let mut obj = match document {
        Value::Object(map) => map,
        _ => return Err(IngestionValidationError::NotAnObject),
    };

    // 1. Handle Schema Mode (Strict vs DropUnmapped vs Dynamic)
    let mut keys_to_remove = Vec::new();
    for key in obj.keys() {
        if !def.fields.contains_key(key) && key != "_source" {
            // Optional protection if users inject _source
            match def.mode {
                SchemaMode::Strict => {
                    return Err(IngestionValidationError::UnknownField(key.clone()));
                }
                SchemaMode::DropUnmapped => {
                    keys_to_remove.push(key.clone());
                }
                SchemaMode::Dynamic => {
                    // Allowed, do nothing
                }
            }
        }
    }

    for key in keys_to_remove {
        obj.remove(&key);
    }

    // 2. Enforce field types, defaults, and optionality
    for (field_name, field_def) in &def.fields {
        if let Some(val) = obj.get(field_name) {
            // Document has the field, let's validate its type
            if val.is_null() {
                if !field_def.optional {
                    return Err(IngestionValidationError::MissingRequiredField(
                        field_name.clone(),
                    ));
                }
            } else {
                let is_valid = match field_def.field_type {
                    FieldType::Text | FieldType::Keyword => val.is_string(),
                    FieldType::Integer => val.is_i64() || val.is_u64(),
                    FieldType::Float => val.is_number(),
                    FieldType::Boolean => val.is_boolean(),
                    FieldType::Datetime => val.is_string() || val.is_number(),
                    FieldType::Bytes => val.is_string() || val.is_array(),
                };

                if !is_valid {
                    return Err(IngestionValidationError::TypeMismatch {
                        field: field_name.clone(),
                        expected_type: field_def.field_type.clone(),
                    });
                }
            }
        } else {
            // Missing field in the document
            if let Some(default_val) = &field_def.default {
                obj.insert(field_name.clone(), default_val.clone());
            } else if !field_def.optional {
                return Err(IngestionValidationError::MissingRequiredField(
                    field_name.clone(),
                ));
            }
        }
    }

    Ok(Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{CompressionOption, FieldDefinition, PartitionStrategy};
    use serde_json::json;
    use std::collections::HashMap;

    fn create_test_definition(mode: SchemaMode) -> IndexDefinition {
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

        fields.insert(
            "level".to_string(),
            FieldDefinition {
                field_type: FieldType::Keyword,
                indexed: true,
                stored: false,
                fast: false,
                optional: true,
                default: Some(json!("INFO")),
            },
        );

        fields.insert(
            "status".to_string(),
            FieldDefinition {
                field_type: FieldType::Integer,
                indexed: true,
                stored: false,
                fast: false,
                optional: false,
                default: None,
            },
        );

        IndexDefinition {
            name: "logs".to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode,
            partition: PartitionStrategy::None,
            retention: None,
            compression: CompressionOption::Zstd,
            fields,
            settings: HashMap::new(),
        }
    }

    #[test]
    fn test_valid_document() {
        let def = create_test_definition(SchemaMode::Strict);
        let doc = json!({
            "timestamp": "2023-01-01T12:00:00Z",
            "level": "ERROR",
            "status": 200
        });

        let result = validate_and_process_document(&def, doc);
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_required_field() {
        let def = create_test_definition(SchemaMode::Strict);
        let doc = json!({
            "level": "ERROR",
            "status": 200
        }); // Missing "timestamp"

        let result = validate_and_process_document(&def, doc);
        assert!(matches!(
            result,
            Err(IngestionValidationError::MissingRequiredField(f)) if f == "timestamp"
        ));
    }

    #[test]
    fn test_default_value_application() {
        let def = create_test_definition(SchemaMode::Strict);
        let doc = json!({
            "timestamp": 1672574400, // Valid integer for datetime
            "status": 500
        }); // Missing "level" but it has a default

        let result = validate_and_process_document(&def, doc).unwrap();

        // Assert the "level" was populated with the default "INFO"
        assert_eq!(result.get("level").unwrap().as_str().unwrap(), "INFO");
    }

    #[test]
    fn test_strict_mode_rejects_unknown() {
        let def = create_test_definition(SchemaMode::Strict);
        let doc = json!({
            "timestamp": "2023-01-01T12:00:00Z",
            "status": 200,
            "extra_field": "hello"
        });

        let result = validate_and_process_document(&def, doc);
        assert!(matches!(
            result,
            Err(IngestionValidationError::UnknownField(f)) if f == "extra_field"
        ));
    }

    #[test]
    fn test_drop_unmapped_mode() {
        let def = create_test_definition(SchemaMode::DropUnmapped);
        let doc = json!({
            "timestamp": "2023-01-01T12:00:00Z",
            "status": 200,
            "extra_field": "hello"
        });

        let result = validate_and_process_document(&def, doc).unwrap();

        // Ensure "extra_field" is silently removed
        assert!(result.get("extra_field").is_none());
        assert!(result.get("timestamp").is_some());
    }

    #[test]
    fn test_dynamic_mode_allows_unknown() {
        let def = create_test_definition(SchemaMode::Dynamic);
        let doc = json!({
            "timestamp": "2023-01-01T12:00:00Z",
            "status": 200,
            "extra_field": "hello"
        });

        let result = validate_and_process_document(&def, doc).unwrap();

        // Ensure "extra_field" remains intact
        assert!(result.get("extra_field").is_some());
        assert_eq!(
            result.get("extra_field").unwrap().as_str().unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_type_mismatch() {
        let def = create_test_definition(SchemaMode::Strict);
        let doc = json!({
            "timestamp": "2023-01-01T12:00:00Z",
            "status": "not an integer" // Invalid type
        });

        let result = validate_and_process_document(&def, doc);
        assert!(matches!(
            result,
            Err(IngestionValidationError::TypeMismatch { field, .. }) if field == "status"
        ));
    }
}
