# DeciSym.AI DEF CON 33 Recon Village Talk Supporting Materials

This project includes tools and scripts demonstrating the techniques
described in the DEF CON 33 Recon Village Talk presented by Donald
Anthony Pellegrino Jr., Ph.D., in Las Vegas, NV, USA, August, 2025.

## Quick Start - Run the Complete Case Study

To reproduce the complete case study from the presentation:

```bash
# Build and run the DEF CON 33 case study example
cargo run --example defcon_case_study
```

This will walk through all five steps of the OSINT workflow:
1. Collect Recon Village speaker data through Tor
2. Extract speaker information using LLM
3. Download security company data from Wikidata
4. Generate RDF knowledge graph (FOAF format)
5. Analyze relationships between speakers and companies

The example handles missing dependencies gracefully and uses
precomputed data where available, so you can explore the workflow even
without all components running.

## OPSEC Risks and Challenges

### Confidentiality

Collecting information on specific targets can reveal the focus of an
investigation to the data providers. This happens when conditions are
added to crawlers, API calls, or database queries that limit or filter
for only the relevant data.

Instead, data can be bulk downloaded with coverage that includes the
focus of the investigation along with other information to prevent
revealing the focus.

### Integrity

Websites designed exclusively for human visual processing may use
iframes and JavaScript to create content dynamically on the client
browser. These cannot be crawled accurately by tools that simply
download resources. The iframes need to be populated and the
JavaScript executed. Therefore, techniques that automate full web
browsers are needed to obtain content for computational processing
that has the same information as the version of the page rendered for
human visual processing.

### Availability

Marshaling evidence into a local environment with LLMs and databases
brings the core tooling under the control of the Investigator. This
allows for exploratory analysis and hypothesis generation to execute
without the limitations of external providers.

### Challenge - Collection

Collection can also reveal the identity of the investigator, the
investigator's organization, or the toolset used. This can happen when
identifying information, such as the investigation system's IP address
is available, or the tools set User-Agent strings in HTTP requests.

One method of collection is to run Tor to anonymize
downloads. Proxychains can be used with HTTrack or wget to download a
website. Another approach is to write a custom Rust program using
Arti.

### Challenge - Large Language Models (LLMs)

LLMs hosted by others exhibit the same risks as source
collection. However, it is much more difficult to use the LLMs
effectively without revealing the focus of the investigation. At some
point, it will be useful to mention the identity of the target in LLM
prompts. It will also be useful to focus the LLM context window to
only relevant source materials.

## Web Service Downloads

The collect command supports downloading from web services and APIs through Tor:

```bash
# Download from a web service with POST request
cargo run --release -- collect https://api.example.com/data \
  -X POST \
  -H "Accept: application/json" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "query=SELECT * FROM data" \
  -o results.json
```

## Wikidata Integration

The project includes Rust integration tests for downloading security
company data from Wikidata:

```bash
# Run the Wikidata download test (requires Tor)
cargo test --test wikidata_download -- --ignored

# The test downloads security companies and converts to RDF format
# Data includes companies in industries like:
#   - Computer security & Cybersecurity
#   - Cloud computing & Cryptocurrency
#   - Information technology
```

**Note on RDF Generation**: The implementation uses SELECT queries
with CSV output followed by local RDF conversion, rather than
CONSTRUCT queries. This is intentional - Wikidata's CONSTRUCT queries
are significantly slower and often timeout, making the CSV→RDF
approach more reliable and performant.

## LLM Inference with vLLM

### Start the vLLM Server

The project uses vLLM with the Qwen model for LLM inference. Start the server first:

```bash
# Using the provided script (AMD ROCm GPUs)
./examples/vllm_server.sh

# Or manually with Docker (NVIDIA GPUs)
docker run --rm --gpus all -p 8000:8000 \
  vllm/vllm-openai:latest \
  --model Qwen/Qwen3-30B-A3B-Instruct-2507 \
  --max-model-len 262144 \
  --disable-log-requests
```

### Run Inference

Once the vLLM server is running, use the enrich command with configuration files:

```bash
# Extract speakers from HTML content
cargo run -- enrich \
  -c examples/extract_speakers.yaml \
  -i data/recon_village.html \
  -o speakers.json

# Chat-based interaction
cargo run -- enrich \
  -c examples/chat.yaml

# Simple completion
cargo run -- enrich \
  -c examples/completion.yaml
```

## Configuration Format

The enrich command uses YAML or JSON configuration files. Example
`extract_speakers.yaml`:

```yaml
api_url: "http://localhost:8000/v1"
model: "Qwen/Qwen3-30B-A3B-Instruct-2507"
messages:
  - role: "system"
    content: "You are an expert at extracting structured data from HTML."
  - role: "user"
    content: "Extract all speaker names from the conference HTML."
max_tokens: 2048
temperature: 0.1
```

See the `examples/` directory for more configuration examples:
- `chat.yaml`: Interactive chat format
- `completion.yaml`: Simple completion format
- `extract_speakers.yaml`: Speaker extraction from HTML

### vLLM Server Setup

Start the vLLM server to provide LLM inference:

```bash
# Using the provided script
./examples/vllm_server.sh

# Or manually with Docker
docker run --rm --gpus all \
  -p 8000:8000 \
  vllm/vllm-openai:latest \
  --model Qwen/Qwen3-30B-A3B-Instruct-2507 \
  --max-model-len 262144 \
  --disable-log-requests
```

The server runs on `http://localhost:8000/v1` and provides an
OpenAI-compatible API endpoint.

## Circuit Isolation Strategy

The tool uses a single Tor circuit per execution for optimal
performance:

### Current Implementation
- Creates one Tor circuit when the tool starts
- Reuses this circuit for all connections during the download
- Provides good privacy while maintaining performance
- Perfect for single-page downloads with multiple resources

This approach balances privacy and performance effectively since we
only download one webpage at a time. All connections (main page,
iframes, resources) share the same circuit, preventing correlation
with other browsing sessions while avoiding the overhead of creating
new circuits for every resource.

## Troubleshooting

### Viewing Debug Logs

The tool provides detailed logging to help diagnose issues:

```bash
# Default (info level logs)
cargo run -- collect https://example.com

# Verbose mode (debug level)
cargo run -- --verbose collect https://example.com

# Save logs to file
cargo run -- collect https://example.com 2>debug.log
```

### Tor Connection Issues

If you encounter Tor connection errors:

1. **"Network is unreachable (os error 101)"**
   - Check your internet connection
   - Ensure no firewall is blocking outbound connections
   - Try running with `--verbose` to see more details

2. **"Failed to obtain exit circuit"**
   - This is normal during Tor startup
   - Wait 10-30 seconds for Tor to establish circuits
   - The tool will automatically retry connections

3. **"Could not connect to guard"**
   - Tor is having trouble connecting to entry nodes
   - This often resolves itself after a few attempts
   - May indicate network restrictions or censorship

### Tor Directory Warnings

If you see warnings like "response too long" from tor_dirmgr, these
are normal and can be safely ignored. These occur when Tor directory
documents exceed internal protocol limits. The tool automatically
suppresses these warnings unless running in verbose mode.

## Alternatives

- **Spiderfoot** An open source intelligence (OSINT) automation tool.

- **HTTrack** A website copier. https://www.httrack.com/

## References

- **Tor** https://www.torproject.org/

- **Proxychains**

- gabi, “Arti 1.4.5 is released: Continued work on xon-based flow
  control, Conflux.,” Tor Blog. Accessed: July
  19, 2025. [Online]. Available:
  https://blog.torproject.org/arti_1_4_5_released/

- **Arti** Tor in Rust, https://gitlab.torproject.org/tpo/core/arti

- **Arti-client** Rust Crate, https://crates.io/crates/arti-client

<!--  LocalWords:  defcon iframes Proxychains HTTrack wget Arti json
<!--  LocalWords:  www urlencoded wikidata vllm yaml Qwen docker rocm
<!--  LocalWords:  nvidia cuda gpus localhost api config speakers html
<!--  LocalWords:  os dirmgr Spiderfoot gabi xon Conflux enrich foaf
 -->
