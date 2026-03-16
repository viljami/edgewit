use tantivy::{Index, doc, schema::*};
use tempfile::TempDir;

#[test]
fn test_manual_reload_policy() {
    let temp_dir = TempDir::new().unwrap();
    let mut schema_builder = Schema::builder();
    let text_field = schema_builder.add_text_field("text", TEXT);
    let schema = schema_builder.build();

    let index = Index::create_in_dir(temp_dir.path(), schema).unwrap();

    // Validate we build readers with Manual reload policy to avoid inotify watcher limits
    let reader = index
        .reader_builder()
        .reload_policy(tantivy::ReloadPolicy::Manual)
        .try_into()
        .unwrap();

    assert_eq!(reader.searcher().num_docs(), 0);

    let mut writer = index.writer(15_000_000).unwrap();
    writer.add_document(doc!(text_field => "hello")).unwrap();
    let commit = writer.prepare_commit().unwrap();
    commit.commit().unwrap();

    // Without an explicit reload, the manual reader should NOT see the new documents
    assert_eq!(reader.searcher().num_docs(), 0);

    // After explicit reload, the searcher updates correctly
    reader.reload().unwrap();
    assert_eq!(reader.searcher().num_docs(), 1);

    temp_dir.close().unwrap();
}
