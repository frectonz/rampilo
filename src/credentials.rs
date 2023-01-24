use color_eyre::eyre::Result;
use inquire::{validator::Validation, Text};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize)]
pub struct ApiCredentials {
    api_id: i32,
    api_hash: String,
}

impl ApiCredentials {
    fn load_from_file() -> Result<Self> {
        let contents = fs::read_to_string("api_info.json")?;
        let api_info: Self = serde_json::from_str(&contents)?;
        Ok(api_info)
    }

    fn load_from_input() -> Result<Self> {
        let api_id = Text::new("Enter your API ID: ")
            .with_validator(|s: &str| {
                let validation = s
                    .parse::<i32>()
                    .map(|_| Validation::Valid)
                    .unwrap_or_else(|_| Validation::Invalid("API ID must be a number".into()));

                Ok(validation)
            })
            .prompt()?;

        let api_id = api_id.parse::<i32>()?;
        let api_hash = Text::new("Enter your API hash: ").prompt()?;

        let api_info = Self { api_id, api_hash };
        let json = serde_json::to_string_pretty(&api_info)?;
        fs::write("api_info.json", json)?;

        Ok(api_info)
    }

    pub fn load() -> Result<Self> {
        if let Ok(api_info) = Self::load_from_file() {
            Ok(api_info)
        } else {
            Self::load_from_input()
        }
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write("api_info.json", json)?;

        Ok(())
    }

    pub fn api_id(&self) -> i32 {
        self.api_id
    }

    pub fn api_hash(&self) -> &str {
        &self.api_hash
    }
}
