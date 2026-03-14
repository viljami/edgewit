use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::schema::definition::IndexDefinition;
use crate::schema::validation::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("index '{0}' already exists")]
    AlreadyExists(String),
    #[error("index '{0}' not found")]
    NotFound(String),
    #[error("validation error: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("registry lock poisoned")]
    LockError,
}

/// Central index management for Edgewit.
/// Thread-safe cloneable registry meant to be shared across the application.
#[derive(Clone, Default)]
pub struct IndexRegistry {
    indexes: Arc<RwLock<HashMap<String, IndexDefinition>>>,
}

impl IndexRegistry {
    /// Create a new empty IndexRegistry.
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new index definition.
    /// Returns an error if an index with the same name already exists.
    pub fn register(&self, definition: IndexDefinition) -> Result<(), RegistryError> {
        definition.validate()?;

        let mut indexes = self.indexes.write().map_err(|e| {
            tracing::error!("Index registry lock poisoned: {}", e);
            RegistryError::LockError
        })?;
        if indexes.contains_key(&definition.name) {
            return Err(RegistryError::AlreadyExists(definition.name));
        }

        indexes.insert(definition.name.clone(), definition);
        Ok(())
    }

    /// Update an existing index definition or insert it if it doesn't exist.
    pub fn upsert(&self, definition: IndexDefinition) -> Result<(), RegistryError> {
        definition.validate()?;

        let mut indexes = self.indexes.write().map_err(|e| {
            tracing::error!("Index registry lock poisoned: {}", e);
            RegistryError::LockError
        })?;
        indexes.insert(definition.name.clone(), definition);
        Ok(())
    }

    /// Retrieve an index definition by name.
    pub fn get(&self, name: &str) -> Option<IndexDefinition> {
        match self.indexes.read() {
            Ok(indexes) => indexes.get(name).cloned(),
            Err(e) => {
                tracing::error!("Index registry lock poisoned: {}", e);
                None
            }
        }
    }

    /// List all registered index definitions.
    pub fn list(&self) -> Vec<IndexDefinition> {
        match self.indexes.read() {
            Ok(indexes) => indexes.values().cloned().collect(),
            Err(e) => {
                tracing::error!("Index registry lock poisoned: {}", e);
                Vec::new()
            }
        }
    }

    /// Remove an index definition from the registry.
    pub fn remove(&self, name: &str) -> Result<IndexDefinition, RegistryError> {
        let mut indexes = self.indexes.write().map_err(|e| {
            tracing::error!("Index registry lock poisoned: {}", e);
            RegistryError::LockError
        })?;
        indexes
            .remove(name)
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))
    }

    /// Load all index definitions from `.index.yaml` files in a given directory.
    pub fn load_from_dir<P: AsRef<Path>>(&self, dir_path: P) -> Result<usize, RegistryError> {
        let mut count = 0;
        let path = dir_path.as_ref();

        if !path.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file()
                && let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && (file_name.ends_with(".index.yaml") || file_name.ends_with(".index.yml"))
            {
                let def = IndexDefinition::from_file(&path)?;
                match self.register(def) {
                    Ok(_) => {
                        tracing::info!("Loaded index definition: {}", file_name);
                        count += 1;
                    }
                    Err(RegistryError::AlreadyExists(name)) => {
                        tracing::warn!(
                            "Index '{}' already registered, skipping {}",
                            name,
                            file_name
                        );
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{
        CompressionOption, FieldDefinition, FieldType, PartitionStrategy, SchemaMode,
    };
    use tempfile::tempdir;

    fn create_test_definition(name: &str) -> IndexDefinition {
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

        IndexDefinition {
            name: name.to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode: SchemaMode::Dynamic,
            partition: PartitionStrategy::Daily,
            retention: Some("7d".to_string()),
            compression: CompressionOption::Zstd,
            fields,
            settings: HashMap::new(),
        }
    }

    #[test]
    fn test_registry_crud() {
        let registry = IndexRegistry::new();
        let def1 = create_test_definition("logs");
        let def2 = create_test_definition("metrics");

        // Register
        assert!(registry.register(def1.clone()).is_ok());
        assert!(registry.register(def2.clone()).is_ok());

        // Duplicate registration should fail
        assert!(matches!(
            registry.register(def1.clone()),
            Err(RegistryError::AlreadyExists(_))
        ));

        // Get
        let retrieved = registry.get("logs").unwrap();
        assert_eq!(retrieved.name, "logs");
        assert!(registry.get("nonexistent").is_none());

        // List
        let list = registry.list();
        assert_eq!(list.len(), 2);

        // Remove
        let removed = registry.remove("logs").unwrap();
        assert_eq!(removed.name, "logs");
        assert!(registry.get("logs").is_none());

        // Remove nonexistent
        assert!(matches!(
            registry.remove("logs"),
            Err(RegistryError::NotFound(_))
        ));
    }

    #[test]
    fn test_load_from_dir() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.index.yaml");

        let yaml_content = r#"
name: test_index
timestamp_field: timestamp
partition: daily
retention: 7d
fields:
  timestamp:
    type: datetime
"#;
        fs::write(file_path, yaml_content).unwrap();

        let registry = IndexRegistry::new();
        let loaded_count = registry.load_from_dir(dir.path()).unwrap();

        assert_eq!(loaded_count, 1);
        let def = registry.get("test_index").unwrap();
        assert_eq!(def.name, "test_index");
    }
}
