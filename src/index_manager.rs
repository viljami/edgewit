use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tantivy::{Index, IndexReader, ReloadPolicy};

use crate::partition::get_partition_path;
use crate::registry::IndexRegistry;
use crate::schema::builder::build_schema;

#[derive(Clone)]
pub struct IndexManager {
    base_dir: PathBuf,
    registry: IndexRegistry,
    indexes: Arc<RwLock<HashMap<(String, String), Index>>>,
    readers: Arc<RwLock<HashMap<(String, String), IndexReader>>>,
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

    pub fn get_or_create_index(
        &self,
        index_name: &str,
        partition_name: &str,
    ) -> Result<Index, String> {
        let key = (index_name.to_string(), partition_name.to_string());

        {
            let indexes = self.indexes.read().unwrap();
            if let Some(index) = indexes.get(&key) {
                return Ok(index.clone());
            }
        }

        let mut indexes = self.indexes.write().unwrap();
        if let Some(index) = indexes.get(&key) {
            return Ok(index.clone());
        }

        let def = self
            .registry
            .get(index_name)
            .ok_or_else(|| format!("Index definition not found for {}", index_name))?;

        let schema = build_schema(&def).map_err(|e| e.to_string())?;

        // Use the explicit partition path logic
        let partition_path = get_partition_path(
            &self.base_dir,
            index_name,
            if partition_name == "default" {
                None
            } else {
                Some(partition_name)
            },
        );

        std::fs::create_dir_all(&partition_path).map_err(|e| e.to_string())?;

        let dir =
            tantivy::directory::MmapDirectory::open(&partition_path).map_err(|e| e.to_string())?;

        let index = Index::open_or_create(dir, schema).map_err(|e| e.to_string())?;

        indexes.insert(key.clone(), index.clone());
        Ok(index)
    }

    pub fn get_reader(
        &self,
        index_name: &str,
        partition_name: &str,
    ) -> Result<IndexReader, String> {
        let key = (index_name.to_string(), partition_name.to_string());

        {
            let readers = self.readers.read().unwrap();
            if let Some(reader) = readers.get(&key) {
                return Ok(reader.clone());
            }
        }

        let index = self.get_or_create_index(index_name, partition_name)?;

        let mut readers = self.readers.write().unwrap();
        if let Some(reader) = readers.get(&key) {
            return Ok(reader.clone());
        }

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .doc_store_cache_num_blocks(self.docstore_cache_blocks)
            .try_into()
            .map_err(|e| e.to_string())?;

        readers.insert(key, reader.clone());
        Ok(reader)
    }

    pub fn get_all_readers(&self, index_name: &str) -> Result<Vec<IndexReader>, String> {
        let mut result = Vec::new();
        let segments_dir = self
            .base_dir
            .join("indexes")
            .join(index_name)
            .join("segments");

        if !segments_dir.exists() {
            return Ok(result);
        }

        let entries = std::fs::read_dir(segments_dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
                && let Some(partition_name) = entry.file_name().to_str()
                && let Ok(reader) = self.get_reader(index_name, partition_name)
            {
                result.push(reader);
            }
        }

        Ok(result)
    }
}
