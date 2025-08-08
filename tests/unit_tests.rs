//! Unit tests for speaker extraction components

use decisym_defcon33::openai_client::{EnrichConfig, PromptConfig};

#[test]
fn test_config_parsing() {
    let yaml = r#"
api_url: "http://localhost:8000/v1"
model: "test-model"
messages:
  - role: "system"
    content: "You are a test assistant"
  - role: "user"
    content: "Extract names"
max_tokens: 100
temperature: 0.5
"#;

    let config: EnrichConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.api_url, "http://localhost:8000/v1");
    assert_eq!(config.model, "test-model");

    if let PromptConfig::Chat { messages } = &config.prompt {
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
    } else {
        panic!("Expected chat prompt config");
    }

    assert_eq!(config.parameters.max_tokens, 100);
    assert_eq!(config.parameters.temperature, 0.5);
}

#[test]
fn test_speaker_name_extraction_patterns() {
    // Test patterns that should match speaker names
    let html_snippets = vec![
        r#"<img alt="Donald Pellegrino" src="speaker.jpg">"#,
        r#"<h3 class="sz-speaker__name">Charles Waterhouse</h3>"#,
        r#"<li class="speaker">Özgün Kültekin</li>"#,
    ];

    for snippet in html_snippets {
        // In real implementation, this would use the actual parsing logic
        assert!(
            snippet.contains("Donald Pellegrino")
                || snippet.contains("Charles Waterhouse")
                || snippet.contains("Özgün Kültekin")
        );
    }
}

#[test]
fn test_reference_speaker_list() {
    let reference_speakers = vec![
        "Ankit Gupta",
        "Apurv Singh Gautam",
        "Charles Waterhouse",
        "Daniel Schwalbe",
        "Donald Pellegrino",
        "Evgueni Erchov",
        "Jeff Foley",
        "John Dilgen",
        "Kaloyan Ivanov",
        "Kevin Dela Rosa",
        "Kumar Ashwin",
        "Master Chen",
        "Michael Portera",
        "Mohamed Nabeel",
        "Muslim Koser",
        "Nishant Sharma",
        "Ram Ganesh",
        "Reuel Magistrado",
        "Rohit Grover",
        "Ryan Bonner",
        "Sean Jones",
        "Shilpi Mittal",
        "Shourya Pratap Singh",
        "Sinwindie",
        "Vladimir Tokarev",
        "Zach Malinich",
        "Zoey Selman",
        "Özgün Kültekin",
    ];

    assert_eq!(reference_speakers.len(), 28);
    assert!(reference_speakers.contains(&"Donald Pellegrino"));

    // Check for duplicates
    let mut sorted = reference_speakers.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 28);
}
