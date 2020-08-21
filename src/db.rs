use once_cell::sync::OnceCell;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Error, OpenFlags, Row};
use serenity::model::id::{ChannelId, GuildId, UserId};

use std::convert::TryInto;

static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

pub fn init() {
	let pool = {
		let manager = SqliteConnectionManager::file("data.db").with_flags(
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		);
		Pool::new(manager).expect("Failed to open database pool")
	};

	POOL.set(pool).unwrap();

	Follow::create_table();
	Keyword::create_table();
}

macro_rules! await_db {
	(|$conn:ident| $body:block) => {
		await_db!("database": |$conn| $body)
	};
	($name:literal: |$conn:ident| $body:block) => {
		::tokio::task::spawn_blocking(move || {
			let $conn = connection();

			$body
		})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
	};
}

#[derive(Debug, Clone)]
pub struct Keyword {
	pub keyword: String,
	pub user_id: i64,
	pub server_id: i64,
}

impl Keyword {
	fn from_row(row: &Row) -> Result<Self, Error> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: row.get(1)?,
			server_id: row.get(2)?,
		})
	}

	pub fn create_table() {
		let conn = connection();

		conn.execute(
			"CREATE TABLE IF NOT EXISTS keywords (
			keyword TEXT NOT NULL,
			user_id INTEGER NOT NULL,
			server_id INTEGER NOT NULL,
			PRIMARY KEY (keyword, user_id, server_id)
			)",
			params![],
		)
		.expect("Failed to create keywords table");
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
				"SELECT keywords.keyword, keywords.user_id, keywords.server_id
				FROM keywords
				INNER JOIN follows
				ON keywords.user_id = follows.user_id
				WHERE keywords.server_id = ? AND follows.channel_id = ? AND keywords.user_id != ?",
			)?;

			let keywords =
				stmt.query_map(params![guild_id, channel_id, author_id], Keyword::from_row)?;

			keywords.collect()
		})
	}

	pub async fn user_keywords_in_server(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Keyword>, Error> {
		await_db!("user server keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			let guild_id: i64 = guild_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, server_id
				FROM keywords
				WHERE user_id = ? AND server_id = ?"
			)?;

			let keywords = stmt.query_map(params![user_id, guild_id], Keyword::from_row)?;

			keywords.collect()
		})
	}

	pub async fn user_keywords(user_id: UserId) -> Result<Vec<Keyword>, Error> {
		await_db!("user keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, server_id
				FROM keywords
				WHERE user_id = ?"
			)?;

			let keywords = stmt.query_map(params![user_id], Keyword::from_row)?;

			keywords.collect()
		})
	}

	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("keyword exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM keywords
				WHERE keyword = ? AND user_id = ? AND server_id = ?",
				params![&self.keyword, self.user_id, self.server_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
	}

	pub async fn user_keyword_count(user_id: UserId) -> Result<u32, Error> {
		await_db!("count user keywords": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();
			conn.query_row(
				"SELECT COUNT(*) FROM keywords WHERE user_id = ?",
				params![user_id],
				|row| row.get::<_, u32>(0),
			)
		})
	}

	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert keyword": |conn| {
			conn.execute(
				"INSERT INTO keywords (keyword, user_id, server_id)
				VALUES (?, ?, ?)",
				params![&self.keyword, self.user_id, self.server_id],
			)?;

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete keyword": |conn| {
			conn.execute(
				"DELETE FROM keywords
				WHERE keyword = ? AND user_id = ? AND server_id = ?",
				params![&self.keyword, self.user_id, self.server_id],
			)?;

			Ok(())
		})
	}
}

#[derive(Debug, Clone)]
pub struct Follow {
	pub channel_id: i64,
	pub user_id: i64,
}

impl Follow {
	fn from_row(row: &Row) -> Result<Self, Error> {
		Ok(Follow {
			user_id: row.get(0)?,
			channel_id: row.get(1)?,
		})
	}

	pub fn create_table() {
		let conn = connection();
		conn.execute(
			"CREATE TABLE IF NOT EXISTS follows (
			user_id INTEGER NOT NULL,
			channel_id INTEGER NOT NULL,
			PRIMARY KEY (user_id, channel_id)
			)",
			params![],
		)
		.expect("Failed to create follows table");
	}

	pub async fn user_follows(user_id: UserId) -> Result<Vec<Follow>, Error> {
		await_db!("user follows": |conn| {
			let user_id: i64 = user_id.0.try_into().unwrap();

			let mut stmt = conn.prepare(
				"SELECT user_id, channel_id
				FROM keywords
				WHERE user_id = ?"
			)?;

			let follows = stmt.query_map(params![user_id], Follow::from_row)?;

			follows.collect()
		})
	}

	pub async fn exists(self) -> Result<bool, Error> {
		await_db!("follow exists": |conn| {
			conn.query_row(
				"SELECT COUNT(*) FROM follows
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id, self.channel_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
	}

	pub async fn insert(self) -> Result<(), Error> {
		await_db!("insert follow": |conn| {
			conn.execute(
				"INSERT INTO follows (user_id, channel_id)
				VALUES (?, ?)",
				params![self.user_id, self.channel_id],
			)?;

			Ok(())
		})
	}

	pub async fn delete(self) -> Result<(), Error> {
		await_db!("delete follow": |conn| {
			conn.execute(
				"DELETE FROM follows
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id, self.channel_id],
			)?;

			Ok(())
		})
	}
}
