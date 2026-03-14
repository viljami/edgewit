use crate::schema::definition::{IndexDefinition, PartitionStrategy};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PartitionError {
    #[error("timestamp field '{0}' missing or invalid in document")]
    InvalidTimestamp(String),
    #[error("failed to parse timestamp: {0}")]
    ParseError(String),
}

/// Resolves the correct partition directory name for a given document based on the index schema.
pub fn resolve_partition(
    def: &IndexDefinition,
    document: &Value,
) -> Result<Option<String>, PartitionError> {
    if def.partition == PartitionStrategy::None {
        return Ok(None);
    }

    let timestamp_val = document
        .get(&def.timestamp_field)
        .ok_or_else(|| PartitionError::InvalidTimestamp(def.timestamp_field.clone()))?;

    let dt: DateTime<Utc> = match timestamp_val {
        Value::String(s) => {
            // Try parsing as RFC3339 (ISO 8601 subset)
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    // Fallback to standard DateTime<Utc> parsing if not exactly RFC3339
                    s.parse::<DateTime<Utc>>()
                })
                .map_err(|e| PartitionError::ParseError(e.to_string()))?
        }
        Value::Number(n) => {
            if let Some(timestamp_sec) = n.as_i64() {
                Utc.timestamp_opt(timestamp_sec, 0)
                    .single()
                    .ok_or_else(|| {
                        PartitionError::ParseError("invalid unix timestamp".to_string())
                    })?
            } else if let Some(timestamp_f64) = n.as_f64() {
                Utc.timestamp_opt(timestamp_f64 as i64, 0)
                    .single()
                    .ok_or_else(|| {
                        PartitionError::ParseError("invalid unix timestamp float".to_string())
                    })?
            } else {
                return Err(PartitionError::ParseError(
                    "unsupported number format".to_string(),
                ));
            }
        }
        _ => {
            return Err(PartitionError::InvalidTimestamp(
                def.timestamp_field.clone(),
            ));
        }
    };

    let partition_name = format_partition_name(&dt, &def.partition);
    Ok(Some(partition_name))
}

/// Formats a DateTime into a deterministic partition directory string.
pub fn format_partition_name(dt: &DateTime<Utc>, strategy: &PartitionStrategy) -> String {
    match strategy {
        PartitionStrategy::None => "".to_string(),
        PartitionStrategy::Daily => dt.format("%Y-%m-%d").to_string(),
        PartitionStrategy::Hourly => dt.format("%Y-%m-%d-%H").to_string(),
        PartitionStrategy::Monthly => dt.format("%Y-%m").to_string(),
    }
}

/// Builds the absolute path to the partition directory.
pub fn get_partition_path(
    base_dir: &PathBuf,
    index_name: &str,
    partition: Option<&str>,
) -> PathBuf {
    let mut path = base_dir.clone();
    path.push("indexes");
    path.push(index_name);
    path.push("segments");
    if let Some(p) = partition {
        path.push(p);
    } else {
        // If no partitioning is configured, use a 'default' partition directory
        path.push("default");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{CompressionOption, SchemaMode};
    use serde_json::json;
    use std::collections::HashMap;

    fn test_def(strategy: PartitionStrategy) -> IndexDefinition {
        IndexDefinition {
            name: "logs".to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode: SchemaMode::Dynamic,
            partition: strategy,
            retention: None,
            compression: CompressionOption::Zstd,
            fields: HashMap::new(),
            settings: HashMap::new(),
        }
    }

    #[test]
    fn test_resolve_partition_none() {
        let def = test_def(PartitionStrategy::None);
        let doc = json!({ "timestamp": "2023-10-25T14:30:00Z" });
        assert_eq!(resolve_partition(&def, &doc).unwrap(), None);
    }

    #[test]
    fn test_resolve_partition_daily() {
        let def = test_def(PartitionStrategy::Daily);

        let doc1 = json!({ "timestamp": "2023-10-25T14:30:00Z" });
        assert_eq!(
            resolve_partition(&def, &doc1).unwrap(),
            Some("2023-10-25".to_string())
        );

        let doc2 = json!({ "timestamp": 1698244200 }); // 2023-10-25T14:30:00Z
        assert_eq!(
            resolve_partition(&def, &doc2).unwrap(),
            Some("2023-10-25".to_string())
        );
    }

    #[test]
    fn test_resolve_partition_hourly() {
        let def = test_def(PartitionStrategy::Hourly);
        let doc = json!({ "timestamp": "2023-10-25T14:30:00Z" });
        assert_eq!(
            resolve_partition(&def, &doc).unwrap(),
            Some("2023-10-25-14".to_string())
        );
    }

    #[test]
    fn test_resolve_partition_monthly() {
        let def = test_def(PartitionStrategy::Monthly);
        let doc = json!({ "timestamp": "2023-10-25T14:30:00Z" });
        assert_eq!(
            resolve_partition(&def, &doc).unwrap(),
            Some("2023-10".to_string())
        );
    }

    #[test]
    fn test_invalid_timestamp() {
        let def = test_def(PartitionStrategy::Daily);
        let doc = json!({ "timestamp": "invalid-date" });
        assert!(resolve_partition(&def, &doc).is_err());
    }

    #[test]
    fn test_get_partition_path() {
        let base_dir = PathBuf::from("/data");

        let path1 = get_partition_path(&base_dir, "logs", Some("2023-10-25"));
        assert_eq!(
            path1,
            PathBuf::from("/data/indexes/logs/segments/2023-10-25")
        );

        let path2 = get_partition_path(&base_dir, "metrics", None);
        assert_eq!(
            path2,
            PathBuf::from("/data/indexes/metrics/segments/default")
        );
    }
}
