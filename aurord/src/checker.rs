use crate::config::UpstreamConfig;
use async_trait::async_trait;
use regex::Regex;
use yaml_rust2::YamlLoader;

#[async_trait]
pub trait UpstreamChecker: Send + Sync {
    async fn fetch_latest_version(&self) -> Result<String, String>;
}

pub struct ElectronBuilderChecker {
    pub url: String,
}

#[async_trait]
impl UpstreamChecker for ElectronBuilderChecker {
    async fn fetch_latest_version(&self) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| format!("Failed to build client: {}", e))?;

        let response = client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {}", e))?
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let docs = YamlLoader::load_from_str(&response)
            .map_err(|e| format!("Failed to parse YAML: {}", e))?;

        let doc = docs
            .first()
            .ok_or_else(|| "Empty YAML document".to_string())?;

        let version = doc["version"]
            .as_str()
            .ok_or_else(|| "YAML does not contain 'version' field".to_string())?;

        Ok(version.to_string())
    }
}

pub struct GithubChecker {
    pub repo: String,
}

#[async_trait]
impl UpstreamChecker for GithubChecker {
    async fn fetch_latest_version(&self) -> Result<String, String> {
        let url = format!("https://github.com/{}.git", self.repo);
        let clean_env = crate::utils::get_clean_env();

        let output = tokio::process::Command::new("git")
            .args(&["ls-remote", "--tags", "--refs", &url])
            .envs(clean_env)
            .output()
            .await
            .map_err(|e| format!("git ls-remote execution failed: {}", e))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git ls-remote failed: {}", err.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let re = Regex::new(r"^refs/tags/v?(\d+\.\d+\.\d+)$")
            .map_err(|e| format!("Regex initialization failed: {}", e))?;

        let mut versions = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let ref_name = parts[1];
            if let Some(caps) = re.captures(ref_name) {
                if let Some(m) = caps.get(1) {
                    versions.push(m.as_str().to_string());
                }
            }
        }

        if versions.is_empty() {
            return Err(format!(
                "No valid semantic releases found for repo: {}",
                self.repo
            ));
        }

        // Sort semantic versions (major.minor.patch)
        versions.sort_by(|a, b| {
            let parse = |v: &str| -> Vec<u32> {
                v.split('.')
                    .map(|s| s.parse::<u32>().unwrap_or(0))
                    .collect()
            };
            parse(a).cmp(&parse(b))
        });

        Ok(versions.last().unwrap().clone())
    }
}

pub struct PyPiChecker {
    pub package: String,
}

#[async_trait]
impl UpstreamChecker for PyPiChecker {
    async fn fetch_latest_version(&self) -> Result<String, String> {
        let url = format!("https://pypi.org/pypi/{}/json", self.package);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| format!("Failed to build client: {}", e))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("PyPI returned status code: {}", response.status()));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let version = data["info"]["version"]
            .as_str()
            .ok_or_else(|| "JSON does not contain 'info.version' field".to_string())?;

        Ok(version.to_string())
    }
}

pub struct CheckerFactory;

impl CheckerFactory {
    pub fn create(config: &UpstreamConfig) -> Box<dyn UpstreamChecker> {
        match config {
            UpstreamConfig::ElectronBuilder { url } => {
                Box::new(ElectronBuilderChecker { url: url.clone() })
            }
            UpstreamConfig::Github { repo } => Box::new(GithubChecker { repo: repo.clone() }),
            UpstreamConfig::Pypi { package } => Box::new(PyPiChecker {
                package: package.clone(),
            }),
        }
    }
}
