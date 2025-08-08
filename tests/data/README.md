# Test Data for Speaker Extraction

This directory contains test data for the speaker extraction integration test.

## Files

- `recon_village_defcon33.html`: Full Recon Village HTML file containing DEFCON 33 data
  - This is the source HTML file for testing
  - Contains iframe content with speaker data near the end of the file
  - Size: ~2.1MB

- `speaker_section.html`: Extract from the Recon Village HTML containing the DEFCON 33 speaker list
  - Source: `recon_village_defcon33.html` (bytes 2021197-2221197)
  - Contains: Speaker list in iframe content with sessionize.com data
  - Size: ~180KB
  - This is checked into version control as reference data


## Test Data Details

The `speaker_section.html` file contains the speaker data extracted from approximately byte position 2,021,197 to 2,221,197 of the source HTML. This section includes the iframe content with sessionize.com data that contains the complete speaker list.

The test data is verified to contain the expected speaker "Donald Pellegrino" which serves as a key validation point.

## Running the Test

1. Ensure vLLM server is running with Qwen3 model:
   ```
   Model: Qwen/Qwen3-30B-A3B-Instruct-2507
   Endpoint: http://localhost:8000/v1
   ```

2. Run the integration test:
   ```bash
   cargo test speaker_extraction -- --ignored --nocapture
   ```

The test will:
- Load the HTML speaker section
- Send it to the vLLM server for extraction
- Compare results with the reference list of 28 speakers (defined in tests/speaker_extraction.rs)
- Report accuracy and any missing/extra speakers