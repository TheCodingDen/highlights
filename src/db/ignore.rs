// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use rusqlite::{params, Error, Row};
use serenity::model::id::{GuildId, UserId};

use std::convert::TryInto;

use crate::{await_db, db::connection};

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

	pub(super) fn create_table() {
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
