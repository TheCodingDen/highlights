# Highlights

Highlights is a simple but flexible highlighting bot for Discord. Add a keyword to get notified in direct messages when your keyword appears in that server.

You can add highlights to your server directly with [this link](https://discord.com/api/oauth2/authorize?client_id=740802975576096829&scope=bot). If you run into any problems, please make an issue here or let me know on [The Coding Den](https://discord.gg/code), `@ThatsNoMoon#0175`.

## Features
- Add keywords to be notified about, per-server or per-channel
- Ignore phrases to make your keywords more specific
- Mute channels to filter out noise
- Block obnoxious users

For self-hosters, highlights includes:
- Automatic backups and backup pruning
- Error reporting via [Discord webhook](https://support.discord.com/hc/en-us/articles/228383668-Intro-to-Webhooks)
- Performance monitoring with [Prometheus](https://prometheus.io)

## Download
You can find downloads for 64 bit Windows and Linux, as well as 64 bit Linux ARM (for e.g. Raspberry Pi) on [the releases page](https://github.com/ThatsNoMoon/highlights/releases/).

## Building
Highlights requires `cargo` to be built. [rustup](https://rustup.rs) is the recommended installation method for most use-cases.

Once you have `cargo` installed, run `cargo build --release` (or `cargo build` for an unoptimized build) to produce an executable at `target/release/highlights` (or `target/debug/highlights`).

## Configuration

Highlights is configured using a TOML file at `./config.toml` by default. To use a different path, set the `HIGHLIGHTS_CONFIG` environment variable. The default config with documentation is provided [here](example_config.toml). All options can be set using environment variables using this format: `HIGHLIGHTS_SECTION.PROPERTY`. Highlights also supports dotenv. To use it, create a file `.env` in the directory you run highlights in, and put key-value pairs in it, like the below:
```
HIGHLIGHTS_BOT.TOKEN="your bot token goes here"
HIGHLIGHTS_DATABASE.PATH="highlights_data"
```

## Backups

Unless backups are disabled in the config, highlights automatically backs up its database every time it starts, and every 24hrs after that. These backups are saved to the `./backups` folder in the configured database path. These backups are a full snapshot of the database, so to restore one you can just move it back to the database path and rename it to `data.db`. Highlights doesn't delete any backups from the last 24hrs, but it does clean up older backups automatically:
- Roughly one backup per day is kept for the past week
- Roughly one backup per week is kept for the past month
- Roughly one backup per month is kept for the past year
- Backups older than a year are deleted

Highlights uses the timestamp embedded in the backup name to determine how old it is, so don't mess with the file names (it'll log a warning about any files it doesn't recognize).

## Monitoring

If you set the `logging.prometheus` config option, highlights will track command and database query execution times to be reported by [Prometheus](https://prometheus.io). The address should be in the form `address:port`, e.g. `127.0.0.1:9000`.

Example prometheus config to scrape `127.0.0.1:9000`:
```yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'highlights'
    static_configs:
      - targets: ['127.0.0.1:9000']
```

Highlights reports the metrics `highlights_command_time` and `highlights_query_time`.

## License

Highlights is licensed under the [OSL 3.0](https://choosealicense.com/licenses/osl-3.0/). Derivatives must be licensed under OSL 3.0, but this does not include any linking restrictions; you may link this code to closed-source code.
