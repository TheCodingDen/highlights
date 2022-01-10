// Copyright 2021 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for keywords.

use anyhow::Result;
use rusqlite::{params, Row};
use serenity::model::id::{ChannelId, GuildId, UserId};

use crate::{await_db, db::connection};

use super::IdI64Ext;

#[derive(Debug, Clone, Copy)]
pub enum KeywordKind {
	Channel(ChannelId),
	Guild(GuildId),
}

impl Default for KeywordKind {
	fn default() -> Self {
		Self::Channel(ChannelId(0))
	}
}

#[derive(Debug, Clone, Default)]
pub struct Keyword {
	pub keyword: String,
	pub user_id: UserId,
	pub kind: KeywordKind,
}

impl Keyword {
	/// Builds a guild-wide `Keyword` from a `Row`, in this order:
	/// - `keyword`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `<guild id>`: `INTEGER`
	fn from_guild_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: UserId::from_i64(row.get(1)?),
			kind: KeywordKind::Guild(GuildId::from_i64(row.get(2)?)),
		})
	}

	/// Builds a channel-specific `Keyword` from a `Row`, in this order:
	/// - `keyword`: `TEXT`
	/// - `user_id`: `INTEGER`
	/// - `<channel id>`: `INTEGER`
	fn from_channel_row(row: &Row) -> rusqlite::Result<Self> {
		Ok(Keyword {
			keyword: row.get(0)?,
			user_id: UserId::from_i64(row.get(1)?),
			kind: KeywordKind::Channel(ChannelId::from_i64(row.get(2)?)),
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
	) -> Result<Vec<Keyword>> {
		await_db!("get keywords": |conn| {
			let guild_id = guild_id.into_i64();
			let channel_id = channel_id.into_i64();
			let author_id = author_id.into_i64();

			let mut stmt = conn.prepare(
				"SELECT guild_keywords.keyword, guild_keywords.user_id, guild_keywords.guild_id
					FROM guild_keywords
					WHERE guild_keywords.guild_id = ?
						AND guild_keywords.user_id != ?
						AND NOT EXISTS (
							SELECT opt_outs.user_id
								FROM opt_outs
								WHERE opt_outs.user_id = ?
						)
						AND NOT EXISTS (
							SELECT opt_outs.user_id
								FROM opt_outs
								WHERE opt_outs.user_id = guild_keywords.user_id
						)
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
					params![
						guild_id,
						author_id,
						author_id,
						channel_id,
						author_id
					],
					Keyword::from_guild_row
				)?;

			let mut keywords = guild_keywords.collect::<Result<Vec<_>, _>>()?;

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
					FROM channel_keywords
					WHERE user_id != ?
						AND channel_id = ?
						AND NOT EXISTS (
							SELECT opt_outs.user_id
								FROM opt_outs
								where opt_outs.user_id = ?
						)
						AND NOT EXISTS (
							SELECT opt_outs.user_id
								FROM opt_outs
								WHERE opt_outs.user_id = channel_keywords.user_id
						)
						AND NOT EXISTS (
							SELECT blocks.user_id
								FROM blocks
								WHERE blocks.user_id = channel_keywords.user_id
									AND blocks.blocked_id = ?
						)"
			)?;

			let channel_keywords = stmt.query_map(
				params![author_id, channel_id, author_id, author_id],
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
	) -> Result<Vec<Keyword>> {
		await_db!("user guild keywords": |conn| {

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, guild_id
				FROM guild_keywords
				WHERE user_id = ? AND guild_id = ?"
			)?;

			let keywords = stmt.query_map(
				params![user_id.into_i64(), guild_id.into_i64()],
				Keyword::from_guild_row
			)?;

			keywords.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Fetches all channel-specific keywords created by the specified user in the specified channel.
	pub async fn user_channel_keywords(
		user_id: UserId,
	) -> Result<Vec<Keyword>> {
		await_db!("user channel keywords": |conn| {
			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
				FROM channel_keywords
				WHERE user_id = ?"
			)?;

			let keywords = stmt.query_map(
				params![user_id.into_i64()],
				Keyword::from_channel_row
			)?;

			keywords.map(|res| res.map_err(Into::into)).collect()
		})
	}

	/// Fetches all guild-wide and channel-specific keywords created by the specified user.
	pub async fn user_keywords(user_id: UserId) -> Result<Vec<Keyword>> {
		await_db!("user keywords": |conn| {
			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, guild_id
				FROM guild_keywords
				WHERE user_id = ?"
			)?;

			let guild_keywords = stmt.query_map(
				params![user_id.into_i64()],
				Keyword::from_guild_row
			)?;

			let mut keywords = guild_keywords.collect::<Result<Vec<_>, _>>()?;

			let mut stmt = conn.prepare(
				"SELECT keyword, user_id, channel_id
				FROM channel_keywords
				WHERE user_id = ?"
			)?;

			let channel_keywords = stmt.query_map(
				params![user_id.into_i64()],
				Keyword::from_channel_row
			)?;

			keywords.extend(channel_keywords.collect::<Result<Vec<_>, _>>()?);

			Ok(keywords)
		})
	}

	/// Checks if this keyword has already been created by this user.
	pub async fn exists(self) -> Result<bool> {
		await_db!("keyword exists": |conn| {
			match self.kind {
				KeywordKind::Guild(guild_id) => {
					conn.query_row(
						"SELECT COUNT(*) FROM guild_keywords
						WHERE keyword = ? AND user_id = ? AND guild_id = ?",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							guild_id.into_i64()
						],
						|row| Ok(row.get::<_, u32>(0)? == 1),
					).map_err(Into::into)
				}
				KeywordKind::Channel(channel_id) => {
					conn.query_row(
						"SELECT COUNT(*) FROM channel_keywords
						WHERE keyword = ? AND user_id = ? AND channel_id = ?",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							channel_id.into_i64()
						],
						|row| Ok(row.get::<_, u32>(0)? == 1),
					).map_err(Into::into)
				}
			}
		})
	}

	/// Returns the number of keywords this user has created across all guilds and channels.
	pub async fn user_keyword_count(user_id: UserId) -> Result<u32> {
		await_db!("count user keywords": |conn| {
			let guild_keywords = conn.query_row(
				"SELECT COUNT(*)
					FROM guild_keywords
					WHERE user_id = ?",
				params![user_id.into_i64()],
				|row| row.get::<_, u32>(0),
			)?;

			let channel_keywords = conn.query_row(
				"SELECT COUNT(*)
					FROM channel_keywords
					WHERE user_id = ?",
				params![user_id.into_i64()],
				|row| row.get::<_, u32>(0),
			)?;

			Ok(guild_keywords + channel_keywords)
		})
	}

	/// Adds this keyword to the DB.
	pub async fn insert(self) -> Result<()> {
		await_db!("insert keyword": |conn| {
			match self.kind {
				KeywordKind::Guild(guild_id) => {
					conn.execute(
						"INSERT INTO guild_keywords (keyword, user_id, guild_id)
							VALUES (?, ?, ?)",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							guild_id.into_i64()
						],
					)?;
				}
				KeywordKind::Channel(channel_id) => {
					conn.execute(
						"INSERT INTO channel_keywords (keyword, user_id, channel_id)
							VALUES (?, ?, ?)",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							channel_id.into_i64()
						],
					)?;
				}
			}

			Ok(())
		})
	}

	/// Deletes this keyword from the DB.
	pub async fn delete(self) -> Result<()> {
		await_db!("delete keyword": |conn| {
			match self.kind {
				KeywordKind::Guild(guild_id) => {
					conn.execute(
						"DELETE FROM guild_keywords
							WHERE keyword = ? AND user_id = ? AND guild_id = ?",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							guild_id.into_i64()
						],
					)?;
				}
				KeywordKind::Channel(channel_id) => {
					conn.execute(
						"DELETE FROM channel_keywords
							WHERE keyword = ? AND user_id = ? AND channel_id = ?",
						params![
							&self.keyword,
							self.user_id.into_i64(),
							channel_id.into_i64()
						],
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
	) -> Result<usize> {
		await_db!("delete keywords in guild": |conn| {
			conn.execute(
				"DELETE FROM guild_keywords
					WHERE user_id = ? AND guild_id = ?",
				params![user_id.into_i64(), guild_id.into_i64()]
			).map_err(Into::into)
		})
	}

	/// Deletes all channel-specific keywords created by the specified user in the specified channel.
	pub async fn delete_in_channel(
		user_id: UserId,
		channel_id: ChannelId,
	) -> Result<usize> {
		await_db!("delete keywords in channel": |conn| {
			conn.execute(
				"DELETE FROM channel_keywords
					WHERE user_id = ? AND channel_id = ?",
				params![user_id.into_i64(), channel_id.into_i64()]
			).map_err(Into::into)
		})
	}
}
