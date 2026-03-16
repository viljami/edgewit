use tantivy::query::QueryParser;
use tantivy::{Index, doc, schema::*};

#[test]
fn test_parser_star() {
    let mut schema_builder = Schema::builder();
    let text_field = schema_builder.add_text_field("text", TEXT);
    let schema = schema_builder.build();
    let index = Index::create_in_ram(schema.clone());
    let parser = QueryParser::for_index(&index, vec![text_field]);
    let query = parser.parse_query("*");
    println!("{:?}", query);
}
