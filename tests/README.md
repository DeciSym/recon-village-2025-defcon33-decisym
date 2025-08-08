# Test Suite

This directory contains tests for the `decisym_defcon33` project.

## Test Files

### `unit_tests.rs`
Unit tests for core functionality including configuration parsing and validation. No external dependencies required.

### `wikidata_download.rs`
Integration test for downloading and converting Wikidata SPARQL results to RDF format. Tests both the CSV to RDF conversion logic and the full download workflow through Tor.

## Test Data

The `data/` directory contains test fixtures:
- `recon_village_defcon33.html` - Sample HTML from Recon Village website
- `speaker_section.html` - Extracted speaker section for testing
- `foaf.rdf` - Example FOAF RDF output

## Running Tests

```bash
# Run all tests
cargo test

# Run only unit tests (fast, no network)
cargo test --test unit_tests

# Run Wikidata tests (requires network/Tor)
cargo test --test wikidata_download -- --ignored

# Run with output
cargo test -- --nocapture
```

## Case Study Example

For the complete DEF CON 33 case study workflow, see:
```bash
cargo run --example defcon_case_study
```

This example in `examples/defcon_case_study.rs` demonstrates the full OSINT workflow presented at the conference.