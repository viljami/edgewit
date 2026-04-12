use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tantivy::{Index, IndexReader, ReloadPolicy};
use tracing::error;

use crate::registry::IndexRegistry;
use crate::schema::builder::build_schema;

/// Manages Tantivy [`Index`] and [`IndexReader`] instances.
///
/// One Tantivy index lives at `<base_dir>/indexes/<index_name>/`.
/// A single reader covers the entire index — no per-partition split.
#[derive(Clone)]
pub struct IndexManager {
    base_dir: PathBuf,
    registry: IndexRegistry,
    indexes: Arc<RwLock<HashMap<String, Index>>>,
    readers: Arc<RwLock<HashMap<String, IndexReader>>>,
    docstore_cache_blocks: usize,
}

impl IndexManager {
    pub fn new(base_dir: PathBuf, registry: IndexRegistry, docstore_cache_blocks: usize) -> Self {
        Self {
            base_dir,
            registry,
            indexes: Arc::new(RwLock::new(HashMap::new())),
            readers: Arc::new(RwLock::new(HashMap::new())),
            docstore_cache_blocks,
        }
    }

    /// Returns the on-disk path for a given index.
    pub fn index_path(&self, index_name: &str) -> PathBuf {
        self.base_dir.join("indexes").join(index_name)
    }

    /// Returns (or lazily opens/creates) the [`Index`] for `index_name`.
    pub fn get_or_create_index(&self, index_name: &str) -> Result<Index, String> {
        // Fast path: already cached
        {
            let indexes = self.indexes.read().unwrap();
            if let Some(idx) = indexes.get(index_name) {
                return Ok(idx.clone());
            }
        }

        // Slow path: open or create on disk
        let mut indexes = self.indexes.write().unwrap();
        if let Some(idx) = indexes.get(index_name) {
            return Ok(idx.clone()); // another thread may have beaten us here
        }

        let def = self
            .registry
            .get(index_name)
            .ok_or_else(|| format!("Index definition not found: '{index_name}'"))?;

        let schema = build_schema(&def).map_err(|e| e.to_string())?;
        let path = self.index_path(index_name);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;

        let dir = tantivy::directory::MmapDirectory::open(&path).map_err(|e| e.to_string())?;
        let index = Index::open_or_create(dir, schema).map_err(|e| e.to_string())?;

        indexes.insert(index_name.to_string(), index.clone());
        Ok(index)
    }

    /// Returns (or lazily creates) an [`IndexReader`] for `index_name`.
    pub fn get_reader(&self, index_name: &str) -> Result<IndexReader, String> {
        // Fast path
        {
            let readers = self.readers.read().unwrap();
            if let Some(r) = readers.get(index_name) {
                return Ok(r.clone());
            }
        }

        let index = self.get_or_create_index(index_name)?;

        let mut readers = self.readers.write().unwrap();
        if let Some(r) = readers.get(index_name) {
            return Ok(r.clone());
        }

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .doc_store_cache_num_blocks(self.docstore_cache_blocks)
            .try_into()
            .map_err(|e: tantivy::TantivyError| {
                error!("Failed to build reader for '{index_name}': {e}");
                e.to_string()
            })?;

        readers.insert(index_name.to_string(), reader.clone());
        Ok(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::definition::{
        CompressionOption, FieldDefinition, FieldType, IndexDefinition, PartitionStrategy,
        SchemaMode,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn test_definition(name: &str) -> IndexDefinition {
        let mut fields = HashMap::new();
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
        IndexDefinition {
            name: name.to_string(),
            description: None,
            timestamp_field: "timestamp".to_string(),
            mode: SchemaMode::Dynamic,
            partition: PartitionStrategy::None,
            retention: None,
            compression: CompressionOption::Zstd,
            fields,
            settings: HashMap::new(),
        }
    }

    #[test]
    fn test_get_or_create_index() {
        let dir = TempDir::new().unwrap();
        let registry = IndexRegistry::new();
        registry.register(test_definition("test")).unwrap();
        let manager = IndexManager::new(dir.path().to_path_buf(), registry, 10);

        assert!(
            manager.get_or_create_index("test").is_ok(),
            "first call creates index"
        );
        assert!(
            manager.get_or_create_index("test").is_ok(),
            "second call returns cached"
        );
    }

    #[test]
    fn test_get_or_create_index_unknown() {
        let dir = TempDir::new().unwrap();
        let manager = IndexManager::new(dir.path().to_path_buf(), IndexRegistry::new(), 10);
        assert!(manager.get_or_create_index("ghost").is_err());
    }

    #[test]
    fn test_get_reader() {
        let dir = TempDir::new().unwrap();
        let registry = IndexRegistry::new();
        registry.register(test_definition("logs")).unwrap();
        let manager = IndexManager::new(dir.path().to_path_buf(), registry, 10);
        assert!(manager.get_reader("logs").is_ok());
    }

    #[test]
    fn test_index_path() {
        let manager = IndexManager::new(PathBuf::from("/data"), IndexRegistry::new(), 10);
        assert_eq!(
            manager.index_path("my_index"),
            PathBuf::from("/data/indexes/my_index")
        );
    }
}
