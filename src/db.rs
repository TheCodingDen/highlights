// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

mod backup;
use backup::start_backup_cycle;

use once_cell::sync::OnceCell;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Error, OpenFlags, OptionalExtension, Row};
use serenity::model::id::{ChannelId, GuildId, UserId};
use tokio::task;

use std::{convert::TryInto, env, fs, io::ErrorKind, path::PathBuf};

use crate::monitoring::Timer;

static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

pub fn init() {
	let data_dir: PathBuf = env::var("HIGHLIGHTS_DATA_DIR")
		.map(Into::into)
		.unwrap_or("data".into());

	if let Err(error) = fs::create_dir(&data_dir) {
		if error.kind() != ErrorKind::AlreadyExists {
			Err::<(), _>(error).expect("Failed to create data directory");
		}
	}

	let manager = SqliteConnectionManager::file(data_dir.join("data.db"))
		.with_flags(
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		);

	let pool = Pool::new(manager).expect("Failed to open database pool");

	POOL.set(pool).unwrap();

	Mute::create_table();
	Ignore::create_table();
	Keyword::create_tables();
	UserState::create_table();

	if env::var_os("HIGHLIGHTS_DONT_BACKUP").is_none() {
		let backup_dir = data_dir.join("backup");

		start_backup_cycle(backup_dir);
	}
}

macro_rules! await_db {
	($name:literal: |$conn:ident| $body:block) => {{
		let _timer = Timer::query($name);
		task::spawn_blocking(move || {
			let $conn = connection();

			$body
			})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
		}};
}

#[derive(Debug, Clone, Copy)]
pub enum KeywordKind {
	Channel(i64),
	Guild(i64),
}

#[derive(Debug, Clone)]
pub struct Keyword {
	pub keyword: String,
	pub user_id: i64,
	pub kind: KeywordKind,
}

impl Keyword {
	fn from_guild_row(row: &Row) -> Result<Self, Error> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: row.get(1)?,
			kind: KeywordKind::Guild(row.get(2)?),
		})
	}

	fn from_channel_row(row: &Row) -> Result<Self, Error> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: row.get(1)?,
			kind: KeywordKind::Channel(row.get(2)?),
		})
	}

	fn create_tables() {
		let conn = connection();

		conn.execute(
			"CREATE TABLE IF NOT EXISTS guild_keywords (
				keyword TEXT NOT NULL,
				user_id INTEGER NOT NULL,
				guild_id INTEGER NOT NULL,
				PRIMARY KEY (keyword, user_id, guild_id)
			)",
			params![],
		)
		.expect("Failed to create guild_keywords table");

		conn.execute(
			"CREATE TABLE IF NOT EXISTS channel_keywords (
				keyword TEXT NOT NULL,
				user_id INTEGER NOT NULL,
				channel_id INTEGER NOT NULL,
				PRIMARY KEY (keyword, user_id, channel_id)
			)",
			params![],
		)
		.expect("Failed to create channel_keywords table");
	}

	pub async fn get_relevant_keywords(
		guild_id: GuildId,
		channel_id: ChannelId,
		author_id: UserId,
	) -> Result<Vec<Keyword>, Error> {
		await_db!("get keywords": |conn| {
			let guild_id: i64 = guild_id.0.try_into().unwrap();
			let channel_id: i64 = channel_id.0.try_into().unwrap();
			let author_id: i64 = author_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT guild_keywords.keyword, guild_keywords.user_id, guild_keywords.guild_id
					FROM guild_keywords
					WHERE guild_keywords.guild_id = ?
						AND guild_keywords.user_id != ?
						AND NOT EXISTS (
							SELECT mutes.user_id
								FROM mutes
								WHERE mutes.user_id = guild_keywords.user_id
									AND mutes.channel_id = ?
						)
				",
			)?;

			let guild_keywords = stmt.query_map(
					params![guild_id, author_id, channel_id],
					Keyword::from_guild_row
				)?;

			let mut keywords = guild_keywords.collect::<Result<Vec<_>, _>>()?;

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
					FROM channel_keywords
					WHERE user_id != ?
						AND channel_id = ?"
			)?;

			let channel_keywords = stmt.query_map(
				params![author_id, channel_id],
				Keyword::from_channel_row
			)?;

			let channel_keywords = channel_keywords.collect::<Result<Vec<_>, _>>()?;

			keywords.extend(channel_keywords);

			Ok(keywords)
		})
	}

	pub async fn user_guild_keywords(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Keyword>, Error> {
		await_db!("user guild keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, guild_id
				FROM guild_keywords
				WHERE user_id = ? AND guild_id = ?"
			)?;

			let keywords = stmt.query_map(params![user_id, guild_id], Keyword::from_guild_row)?;

			keywords.collect()
		})
	}

	pub async fn user_channel_keywords(
		user_id: UserId,
	) -> Result<Vec<Keyword>, Error> {
		await_db!("user channel keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
				FROM channel_keywords
				WHERE user_id = ?"
			)?;

			let keywords = stmt.query_map(params![user_id], Keyword::from_channel_row)?;

			keywords.collect()
		})
	}

	pub async fn user_keywords(user_id: UserId) -> Result<Vec<Keyword>, Error> {
		await_db!("user keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, guild_id
				FROM guild_keywords
				WHERE user_id = ?"
			)?;

			let guild_keywords = stmt.query_map(params![user_id], Keyword::from_guild_row)?;

			let mut keywords = guild_keywords.collect::<Result<Vec<_>, _>>()?;

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
				FROM channel_keywords
				WHERE user_id = ?"
			)?;

			let channel_keywords = stmt.query_map(params![user_id], Keyword::from_channel_row)?;

			keywords.extend(channel_keywords.collect::<Result<Vec<_>, _>>()?);

			Ok(keywords)
		})
	}

	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("keyword exists": |conn| {
			match self.kind {
				KeywordKind::Channel(channel_id) => {
					conn.query_row(
						"SELECT COUNT(*) FROM channel_keywords
						WHERE keyword = ? AND user_id = ? AND channel_id = ?",
						params![&self.keyword, self.user_id, channel_id],
						|row| Ok(row.get::<_, u32>(0)? == 1),
					)
				}
				KeywordKind::Guild(guild_id) => {
					conn.query_row(
						"SELECT COUNT(*) FROM guild_keywords
						WHERE keyword = ? AND user_id = ? AND guild_id = ?",
						params![&self.keyword, self.user_id, guild_id],
						|row| Ok(row.get::<_, u32>(0)? == 1),
					)
				}
			}
		})
	}

	pub async fn user_keyword_count(user_id: UserId) -> Result<u32, Error> {
		await_db!("count user keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_keywords = conn.query_row(
				"SELECT COUNT(*)
					FROM guild_keywords
					WHERE user_id = ?",
				params![user_id],
				|row| row.get::<_, u32>(0),
			)?;

			let channel_keywords = conn.query_row(
				"SELECT COUNT(*)
					FROM channel_keywords
					WHERE user_id = ?",
				params![user_id],
				|row| row.get::<_, u32>(0),
			)?;

			Ok(guild_keywords + channel_keywords)
		})
	}

	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert keyword": |conn| {
			match self.kind {
				KeywordKind::Guild(guild_id) => {
					conn.execute(
						"INSERT INTO guild_keywords (keyword, user_id, guild_id)
							VALUES (?, ?, ?)",
						params![&self.keyword, self.user_id, guild_id],
					)?;
				}
				KeywordKind::Channel(channel_id) => {
					conn.execute(
						"INSERT INTO channel_keywords (keyword, user_id, channel_id)
							VALUES (?, ?, ?)",
						params![&self.keyword, self.user_id, channel_id],
					)?;
				}
			}

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete keyword": |conn| {
			match self.kind {
				KeywordKind::Guild(guild_id) => {
					conn.execute(
						"DELETE FROM guild_keywords
							WHERE keyword = ? AND user_id = ? AND guild_id = ?",
						params![&self.keyword, self.user_id, guild_id],
					)?;
				}
				KeywordKind::Channel(channel_id) => {
					conn.execute(
						"DELETE FROM channel_keywords
							WHERE keyword = ? AND user_id = ? AND channel_id = ?",
						params![&self.keyword, self.user_id, channel_id],
					)?;
				}
			}

			Ok(())
		})
	}

	pub async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<usize, Error> {
		await_db!("delete keywords in guild": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();
			conn.execute(
				"DELETE FROM guild_keywords
					WHERE user_id = ? AND guild_id = ?",
				params![user_id, guild_id]
			)
		})
	}

	pub async fn delete_in_channel(
		user_id: UserId,
		channel_id: ChannelId,
	) -> Result<usize, Error> {
		await_db!("delete keywords in channel": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let channel_id: i64 = channel_id.0.try_into().unwrap();
			conn.execute(
				"DELETE FROM channel_keywords
					WHERE user_id = ? AND channel_id = ?",
				params![user_id, channel_id]
			)
		})
	}
}

#[derive(Debug, Clone)]
pub struct Mute {
	pub user_id: i64,
	pub channel_id: i64,
}

impl Mute {
	fn from_row(row: &Row) -> Result<Self, Error> {
		Ok(Mute {
			user_id: row.get(0)?,
			channel_id: row.get(1)?,
		})
	}

	fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS mutes (
			user_id INTEGER NOT NULL,
			channel_id INTEGER NOT NULL,
			PRIMARY KEY (user_id, channel_id)
			)",
			params![],
		)
		.expect("Failed to create follows table");
	}

	pub async fn user_mutes(user_id: UserId) -> Result<Vec<Mute>, Error> {
		await_db!("user mutes": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT user_id, channel_id
				FROM mutes
				WHERE user_id = ?"
			)?;

			let mutes = stmt.query_map(params![user_id], Mute::from_row)?;

			mutes.collect()
		})
	}

	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("mute exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM mutes
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id, self.channel_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
	}

	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert mute": |conn| {
			conn.execute(
				"INSERT INTO mutes (user_id, channel_id)
				VALUES (?, ?)",
				params![self.user_id, self.channel_id],
			)?;

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete mute": |conn| {
			conn.execute(
				"DELETE FROM mutes
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id, self.channel_id],
			)?;

			Ok(())
		})
	}
}

#[derive(Debug, Clone)]
pub struct Ignore {
	pub phrase: String,
	pub user_id: i64,
	pub guild_id: i64,
}

impl Ignore {
	fn from_row(row: &Row) -> Result<Self, Error> {
		Ok(Ignore {
			phrase: row.get(0)?,
			user_id: row.get(1)?,
			guild_id: row.get(2)?,
		})
	}

	fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS guild_ignores (
			phrase TEXT NOT NULL,
			user_id INTEGER NOT NULL,
			guild_id INTEGER NOT NULL,
			PRIMARY KEY (phrase, user_id, guild_id)
			)",
			params![],
		)
		.expect("Failed to create guild_ignores table");
	}

	pub async fn user_guild_ignores(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Ignore>, Error> {
		await_db!("user guild ignores": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ? AND guild_id = ?"
			)?;

			let ignores = stmt.query_map(params![user_id, guild_id], Ignore::from_row)?;

			ignores.collect()
		})
	}

	pub async fn user_ignores(user_id: UserId) -> Result<Vec<Ignore>, Error> {
		await_db!("user ignores": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT phrase, user_id, guild_id
				FROM guild_ignores
				WHERE user_id = ?"
			)?;

			let ignores = stmt.query_map(params![user_id], Ignore::from_row)?;

			ignores.collect()
		})
	}

	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("ignore exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![&*self.phrase, self.user_id, self.guild_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
	}

	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert ignore": |conn| {
			conn.execute(
				"INSERT INTO guild_ignores (phrase, user_id, guild_id)
				VALUES (?, ?, ?)",
				params![&*self.phrase, self.user_id, self.guild_id],
			)?;

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete ignore": |conn| {
			conn.execute(
				"DELETE FROM guild_ignores
				WHERE phrase = ? AND user_id = ? AND guild_id = ?",
				params![&*self.phrase, self.user_id, self.guild_id],
			)?;

			Ok(())
		})
	}

	pub async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<usize, Error> {
		await_db!("delete ignores in guild": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();
			conn.execute(
				"DELETE FROM guild_ignores
					WHERE user_id = ? AND guild_id = ?",
				params![user_id, guild_id]
			)
		})
	}
}

#[derive(Debug, Clone)]
pub struct UserState {
	pub user_id: i64,
	pub state: UserStateKind,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum UserStateKind {
	CannotDm = 0,
}

impl UserState {
	const CANNOT_DM_STATE: u8 = UserStateKind::CannotDm as u8;

	fn from_row(row: &Row) -> Result<Self, Error> {
		let user_id = row.get(0)?;
		let state = match row.get(1)? {
			Self::CANNOT_DM_STATE => UserStateKind::CannotDm,
			other => Err(Error::IntegralValueOutOfRange(1, other as i64))?,
		};

		Ok(Self { user_id, state })
	}

	fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS user_states (
			user_id INTEGER PRIMARY KEY,
			state INTEGER NOT NULL
			)",
			params![],
		)
		.expect("Failed to create user_states table");
	}

	pub async fn user_state(user_id: UserId) -> Result<Option<Self>, Error> {
		await_db!("user state": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT user_id, state
				FROM user_states
				WHERE user_id = ?"
			)?;

			stmt.query_row(params![user_id], Self::from_row).optional()
		})
	}

	pub async fn set(self) -> Result<(), Error> {
		await_db!("set user state": |conn| {
			conn.execute(
				"INSERT INTO user_states (user_id, state)
				VALUES (?, ?)
				ON CONFLICT (user_id)
					DO UPDATE SET state = excluded.state",
				params![self.user_id, self.state as u8],
			)?;

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete user state": |conn| {
			conn.execute(
				"DELETE FROM user_states
				WHERE user_id = ?",
				params![self.user_id],
			)?;

			Ok(())
		})
	}

	pub async fn clear(user_id: UserId) -> Result<(), Error> {
		await_db!("delete user state": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			conn.execute(
				"DELETE FROM user_states
				WHERE user_id = ?",
				params![user_id],
			)?;

			Ok(())
		})
	}
}
