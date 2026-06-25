//! Durable key/value storage backends for Rex `db.*` opcodes.
//!
//! SQLite remains the local default. When configured, Upstash implements the
//! same string KV contract over its connectionless HTTPS REST API.

use std::time::Duration;

use serde_json::Value as JsonValue;

const REST_URL_ENV: &str = "UPSTASH_REDIS_REST_URL";
const REST_TOKEN_ENV: &str = "UPSTASH_REDIS_REST_TOKEN";

pub struct UpstashClient {
    url: String,
    token: String,
    client: reqwest::blocking::Client,
}

impl UpstashClient {
    /// Load an Upstash REST client from the environment.
    ///
    /// Returns `Ok(None)` when neither variable is present, and an error for a
    /// partial configuration so deployments fail loudly instead of silently
    /// falling back to ephemeral SQLite.
    pub fn from_env() -> Result<Option<Self>, String> {
        let url = std::env::var(REST_URL_ENV).ok().filter(|v| !v.trim().is_empty());
        let token = std::env::var(REST_TOKEN_ENV).ok().filter(|v| !v.trim().is_empty());

        match (url, token) {
            (None, None) => Ok(None),
            (Some(url), Some(token)) => {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .map_err(|e| format!("failed to create Upstash client: {e}"))?;
                Ok(Some(Self {
                    url: url.trim_end_matches('/').to_string(),
                    token,
                    client,
                }))
            }
            _ => Err(format!(
                "Upstash requires both {REST_URL_ENV} and {REST_TOKEN_ENV}"
            )),
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        optional_string(self.command(&["GET", key])?, "GET")
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.command(&["SET", key, value])?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<Option<String>, String> {
        optional_string(self.command(&["GETDEL", key])?, "GETDEL")
    }

    pub fn list(&self, prefix: &str) -> Result<Vec<(String, String)>, String> {
        let pattern = format!("{}*", escape_redis_glob(prefix));
        let mut cursor = "0".to_string();
        let mut rows = Vec::new();

        loop {
            let result = self.command(&["SCAN", &cursor, "MATCH", &pattern, "COUNT", "1000"])?;
            let page = result.as_array()
                .filter(|page| page.len() == 2)
                .ok_or_else(|| "Upstash SCAN returned an invalid response".to_string())?;

            cursor = redis_string(&page[0], "SCAN cursor")?;
            let keys: Vec<String> = page[1].as_array()
                .ok_or_else(|| "Upstash SCAN returned invalid keys".to_string())?
                .iter()
                .map(|value| redis_string(value, "SCAN key"))
                .collect::<Result<_, _>>()?;

            if !keys.is_empty() {
                let mut command = Vec::with_capacity(keys.len() + 1);
                command.push("MGET".to_string());
                command.extend(keys.iter().cloned());
                let refs: Vec<&str> = command.iter().map(String::as_str).collect();
                let values = self.command(&refs)?;
                let values = values.as_array()
                    .ok_or_else(|| "Upstash MGET returned an invalid response".to_string())?;

                if values.len() != keys.len() {
                    return Err("Upstash MGET returned the wrong number of values".into());
                }

                for (key, value) in keys.into_iter().zip(values) {
                    if let Some(value) = optional_string(value.clone(), "MGET value")? {
                        rows.push((key, value));
                    }
                }
            }

            if cursor == "0" {
                break;
            }
        }

        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    }

    /// Atomic string compare-and-swap. `None` means the write succeeded;
    /// `Some(actual)` reports a conflict with the current value.
    pub fn compare_and_swap(
        &self,
        key: &str,
        expected: &str,
        replacement: &str,
    ) -> Result<Option<String>, String> {
        const SCRIPT: &str = "local current=redis.call('GET',KEYS[1]); if (current==ARGV[1]) or (not current and ARGV[1]=='') then redis.call('SET',KEYS[1],ARGV[2]); return {1}; end; return {0,current or ''}";
        let result = self.command(&["EVAL", SCRIPT, "1", key, expected, replacement])?;
        let values = result.as_array()
            .ok_or_else(|| "Upstash CAS returned an invalid response".to_string())?;

        match values.first().and_then(JsonValue::as_i64) {
            Some(1) => Ok(None),
            Some(0) => values.get(1)
                .ok_or_else(|| "Upstash CAS omitted the current value".to_string())
                .and_then(|value| redis_string(value, "CAS value"))
                .map(Some),
            _ => Err("Upstash CAS returned an invalid status".into()),
        }
    }

    fn command(&self, args: &[&str]) -> Result<JsonValue, String> {
        let response = self.client
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(args)
            .send()
            .map_err(|e| format!("Upstash request failed: {e}"))?;
        let status = response.status();
        let payload: JsonValue = response.json()
            .map_err(|e| format!("Upstash returned invalid JSON: {e}"))?;

        if let Some(error) = payload.get("error").and_then(JsonValue::as_str) {
            return Err(format!("Upstash command failed: {error}"));
        }
        if !status.is_success() {
            return Err(format!("Upstash returned HTTP {status}"));
        }

        payload.get("result").cloned()
            .ok_or_else(|| "Upstash response omitted result".to_string())
    }
}

fn optional_string(value: JsonValue, context: &str) -> Result<Option<String>, String> {
    match value {
        JsonValue::Null => Ok(None),
        JsonValue::String(value) => Ok(Some(value)),
        _ => Err(format!("Upstash {context} returned a non-string value")),
    }
}

fn redis_string(value: &JsonValue, context: &str) -> Result<String, String> {
    match value {
        JsonValue::String(value) => Ok(value.clone()),
        JsonValue::Number(value) => Ok(value.to_string()),
        _ => Err(format!("Upstash {context} was not a string")),
    }
}

fn escape_redis_glob(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '*' | '?' | '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::escape_redis_glob;

    #[test]
    fn escapes_scan_prefix_metacharacters() {
        assert_eq!(escape_redis_glob(r"article:[draft]*?\x"), r"article:\[draft\]\*\?\\x");
    }
}
