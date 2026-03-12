use edgewit::api::ApiDoc;
use std::fs;
use utoipa::OpenApi;

fn main() {
    println!("Generating OpenAPI documentation for Edgewit...");

    let doc = ApiDoc::openapi()
        .to_pretty_json()
        .expect("Failed to serialize OpenAPI spec");

    // Ensure the docs directory exists
    fs::create_dir_all("docs").expect("Failed to create docs directory");

    // Write the openapi.json file
    fs::write("docs/openapi.json", doc).expect("Failed to write openapi.json");

    println!("Successfully wrote docs/openapi.json");
}
