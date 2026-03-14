use tantivy::schema::{
    BytesOptions, DateOptions, IndexRecordOption, NumericOptions, STORED, Schema,
    TextFieldIndexing, TextOptions,
};
use thiserror::Error;

use crate::schema::definition::{FieldType, IndexDefinition, SchemaMode};

#[derive(Debug, Error)]
pub enum SchemaBuilderError {
    #[error("validation error: {0}")]
    ValidationError(String),
}

/// Converts an Edgewit IndexDefinition into a Tantivy Schema.
pub fn build_schema(definition: &IndexDefinition) -> Result<Schema, SchemaBuilderError> {
    let mut builder = Schema::builder();

    for (name, field_def) in &definition.fields {
        match field_def.field_type {
            FieldType::Text => {
                let mut options = TextOptions::default();
                if field_def.indexed {
                    options = options.set_indexing_options(
                        TextFieldIndexing::default()
                            .set_tokenizer("default")
                            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
                    );
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    // Fast fields for text are typically used for faceting/sorting but can consume memory
                    options = options.set_fast(None);
                }
                builder.add_text_field(name, options);
            }
            FieldType::Keyword => {
                let mut options = TextOptions::default();
                if field_def.indexed {
                    options = options.set_indexing_options(
                        TextFieldIndexing::default()
                            .set_tokenizer("raw")
                            .set_index_option(IndexRecordOption::Basic),
                    );
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast(None);
                }
                builder.add_text_field(name, options);
            }
            FieldType::Datetime => {
                let mut options = DateOptions::default();
                if field_def.indexed {
                    options = options.set_indexed();
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast();
                }
                builder.add_date_field(name, options);
            }
            FieldType::Integer => {
                let mut options = NumericOptions::default();
                if field_def.indexed {
                    options = options.set_indexed();
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast();
                }
                builder.add_i64_field(name, options);
            }
            FieldType::Float => {
                let mut options = NumericOptions::default();
                if field_def.indexed {
                    options = options.set_indexed();
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast();
                }
                builder.add_f64_field(name, options);
            }
            FieldType::Boolean => {
                let mut options = NumericOptions::default();
                if field_def.indexed {
                    options = options.set_indexed();
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast();
                }
                builder.add_bool_field(name, options);
            }
            FieldType::Bytes => {
                let mut options = BytesOptions::default();
                if field_def.indexed {
                    options = options.set_indexed();
                }
                if field_def.stored {
                    options = options.set_stored();
                }
                if field_def.fast {
                    options = options.set_fast();
                }
                builder.add_bytes_field(name, options);
            }
        }
    }

    // Edgewit typically stores the original JSON document as `_source`.
    // It is fast and stored to allow quick full-document retrieval.
    builder.add_json_field("_source", STORED);

    // If dynamic mode is enabled, we could catch unmapped fields in a dynamic JSON field,
    // but for now, we rely on _source for document retention, and explicitly mapped fields
    // for indexing and search, to maintain predictable performance at the edge.
    if definition.mode == SchemaMode::Dynamic {
        // Option to add a catch-all dynamic JSON field for indexing if desired:
        // builder.add_json_field("_dynamic", TEXT | FAST);
    }

    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{CompressionOption, FieldDefinition, PartitionStrategy};
    use std::collections::HashMap;

    #[test]
    fn test_build_schema_success() {
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
            "message".to_string(),
            FieldDefinition {
                field_type: FieldType::Text,
                indexed: true,
                stored: false,
                fast: false,
                optional: true,
                default: None,
            },
        );
        fields.insert(
            "status".to_string(),
            FieldDefinition {
                field_type: FieldType::Integer,
                indexed: true,
                stored: true,
                fast: true,
                optional: true,
                default: None,
            },
        );

        let def = IndexDefinition {
            name: "logs".to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode: SchemaMode::DropUnmapped,
            partition: PartitionStrategy::Daily,
            retention: Some("7d".to_string()),
            compression: CompressionOption::Zstd,
            fields,
            settings: HashMap::new(),
        };

        let schema = build_schema(&def).unwrap();

        let _ = schema
            .get_field("timestamp")
            .expect("failed to get field timestamp");
        let _ = schema
            .get_field("message")
            .expect("failed to get field message");
        let _ = schema
            .get_field("status")
            .expect("failed to get field status");
        let _ = schema
            .get_field("_source")
            .expect("failed to get field _source");

        assert!(schema.get_field("timestamp").is_ok());
        assert!(schema.get_field("_source").is_ok());
    }
}
