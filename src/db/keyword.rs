// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Handling for keywords.

use rusqlite::{params, Error, Row};
use serenity::model::id::{ChannelId, GuildId, UserId};

use std::convert::TryInto;

use crate::{await_db, db::connection};

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
	/// Builds a guild-wide `Keyword` from a `Row`, in this order:
	/// - `keyword`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `<guild id>`: `INTEGER`
	fn from_guild_row(row: &Row) -> Result<Self, Error> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: row.get(1)?,
			kind: KeywordKind::Guild(row.get(2)?),
		})
	}

	/// Builds a channel-specific `Keyword` from a `Row`, in this order:
	/// - `keyword`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `<channel id>`: `INTEGER`
	fn from_channel_row(row: &Row) -> Result<Self, Error> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: row.get(1)?,
			kind: KeywordKind::Channel(row.get(2)?),
		})
	}

	/// Creates the DB tables for storing guild-wide and channel specific keywords.
	pub(super) fn create_tables() {
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

	/// Gets keywords that may be relelvant to a message.
	///
	/// Fetches all guild-wide keywords in the specified guild, as long as the creator of the
	/// keyword didn't mute the channel or block the author.
	///
	/// Fetches all channel-specific keywords in the specified channel, as long as the creator of
	/// the keyword didn't block the author.
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
						AND NOT EXISTS (
							SELECT blocks.user_id
								FROM blocks
								WHERE blocks.user_id = guild_keywords.user_id
									AND blocks.blocked_id = ?
						)
				",
			)?;

			let guild_keywords = stmt.query_map(
					params![guild_id, author_id, channel_id, author_id],
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

	/// Fetches all guild-wide keywords created by the specified user in the specified guild.
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

	/// Fetches all channel-specific keywords created by the specified user in the specified channel.
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

	/// Fetches all guild-wide and channel-specific keywords created by the specified user.
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

	/// Checks if this keyword has already been created by this user.
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

	/// Returns the number of keywords this user has created across all guilds and channels.
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

	/// Adds this keyword to the DB.
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

	/// Deletes this keyword from the DB.
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

	/// Deletes all guild-wide keywords created by the specified user in the specified guild.
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

	/// Deletes all channel-specific keywords created by the specified user in the specified channel.
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
