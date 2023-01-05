use std::{collections::HashMap, fs};

use color_eyre::eyre::{self, Result};
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use inquire::{validator::Validation, Password, Text};
use regex::Regex;
use serde::{Deserialize, Serialize};

const SESSION_FILE: &str = "crawler.session";

#[derive(Deserialize, Serialize)]
struct ApiCredentials {
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

    fn load() -> Result<Self> {
        if let Ok(api_info) = Self::load_from_file() {
            Ok(api_info)
        } else {
            Self::load_from_input()
        }
    }

    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write("api_info.json", json)?;

        Ok(())
    }

    fn api_id(&self) -> i32 {
        self.api_id
    }

    fn api_hash(&self) -> &str {
        &self.api_hash
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    println!("Connecting to Telegram servers...");
    let session = Session::load_file_or_create("crawler.session")?;

    let credentials = ApiCredentials::load()?;

    let client = Client::connect(Config {
        session,
        api_id: credentials.api_id(),
        api_hash: credentials.api_hash().to_owned(),
        params: Default::default(),
    })
    .await?;

    println!("Connected!");

    let is_authorized = client.is_authorized().await?;

    if !is_authorized {
        sign_in(&client, credentials.api_id(), credentials.api_hash()).await?;
        credentials.save()?;
    }

    let client_handle = client.clone();

    let username = Text::new("Enter the username: ").prompt()?;
    let maybe_chat = client_handle.resolve_username(&username).await?;

    let chat = maybe_chat
        .ok_or_else(|| eyre::eyre!("Could not find a chat with the username {}", username))?;

    let mut messages = client_handle.search_messages(&chat).query("https://t.me/");

    let mut usernames: HashMap<String, (LinkType, usize)> = HashMap::new();
    while let Some(message) = messages.next().await? {
        let text = message.text();

        if let Some(username) = extract(text) {
            usernames
                .entry(username.to_string())
                .and_modify(|(_, count)| {
                    *count += 1;
                })
                .or_insert((username, 1));
        }
    }

    println!("Found {} usernames", usernames.len());

    let json = serde_json::to_string_pretty(&usernames)?;
    let filename = format!("{}.json", username);
    fs::write(filename, json)?;

    println!("Saved {} usernames to {}.json", usernames.len(), username);

    Ok(())
}

#[derive(Deserialize, Serialize, Hash, PartialEq, Eq, Debug)]
enum LinkType {
    Username(String),
    Hash(String),
}

impl ToString for LinkType {
    fn to_string(&self) -> String {
        match self {
            LinkType::Username(username) => username.to_string(),
            LinkType::Hash(hash) => hash.to_string(),
        }
    }
}

fn extract(link: &str) -> Option<LinkType> {
    extract_username(link)
        .map(LinkType::Username)
        .or_else(|| extract_hash(link).map(LinkType::Hash))
}

fn extract_username(link: &str) -> Option<String> {
    let regex = Regex::new(r"https://t.me/([a-zA-Z0-9_]+)").unwrap();
    let captures = regex.captures(link)?;
    let group_name = captures.get(1)?.as_str();
    match group_name {
        "joinchat" | "addstickers" | "addemoji" | "addtheme" | "share" | "socks" | "proxy"
        | "bg" | "login" | "invoice" | "setlanguage" | "confirmphone" | "path" | "c" => None,
        _ => Some(group_name.to_string()),
    }
}

fn extract_hash(link: &str) -> Option<String> {
    let regex = Regex::new(r"https://t.me/(joinchat/|\+)([a-zA-Z0-9_-]+)").unwrap();
    let captures = regex.captures(link)?;
    let group_name = captures.get(2)?.as_str();
    Some(group_name.to_string())
}

async fn sign_in(client: &Client, api_id: i32, app_hash: &str) -> Result<()> {
    println!("Signing in...");

    let phone = Text::new("Enter your phone number: ").prompt()?;
    let token = client.request_login_code(&phone, api_id, app_hash).await?;
    let code = Text::new("Enter the code: ").prompt()?;
    let sign_in = client.sign_in(&token, &code).await;

    match sign_in {
        Ok(user) => {
            println!("Signed in as {}!", user.first_name());
        }
        Err(SignInError::PasswordRequired(password_token)) => {
            let password = Password::new("Enter the password: ").prompt()?;

            client
                .check_password(password_token, password.trim())
                .await?;
        }
        Err(e) => return Err(e.into()),
    };

    client.session().save_to_file(SESSION_FILE)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_username() {
        let link = "https://t.me/grammers";
        let username = extract(link);
        assert_eq!(username, Some(LinkType::Username("grammers".to_string())));
    }

    #[test]
    fn test_extract_username_with_query() {
        let link = "https://t.me/grammers?start=123";
        let username = extract(link);
        assert_eq!(username, Some(LinkType::Username("grammers".to_string())));
    }

    #[test]
    fn test_joined_username() {
        let link = "https://t.me/joinchat/USpx-sviNKIj408g";
        let username = extract(link);
        assert_eq!(
            username,
            Some(LinkType::Hash("USpx-sviNKIj408g".to_string()))
        );
    }

    #[test]
    fn test_invite_link() {
        let link = "https://t.me/+_DGX2NIt9IhkNTVk";
        let username = extract(link);
        assert_eq!(
            username,
            Some(LinkType::Hash("_DGX2NIt9IhkNTVk".to_string()))
        );
    }
}
