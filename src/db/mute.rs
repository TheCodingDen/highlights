use rusqlite::{params, Error, Row};
use serenity::model::id::UserId;

use std::convert::TryInto;

use crate::{await_db, db::connection};

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

	pub(super) fn create_table() {
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
