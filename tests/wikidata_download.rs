use anyhow::{Context, Result};
use decisym_defcon33::TorDownloader;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

/// Structure representing a Wikidata SPARQL response
#[derive(Debug, Deserialize)]
struct SparqlResponse {
    results: SparqlResults,
}

#[derive(Debug, Deserialize)]
struct SparqlResults {
    bindings: Vec<SparqlBinding>,
}

#[derive(Debug, Deserialize)]
struct SparqlBinding {
    #[serde(default)]
    count: Option<SparqlValue>,
    #[serde(default)]
    company: Option<SparqlValue>,
    #[serde(rename = "companyName", default)]
    company_name: Option<SparqlValue>,
    #[serde(default)]
    industry: Option<SparqlValue>,
    #[serde(default)]
    inception: Option<SparqlValue>,
    #[serde(default)]
    owns: Option<SparqlValue>,
    #[serde(rename = "ownsName", default)]
    owns_name: Option<SparqlValue>,
    #[serde(rename = "ownedBy", default)]
    owned_by: Option<SparqlValue>,
    #[serde(rename = "ownedByName", default)]
    owned_by_name: Option<SparqlValue>,
}

#[derive(Debug, Deserialize)]
struct SparqlValue {
    value: String,
    #[serde(rename = "type")]
    value_type: String,
}

/// Company data for RDF generation
#[derive(Debug, Default)]
struct CompanyData {
    label: String,
    industry: Option<String>,
    inception: Option<String>,
    owns: Vec<(String, String)>,
    owned_by: Vec<(String, String)>,
}

/// Downloads security companies from Wikidata through Tor
pub struct WikidataDownloader {
    downloader: TorDownloader,
    data_dir: PathBuf,
}

impl WikidataDownloader {
    /// Creates a new WikidataDownloader
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        // Ensure data directory exists
        fs::create_dir_all(&data_dir)?;

        let downloader = TorDownloader::new()
            .await
            .context("Failed to initialize Tor downloader")?;

        Ok(Self {
            downloader,
            data_dir,
        })
    }

    /// Get count query SPARQL
    fn get_count_query() -> &'static str {
        r#"SELECT (COUNT(DISTINCT ?company) as ?count)
WHERE {
  VALUES ?type { wd:Q891723 wd:Q4830453 wd:Q163740 }
  VALUES ?industry { wd:Q3510521 wd:Q21157865 wd:Q880371 wd:Q638608 wd:Q484847 wd:Q97466080 wd:Q11451 }
  ?company wdt:P31/wdt:P279* ?type ;
           wdt:P452 ?industry ;
           rdfs:label ?companyName .
  FILTER(LANG(?companyName) = "en")
}"#
    }

    /// Get main query SPARQL
    fn get_main_query() -> &'static str {
        r#"SELECT DISTINCT ?company ?companyName ?industry ?inception ?owns ?ownsName ?ownedBy ?ownedByName
WHERE {
  VALUES ?type { wd:Q891723 wd:Q4830453 wd:Q163740 }
  VALUES ?industry { wd:Q3510521 wd:Q21157865 wd:Q880371 wd:Q638608 wd:Q484847 wd:Q97466080 wd:Q11451 }
  ?company wdt:P31/wdt:P279* ?type ;
           wdt:P452 ?industry ;
           rdfs:label ?companyName .
  FILTER(LANG(?companyName) = "en")
  
  OPTIONAL { ?company wdt:P571 ?inception }
  
  OPTIONAL { 
    ?company wdt:P1830 ?owns .
    OPTIONAL {
      ?owns rdfs:label ?ownsName .
      FILTER(LANG(?ownsName) = "en")
    }
  }
  
  OPTIONAL { 
    ?company wdt:P127 ?ownedBy .
    OPTIONAL {
      ?ownedBy rdfs:label ?ownedByName .
      FILTER(LANG(?ownedByName) = "en")
    }
  }
}
ORDER BY ?companyName"#
    }

    /// Execute a SPARQL query and return JSON response
    async fn execute_sparql_query(&mut self, query: &str, accept: &str) -> Result<Vec<u8>> {
        let url = "https://query.wikidata.org/sparql";

        // URL encode the query
        let encoded_query = urlencoding::encode(query);
        let body = format!("query={}", encoded_query);

        // Headers for SPARQL endpoint
        let headers = vec![
            format!("Accept: {}", accept),
            "User-Agent: OSINT-Research-Bot/1.0".to_string(),
            "Content-Type: application/x-www-form-urlencoded".to_string(),
        ];

        println!("Executing SPARQL query through Tor...");
        let (response, _) = self
            .downloader
            .download_web_service(url, "POST", &headers, Some(&body))
            .await
            .context("Failed to execute SPARQL query")?;

        Ok(response)
    }

    /// Get the count of companies
    pub async fn get_company_count(&mut self) -> Result<usize> {
        let query = Self::get_count_query();
        let response = self
            .execute_sparql_query(query, "application/sparql-results+json")
            .await?;

        let json_response: SparqlResponse =
            serde_json::from_slice(&response).context("Failed to parse count response")?;

        if let Some(binding) = json_response.results.bindings.first() {
            if let Some(count) = &binding.count {
                return count.value.parse().context("Failed to parse count value");
            }
        }

        anyhow::bail!("No count found in response");
    }

    /// Download companies data as CSV
    pub async fn download_companies_csv(&mut self) -> Result<PathBuf> {
        let query = Self::get_main_query();
        let response = self.execute_sparql_query(query, "text/csv").await?;

        let csv_path = self.data_dir.join("security_companies.csv");
        fs::write(&csv_path, response).context("Failed to write CSV file")?;

        println!(
            "Downloaded {} bytes to {}",
            fs::metadata(&csv_path)?.len(),
            csv_path.display()
        );

        Ok(csv_path)
    }

    /// Convert CSV to RDF Turtle format
    ///
    /// IMPORTANT: This CSV to RDF conversion is a necessary workaround for Wikidata's
    /// SPARQL endpoint limitations. While Wikidata technically supports CONSTRUCT queries
    /// that can return RDF directly, in practice:
    ///
    /// 1. CONSTRUCT queries are significantly slower than SELECT queries
    /// 2. CONSTRUCT queries often timeout for larger result sets
    /// 3. SELECT queries with CSV output are much more performant
    ///
    /// Therefore, we use SELECT → CSV → RDF transformation as a pragmatic solution
    /// that provides better performance and reliability when working with Wikidata.
    pub fn csv_to_rdf(csv_path: &PathBuf) -> Result<String> {
        let csv_content = fs::read_to_string(csv_path).context("Failed to read CSV file")?;

        let mut rdf = String::new();

        // Add RDF prefixes
        rdf.push_str("@prefix wd: <http://www.wikidata.org/entity/> .\n");
        rdf.push_str("@prefix wdt: <http://www.wikidata.org/prop/direct/> .\n");
        rdf.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        rdf.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
        rdf.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

        // Parse CSV and collect company data
        let mut companies: HashMap<String, CompanyData> = HashMap::new();
        let mut reader = csv::Reader::from_reader(csv_content.as_bytes());

        for result in reader.records() {
            let record = result?;

            // Extract fields
            let company_uri = record.get(0).unwrap_or("");
            if company_uri.is_empty() {
                continue;
            }

            let company_id = company_uri.split('/').last().unwrap_or("");
            let company_name = Self::escape_label(record.get(1).unwrap_or(""));
            let industry = record.get(2).and_then(|s| s.split('/').last());
            let inception = record.get(3);
            let owns = record.get(4);
            let owns_name = record.get(5);
            let owned_by = record.get(6);
            let owned_by_name = record.get(7);

            // Get or create company data
            let company = companies
                .entry(company_id.to_string())
                .or_insert_with(|| CompanyData {
                    label: company_name.to_string(),
                    industry: industry.map(String::from),
                    inception: inception.map(String::from),
                    owns: Vec::new(),
                    owned_by: Vec::new(),
                });

            // Add ownership relationships
            if let Some(owns_uri) = owns {
                if !owns_uri.is_empty() {
                    let owns_id = owns_uri.split('/').last().unwrap_or("");
                    let owns_label = owns_name
                        .map(Self::escape_label)
                        .unwrap_or(owns_id.to_string());
                    company.owns.push((owns_id.to_string(), owns_label));
                }
            }

            if let Some(owned_by_uri) = owned_by {
                if !owned_by_uri.is_empty() {
                    let owned_by_id = owned_by_uri.split('/').last().unwrap_or("");
                    let owned_by_label = owned_by_name
                        .map(Self::escape_label)
                        .unwrap_or(owned_by_id.to_string());
                    company
                        .owned_by
                        .push((owned_by_id.to_string(), owned_by_label));
                }
            }
        }

        // Write RDF for each company
        let mut processed_labels = HashSet::new();

        for (company_id, data) in companies.iter() {
            // Company declaration
            rdf.push_str(&format!(
                "wd:{} a wd:Q891723, wd:Q4830453, wd:Q163740 ;\n",
                company_id
            ));
            rdf.push_str(&format!("    rdfs:label \"{}\"@en ;\n", data.label));

            // Industry
            if let Some(industry) = &data.industry {
                rdf.push_str(&format!("    wdt:P452 wd:{}", industry));
            } else {
                rdf.push_str("    wdt:P452 wd:Q3510521"); // default to computer security
            }

            // Inception date
            if let Some(inception) = &data.inception {
                rdf.push_str(&format!(" ;\n    wdt:P571 \"{}\"^^xsd:dateTime", inception));
            }

            // Ownership relationships
            if !data.owns.is_empty() {
                rdf.push_str(" ;\n    wdt:P1830"); // owner of
                for (i, (owns_id, _)) in data.owns.iter().enumerate() {
                    if i == 0 {
                        rdf.push_str(&format!(" wd:{}", owns_id));
                    } else {
                        rdf.push_str(&format!(" , wd:{}", owns_id));
                    }
                }
            }

            if !data.owned_by.is_empty() {
                rdf.push_str(" ;\n    wdt:P127"); // owned by
                for (i, (owned_by_id, _)) in data.owned_by.iter().enumerate() {
                    if i == 0 {
                        rdf.push_str(&format!(" wd:{}", owned_by_id));
                    } else {
                        rdf.push_str(&format!(" , wd:{}", owned_by_id));
                    }
                }
            }

            rdf.push_str(" .\n\n");

            // Add labels for owned/owner entities
            for (owns_id, owns_name) in &data.owns {
                let label_key = format!("{}_label", owns_id);
                if owns_name != owns_id && !processed_labels.contains(&label_key) {
                    processed_labels.insert(label_key);
                    rdf.push_str(&format!(
                        "wd:{} rdfs:label \"{}\"@en .\n\n",
                        owns_id, owns_name
                    ));
                }
            }

            for (owned_by_id, owned_by_name) in &data.owned_by {
                let label_key = format!("{}_label", owned_by_id);
                if owned_by_name != owned_by_id && !processed_labels.contains(&label_key) {
                    processed_labels.insert(label_key);
                    rdf.push_str(&format!(
                        "wd:{} rdfs:label \"{}\"@en .\n\n",
                        owned_by_id, owned_by_name
                    ));
                }
            }
        }

        Ok(rdf)
    }

    /// Escape quotes and backslashes in RDF labels
    fn escape_label(label: &str) -> String {
        label.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Complete workflow: download and convert to RDF
    pub async fn download_and_convert(&mut self) -> Result<PathBuf> {
        println!("=== Downloading Tech/Security Companies from Wikidata ===");
        println!("Entity types:");
        println!("  - Q891723: public company");
        println!("  - Q4830453: business");
        println!("  - Q163740: nonprofit organization");
        println!();
        println!("Filtering to industries:");
        println!("  - Q3510521: computer security");
        println!("  - Q21157865: cybersecurity");
        println!("  - Q880371: computer network");
        println!("  - Q638608: cloud computing");
        println!("  - Q484847: cryptocurrency");
        println!("  - Q97466080: information technology");
        println!("  - Q11451: agriculture");
        println!();

        // Step 1: Get count
        println!("Step 1: Counting entities...");
        let count = self.get_company_count().await?;
        println!("Total computer security companies: {}", count);

        if count > 10000 {
            anyhow::bail!("Too many entities ({}). This might timeout.", count);
        }

        println!();
        println!("Step 2: Downloading company data...");

        // Step 2: Download CSV
        let csv_path = self.download_companies_csv().await?;

        // Count rows
        let csv_content = fs::read_to_string(&csv_path)?;
        let row_count = csv_content.lines().count() - 1; // subtract header
        println!("Downloaded {} rows", row_count);

        // Step 3: Convert to RDF
        println!();
        println!("Step 3: Converting to RDF...");
        let rdf_content = Self::csv_to_rdf(&csv_path)?;

        // Count companies in RDF before writing
        let company_count = rdf_content.matches("a wd:Q891723").count();

        let ttl_path = self.data_dir.join("security_companies.ttl");
        fs::write(&ttl_path, rdf_content)?;
        println!("Processed {} companies", company_count);

        println!();
        println!("=== Download Complete ===");
        println!("CSV file: {}", csv_path.display());
        println!("RDF file: {}", ttl_path.display());
        println!("Total companies: {}", company_count);
        println!();
        println!("This focused dataset (tech/security companies) maintains OPSEC");
        println!("while being small enough to avoid timeouts.");
        println!();
        println!("All data was downloaded through Tor for privacy.");
        println!();
        println!("Industries included:");
        println!("  - Computer security & Cybersecurity");
        println!("  - Computer networks & Cloud computing");
        println!("  - Cryptocurrency");
        println!("  - Information technology");
        println!("  - Agriculture");

        Ok(ttl_path)
    }
}

#[tokio::test]
#[ignore] // This test requires network access and Tor, run with: cargo test --ignored
async fn test_wikidata_download() -> Result<()> {
    // Set up test data directory
    let data_dir = PathBuf::from("runtime/wikidata");

    // Create downloader
    let mut downloader = WikidataDownloader::new(data_dir).await?;

    // Run the complete workflow
    let rdf_path = downloader.download_and_convert().await?;

    // Verify the file was created
    assert!(rdf_path.exists());

    // Verify it contains RDF content
    let content = fs::read_to_string(&rdf_path)?;
    assert!(content.contains("@prefix wd:"));
    assert!(content.contains("wdt:P452")); // industry property

    Ok(())
}

#[tokio::test]
async fn test_csv_to_rdf_conversion() -> Result<()> {
    // Create a sample CSV for testing (note: multiple rows can represent the same company with different relationships)
    let test_csv = r#"company,companyName,industry,inception,owns,ownsName,ownedBy,ownedByName
http://www.wikidata.org/entity/Q123,Test Corp,http://www.wikidata.org/entity/Q3510521,2020-01-01T00:00:00Z,http://www.wikidata.org/entity/Q456,SubCorp,,
http://www.wikidata.org/entity/Q123,Test Corp,http://www.wikidata.org/entity/Q3510521,2020-01-01T00:00:00Z,,,http://www.wikidata.org/entity/Q789,Parent Inc
"#;

    let temp_dir = tempfile::tempdir()?;
    let csv_path = temp_dir.path().join("test.csv");
    fs::write(&csv_path, test_csv)?;

    // Convert to RDF
    let rdf = WikidataDownloader::csv_to_rdf(&csv_path)?;

    // Verify RDF content
    assert!(rdf.contains("@prefix wd:"));
    assert!(rdf.contains("wd:Q123"));
    assert!(rdf.contains("rdfs:label \"Test Corp\"@en"));
    assert!(rdf.contains("wdt:P452 wd:Q3510521"));
    assert!(rdf.contains("wdt:P571 \"2020-01-01T00:00:00Z\"^^xsd:dateTime"));
    assert!(rdf.contains("wdt:P1830 wd:Q456")); // owns
    assert!(rdf.contains("wdt:P127 wd:Q789")); // owned by

    Ok(())
}
