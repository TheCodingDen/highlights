# Highlights

Highlights is a simple but flexible highlighting bot for Discord. Add a keyword to get notified in direct messages when your keyword appears in that server.

You can add highlights to your server directly with [this link](https://discord.com/api/oauth2/authorize?client_id=740802975576096829&scope=bot+applications.commands). If you run into any problems, please [make an issue here](https://github.com/ThatsNoMoon/highlights/issues/new?template=bug_report.md) or let me know on [the Highlights dev server](https://discord.gg/9phBJ9tzQ2), `@ThatsNoMoon#0175`.

## Features
- Add keywords to be notified about, per-server or per-channel
- Ignore phrases to make your keywords more specific
- Mute channels to filter out noise
- Block obnoxious users

For self-hosters, highlights includes:
- PostgreSQL and SQLite support
- Automatic SQLite backups and backup pruning
- Error reporting via [Discord webhook](https://support.discord.com/hc/en-us/articles/228383668-Intro-to-Webhooks)
- Performance monitoring and observability with [Jaeger](https://jaegertracing.io/)
- Feature flags for smaller custom builds

## Docker
You can find a Dockerfile in this repository, or use [`thatsnomoon/highlights`](https://hub.docker.com/r/thatsnomoon/highlights). Also provided is a `docker-compose.yml` that will organize Highlights, a Jaeger agent, collector, and query server, and Cassandra, and should set up Cassandra to accept Jaeger logs.

### AArch64, other alternate architectures

The Dockerfile provided supports building to any architecture supported by both Rust and [musl.cc](https://musl.cc). I build `thatsnomoon/highlights` for AArch64 alongside x86_64; if you need a different architecture, use `docker buildx build` with `--platform=linux/<architecture>` and provide appropriate values for the following build args:
- `--build-arg RUSTTARGET=<rust target triple>` (ex: `aarch64-unknown-linux-musl`)
- `--build-arg MUSLHOST=<musl host triple>` (ex: `x86_64-linux-musl`; see [supported musl.cc hosts](https://more.musl.cc/10.2.1))
- `--build-arg MUSLTARGET=<musl target triple>` (ex: `aarch64-linux-musl`; for x86_64, see [supported musl.cc targets here](https://more.musl.cc/10.2.1/x86_64-linux-musl))

## Download
You can find downloads for 64 bit Windows and Linux, as well as 64 bit Linux ARM (for e.g. Raspberry Pi) on [the releases page](https://github.com/ThatsNoMoon/highlights/releases/).

## Building
Highlights requires `cargo` to be built. [rustup](https://rustup.rs) is the recommended installation method for most use-cases.

Once you have `cargo` installed, run `cargo build --release` (or `cargo build` for an unoptimized build) to produce an executable at `target/release/highlights` (or `target/debug/highlights`).

If you're contributing to highlights, I recommend moving the `pre-commit` file to `.git/hooks` so your code is checked for issues before committing (avoiding the need for commits to fix `rustfmt` or `clippy` errors).

## Configuration

Highlights is configured using a TOML file at `./config.toml` by default. To use a different path, set the `HIGHLIGHTS_CONFIG` environment variable. The default config with documentation is provided [here](example_config.toml). All options can be set using environment variables using this format: `HIGHLIGHTS_SECTION_PROPERTY`. Examples:
```
HIGHLIGHTS_BOT_TOKEN="your bot token goes here"
HIGHLIGHTS_BOT_APPLICATIONID="your discord application id (not bot token) here"
HIGHLIGHTS_DATABASE_PATH="highlights_data"
```
As in the above example, underscores in property names should be removed so that they aren't interpreted as section separators.

## Backups

Unless backups are disabled in the config, highlights automatically backs up its database every time it starts, and every 24hrs after that. These backups are saved to the `./backups` folder in the configured database path. These backups are a full snapshot of the database, so to restore one you can just move it back to the database path and rename it to `data.db`. Highlights doesn't delete any backups from the last 24hrs, but it does clean up older backups automatically:
- Seven daily backups are kept
- Four weekly backups are kept
- Twelve monthly backups are kept
- Indefinite yearly backups are kept

Note that highlights will always keep up to these numbers of backups. For example, even if there are not seven backups from the last week, the seven most recent backups made at least a day apart will be saved; likewise, the next four most recent backups made at least a week apart will be saved, and so on.

Highlights uses the timestamp embedded in the backup name to determine how old it is, so don't mess with the file names (it'll log a warning about any files it doesn't recognize).

## Monitoring

If you set the `logging.jaeger` config option, highlights will trace execution times to be reported by [Jaeger](https://jaegertracing.io/). The address should be in the form `address:port`, e.g. `127.0.0.1:6831`. You should provide the address of your Jaeger agent.

## License

Highlights is licensed under the [OSL 3.0](https://choosealicense.com/licenses/osl-3.0/). Derivatives must be licensed under OSL 3.0, but this does not include any linking restrictions; you may link this code to closed-source code.
