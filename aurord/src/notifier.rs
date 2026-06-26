use serde::Serialize;

#[derive(Serialize)]
struct DiscordMessage {
    content: String,
}

pub struct DiscordNotifier {
    notification_webhook_url: Option<String>,
    error_webhook_url: Option<String>,
}

impl DiscordNotifier {
    pub fn new(
        notification_webhook_url: Option<String>,
        error_webhook_url: Option<String>,
    ) -> Self {
        Self {
            notification_webhook_url,
            error_webhook_url,
        }
    }

    /// Sends a notification to Discord on a successful package upgrade.
    pub async fn notify_success(&self, pkg_name: &str, version: &str) {
        if let Some(ref url) = self.notification_webhook_url {
            if url.is_empty() || url.contains("your-notification-webhook-url-here") {
                return;
            }
            let msg = DiscordMessage {
                content: format!(
                    "🚀 **[aurord] Package Updated Successfully!**\n**Package**: `{}`\n**Version**: `{}`\n**Status**: Success",
                    pkg_name, version
                ),
            };
            if let Err(e) = self.send(url, &msg).await {
                tracing::error!("Discord success notification failed: {}", e);
            }
        }
    }

    /// Sends a notification to Discord on a package upgrade failure.
    pub async fn notify_failure(&self, pkg_name: &str, error: &str) {
        if let Some(ref url) = self.error_webhook_url {
            if url.is_empty() || url.contains("your-error-webhook-url-here") {
                return;
            }
            // Truncate error if too long for Discord message size limits
            let clean_err = if error.len() > 1500 {
                format!("{}...", &error[..1500])
            } else {
                error.to_string()
            };
            let msg = DiscordMessage {
                content: format!(
                    "⚠️ **[aurord] Package Update Failed!**\n**Package**: `{}`\n**Status**: Failed\n**Error**:\n```\n{}\n```",
                    pkg_name, clean_err
                ),
            };
            if let Err(e) = self.send(url, &msg).await {
                tracing::error!("Discord failure notification failed: {}", e);
            }
        }
    }

    async fn send(&self, url: &str, msg: &DiscordMessage) -> Result<(), String> {
        let client = reqwest::Client::new();
        let response = client
            .post(url)
            .json(msg)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Server returned status code: {}",
                response.status()
            ));
        }
        Ok(())
    }
}
