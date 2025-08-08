# Enrich Command Examples

The `enrich` command now uses configuration files to specify all parameters for interacting with OpenAI-compatible APIs.

## Basic Usage

```bash
# Using a YAML config file
./target/release/decisym_defcon33 enrich -c examples/chat.yaml

# Using a JSON config file
./target/release/decisym_defcon33 enrich -c examples/completion.json

# Process a specific file
./target/release/decisym_defcon33 enrich -c examples/extract_speakers.yaml -i data/pretty.html -o speakers.json
```

## Configuration Format

Configuration files can be in YAML or JSON format and support:

### API Configuration
- `api_url`: The base URL for the OpenAI-compatible API (e.g., `http://localhost:8000/v1`)
- `api_key`: Optional API key for authentication
- `model`: The model name to use

### Prompt Configuration
Choose one of two formats:

1. **Completion format** (simple prompt):
   ```yaml
   prompt: "Your prompt here"
   ```

2. **Chat format** (with roles):
   ```yaml
   messages:
     - role: "system"
       content: "System prompt"
     - role: "user"
       content: "User message"
   ```

### Generation Parameters
- `max_tokens`: Maximum tokens to generate (default: 1024)
- `temperature`: Sampling temperature 0.0-1.0 (default: 0.7)
- `top_p`: Top-p sampling parameter
- `seed`: Random seed for reproducibility
- `stop`: Array of stop sequences
- `n`: Number of completions to generate

### Other Settings
- `timeout_seconds`: Request timeout (default: 300)

## Example Configurations

- `completion.yaml` / `completion.json`: Basic text completion
- `chat.yaml`: Chat format with system message
- `extract_speakers.yaml`: Optimized for extracting speaker names from HTML

## Working with vLLM

Start a vLLM server:
```bash
docker run --rm \
  --gpus all \
  -p 8000:8000 \
  vllm/vllm-openai:latest \
  --model ibm-granite/granite-3.3-8b-instruct
```

Then use any of the example configs to interact with it.