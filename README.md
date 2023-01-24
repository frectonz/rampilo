# Rampilo

Rampilo is a simple telegram crawler that checks every message in a chat and extracts mentions of usernames and telegram links. It also keeps a count of how many times a username has been mentioned.

It needs a telegram API key and API hash to work. You can get them from [here](https://my.telegram.org/). It will ask you for API key and API hash when you run it for the first time and store them in a file called `api_info.json` in the same directory. After that it will sign in to telegram. To do that it will ask you for your phone number and a verification code that will be sent to your phone. If you have 2FA enabled, it will ask you for your password as well.

Don't worry, you only need to do this once. After that it will store your session in a file called `crawler.session` in the current directory. It will use this session to sign in to telegram the next time you run it.

For normal usage you only need to provide the username of the chat (group/channel) you want to crawl. It will show you progress bar as it crawls the chat. When it's done it will store the results in a file called `<username>.json` in the current directory. The output file will have the following schema.

```text
[
  {
    "username": {
      "Username": string
    } | {
      "Hash": string
    } | {
      "Mention": string
    },
    "count": number,
    "metadata": {
      "name": string,
      "type": "Group" | "Channel" | "User"
    }
  }
]
```

```json
[
  {
    "username": {
      "Mention": "codenight"
    },
    "count": 4,
    "metadata": {
      "name": "CodeNight",
      "type": "Group"
    }
  }
]
```

## Usage

- `git clone`
- `cd rampilo`
- `cargo run`

## What does `rampilo` mean?

Rampilo is `crawler` in Esperanto.
