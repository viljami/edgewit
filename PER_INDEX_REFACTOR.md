# PER-INDEX & PARTITION REFACTOR PLAN

Moving from a single monolithic Tantivy index to physically separated indexes per-index and per-partition.

## Goal

Implement a true multi-index engine that aligns with the YAML schema definitions and partition strategies we built in Phases 1-10. Currently, all documents are dumped into a single index directory. This refactor will route documents into specific physical directories based on their definition and timestamp.

## Step 1: The Index Manager (Writer & Reader Pool)

- Create an `IndexManager` struct to hold dynamic `Index`, `IndexWriter`, and `IndexReader` instances keyed by `(String, String)` -> `(IndexName, PartitionName)`.
- When a document arrives, use `src/partition.rs` to compute its deterministic folder name.
- Open or create the Tantivy index for that exact folder, using the specific Tantivy Schema compiled from `schema::builder::build_schema` (rather than the old generic catch-all schema).
- Manage a global memory budget across these writers (e.g., dynamically adjust memory per writer or limit total open writers).

## Step 2: Ingestion & WAL Replay Routing

- Modify `indexer::IndexerActor` to use the new `IndexManager` instead of a single `IndexWriter`.
- When handling `add_document`, route it to the specific index and partition writer.
- Ensure periodic background commits are distributed to all currently "dirty" writers.
- Update `main.rs` crash recovery (WAL replay) to do the same routing on startup.

## Step 3: Single-Index Distributed Search (Cross-Partition Scatter-Gather)

- **Remove Global Search**: Delete the global `/_search` endpoint entirely to strictly enforce single-index querying. Cross-index searching is not supported.
- Rewrite the `GET /indexes/<name>/_search` endpoint in `api/search.rs`.
- Because a single logical index (like `logs`) is now physically split into multiple time-based partition directories, a query to `/indexes/logs/_search` must ask the `IndexManager` to identify all valid partition directories for that specific index.
- **Scatter**: Run the Tantivy search against all of those specific partition readers simultaneously.
- **Gather**: Merge the `TopDocs` (sort by score or time) across the multiple partition results into a single unified response.
- Merge Aggregations (using `tantivy::aggregation` tree merging mechanisms) across the partitions.
- Retrieve the final `_source` JSONs from the respective partition stores.

## Step 4: System Integration & Cleanup

- Fix Observability endpoints (`_cat/indexes`) to sum up doc counts and disk sizes from the individual partition managers.
- Remove the old monolithic index code (`setup_index` in `indexer.rs`).
- Update all integration and unit tests for the new scatter-gather routing logic.
