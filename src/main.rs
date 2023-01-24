use std::{collections::HashMap, fs};

use color_eyre::eyre::{self, Result};
use grammers_client::{
    types::{chat::Chat, Message},
    Client, Config, SignInError,
};
use grammers_session::Session;
use grammers_tl_types::enums::MessageEntity;
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

#[derive(Debug, Deserialize, Serialize)]
struct Username {
    username: LinkType,
    count: usize,
    metadata: Option<UsernameMetadata>,
}

impl Username {
    fn new(username: LinkType) -> Self {
        Self {
            username,
            count: 0,
            metadata: None,
        }
    }
}

#[derive(Deserialize, Serialize, Hash, PartialEq, Eq, Debug)]
enum LinkType {
    Username(String),
    Hash(String),
    Mention(String),
}

impl ToString for LinkType {
    fn to_string(&self) -> String {
        match self {
            LinkType::Username(username) => username.to_string(),
            LinkType::Hash(hash) => hash.to_string(),
            LinkType::Mention(username) => username.to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct UsernameMetadata {
    name: String,
    #[serde(rename = "type")]
    type_: UsernameType,
}

#[derive(Debug, Deserialize, Serialize)]
enum UsernameType {
    User,
    Group,
    Channel,
}

impl From<&Chat> for UsernameMetadata {
    fn from(chat: &Chat) -> Self {
        let type_ = match chat {
            Chat::User(_) => UsernameType::User,
            Chat::Group(_) => UsernameType::Group,
            Chat::Channel(_) => UsernameType::Channel,
        };

        Self {
            name: chat.name().to_string(),
            type_,
        }
    }
}

type Usernames = HashMap<String, Username>;

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

    let mut usernames: Usernames = HashMap::new();

    let mut count = 0;
    let mut messages = client_handle.iter_messages(&chat);
    while let Some(message) = messages.next().await? {
        extract_link(&message, &mut usernames);
        extract_mentions(&message, &mut usernames);
        count += 1;
    }

    let mut usernames: Vec<_> = usernames.into_iter().map(|(_, v)| v).collect();
    usernames.sort_by(|a, b| b.count.cmp(&a.count));

    for username in usernames.iter_mut() {
        let entity_username = match username.username {
            LinkType::Username(ref username) => username.as_str(),
            LinkType::Mention(ref username) => username.as_str(),
            LinkType::Hash(_) => continue,
        };

        let maybe_user = client_handle.resolve_username(entity_username).await?;
        if let Some(ref chat) = maybe_user {
            username.metadata = Some(chat.into());
        }
    }

    let json = serde_json::to_string_pretty(&usernames)?;
    let filename = format!("{}.json", username);
    fs::write(filename, json)?;

    println!(
        "Saved {} usernames from {count} messages to {username}.json",
        usernames.len(),
    );

    Ok(())
}

fn extract_link(message: &Message, usernames: &mut Usernames) {
    let text = message.text();
    if let Some(username) = extract(text) {
        usernames
            .entry(username.to_string().to_lowercase())
            .and_modify(|u| {
                u.count += 1;
            })
            .or_insert(Username::new(username));
    }
}

fn extract_mentions(message: &Message, usernames: &mut Usernames) {
    let text = message.text();
    let empty = Vec::<MessageEntity>::new();
    let entities: &Vec<MessageEntity> = message.fmt_entities().unwrap_or(&empty);

    for entity in entities {
        if let MessageEntity::Mention(e) = entity {
            let offset = e.offset as usize;
            let length = e.length as usize;

            let points = text.encode_utf16().collect::<Vec<_>>();

            let username = &points[offset..offset + length];
            let username = String::from_utf16_lossy(username);
            let username = username.trim_start_matches('@').trim().to_lowercase();
            let username = LinkType::Mention(username.to_string());

            usernames
                .entry(username.to_string())
                .and_modify(|u| {
                    u.count += 1;
                })
                .or_insert(Username::new(username));
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
