use crate::{connection, util::spawn_blocking_expect};

use automate::Snowflake;
use rusqlite::{params, Error};

use std::convert::TryInto;

#[derive(Debug, Clone)]
pub struct Keyword {
	pub keyword: String,
	pub user_id: i64,
	pub server_id: i64,
}

impl Keyword {
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
		guild_id: Snowflake,
		channel_id: Snowflake,
	) -> Result<Vec<Keyword>, Error> {
		spawn_blocking_expect(move || {
			let guild_id: i64 = guild_id.0.try_into().unwrap();
			let channel_id: i64 = channel_id.0.try_into().unwrap();

			let conn = connection();
			let mut stmt = conn.prepare(
				"SELECT keywords.keyword, keywords.user_id, keywords.server_id
				FROM keywords
				INNER JOIN follows
				ON keywords.user_id = follows.user_id
				WHERE keywords.server_id = ? AND follows.channel_id = ?",
			)?;

			let keywords =
				stmt.query_map(params![guild_id, channel_id], |row| {
					Ok(Keyword {
						keyword: row.get(0)?,
						user_id: row.get(1)?,
						server_id: row.get(2)?,
					})
				})?;

			keywords.collect()
		})
		.await
	}

	pub async fn exists(self) -> Result<bool, Error> {
		spawn_blocking_expect(move || {
			let conn = connection();

			conn.query_row(
				"SELECT COUNT(*) FROM keywords
				WHERE keyword = ? AND user_id = ? AND server_id = ?",
				params![&self.keyword, self.user_id, self.server_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
		.await
	}

	pub async fn user_keyword_count(user_id: i64) -> Result<u32, Error> {
		spawn_blocking_expect(move || {
			let conn = connection();

			conn.query_row(
				"SELECT COUNT(*) FROM keywords WHERE user_id = ?",
				params![user_id],
				|row| row.get::<_, u32>(0),
			)
		})
		.await
	}

	pub async fn insert(self) -> Result<(), Error> {
		spawn_blocking_expect(move || {
			let conn = connection();

			conn.execute(
				"INSERT INTO keywords (keyword, user_id, server_id)
				VALUES (?, ?, ?)",
				params![&self.keyword, self.user_id, self.server_id],
			)?;

			Ok(())
		})
		.await
	}
}

#[derive(Debug, Clone)]
pub struct Follow {
	pub channel_id: i64,
	pub user_id: i64,
}

impl Follow {
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

	pub async fn exists(self) -> Result<bool, Error> {
		spawn_blocking_expect(move || {
			let conn = connection();

			conn.query_row(
				"SELECT COUNT(*) FROM follows
				WHERE user_id = ? AND channel_id = ?",
				params![self.user_id, self.channel_id],
				|row| Ok(row.get::<_, u32>(0)? == 1),
			)
		})
		.await
	}

	pub async fn insert(self) -> Result<(), Error> {
		spawn_blocking_expect(move || {
			let conn = connection();

			conn.execute(
				"INSERT INTO follows (user_id, channel_id)
				VALUES (?, ?)",
				params![self.user_id, self.channel_id],
			)?;

			Ok(())
		})
		.await
	}
}
