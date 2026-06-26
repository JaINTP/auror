use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UpstreamConfig {
    ElectronBuilder { url: String },
    Github { repo: String },
    Pypi { package: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscordConfig {
    pub notification_webhook_url: Option<String>,
    pub error_webhook_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub packages: HashMap<String, UpstreamConfig>,
    pub discord: Option<DiscordConfig>,
}

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Self {
        let home = home::home_dir().expect("Could not find home directory");
        let config_path = home.join(".config/auror/config.toml");
        Self { config_path }
    }

    pub fn get_config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn load_or_create_default(&self) -> Result<Config, String> {
        if !self.config_path.exists() {
            if let Some(parent) = self.config_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }
            let default_config = self.generate_default_config();
            let toml_str = toml::to_string_pretty(&default_config)
                .map_err(|e| format!("Failed to serialize default config: {}", e))?;
            fs::write(&self.config_path, toml_str)
                .map_err(|e| format!("Failed to write default config file: {}", e))?;
            Ok(default_config)
        } else {
            let content = fs::read_to_string(&self.config_path)
                .map_err(|e| format!("Failed to read config file: {}", e))?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| format!("Failed to parse config file: {}", e))?;
            Ok(config)
        }
    }

    fn generate_default_config(&self) -> Config {
        let mut packages = HashMap::new();
        packages.insert(
            "capacities-appimage".to_string(),
            UpstreamConfig::ElectronBuilder {
                url: "https://2vks4.upcloudobjects.com/capacities-desktop-app/latest-linux.yml"
                    .to_string(),
            },
        );
        packages.insert(
            "devpod-community-appimage".to_string(),
            UpstreamConfig::Github {
                repo: "skevetter/devpod".to_string(),
            },
        );
        packages.insert(
            "devpod-community-bin".to_string(),
            UpstreamConfig::Github {
                repo: "skevetter/devpod".to_string(),
            },
        );
        packages.insert(
            "panoptic".to_string(),
            UpstreamConfig::Github {
                repo: "JaINTP/Panoptic".to_string(),
            },
        );
        packages.insert(
            "python-chromadb".to_string(),
            UpstreamConfig::Pypi {
                package: "chromadb".to_string(),
            },
        );
        packages.insert(
            "python-mempalace".to_string(),
            UpstreamConfig::Pypi {
                package: "mempalace".to_string(),
            },
        );
        let discord = Some(DiscordConfig {
            notification_webhook_url: Some(
                "https://discord.com/api/webhooks/your-notification-webhook-url-here".to_string(),
            ),
            error_webhook_url: Some(
                "https://discord.com/api/webhooks/your-error-webhook-url-here".to_string(),
            ),
        });
        Config { packages, discord }
    }
}
