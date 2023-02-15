// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for keywords.

use anyhow::{Context, Result};
use futures_util::TryStreamExt;
use sea_orm::{
	sea_query::Expr, ColumnTrait, Condition, DeriveColumn, EntityTrait,
	EnumIter, IntoActiveModel, QueryFilter, QuerySelect, QueryTrait,
};
use serenity::model::id::{ChannelId, GuildId, UserId};
use tracing::info_span;

use super::{
	block, channel_keyword, connection, guild_keyword, mute, opt_out, IdDbExt,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum KeywordKind {
	Channel(ChannelId),
	Guild(GuildId),
}

impl Default for KeywordKind {
	fn default() -> Self {
		Self::Channel(ChannelId(0))
	}
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Keyword {
	pub(crate) keyword: String,
	pub(crate) user_id: UserId,
	pub(crate) kind: KeywordKind,
}

enum EitherModel {
	Channel(channel_keyword::Model),
	Guild(guild_keyword::Model),
}

impl Keyword {
	fn into_model(self) -> EitherModel {
		match self.kind {
			KeywordKind::Guild(guild_id) => {
				EitherModel::Guild(guild_keyword::Model {
					keyword: self.keyword,
					user_id: self.user_id.into_db(),
					guild_id: guild_id.into_db(),
				})
			}
			KeywordKind::Channel(channel_id) => {
				EitherModel::Channel(channel_keyword::Model {
					keyword: self.keyword,
					user_id: self.user_id.into_db(),
					channel_id: channel_id.into_db(),
				})
			}
		}
	}

	/// Gets keywords that may be relelvant to a message.
	///
	/// Fetches all guild-wide keywords in the specified guild, as long as the
	/// creator of the keyword didn't mute the channel or block the author.
	///
	/// Fetches all channel-specific keywords in the specified channel, as long
	/// as the creator of the keyword didn't block the author.
	#[tracing::instrument]
	pub(crate) async fn get_relevant_keywords(
		guild_id: GuildId,
		channel_id: ChannelId,
		author_id: UserId,
	) -> Result<Vec<Keyword>> {
		let span = info_span!(
			"relevant_guild_keywords",
			author_id = %author_id,
			guild_id = %guild_id
		);

		let entered = span.enter();

		let opted_out = opt_out::Entity::find()
			.select_only()
			.column(opt_out::Column::UserId)
			.into_query();

		let muted_channels =
			mute::Entity::find()
				.select_only()
				.column(mute::Column::ChannelId)
				.filter(Expr::col((mute::Entity, mute::Column::UserId)).equals(
					(guild_keyword::Entity, guild_keyword::Column::UserId),
				))
				.into_query();

		let users_with_block = block::Entity::find()
			.select_only()
			.column(block::Column::UserId)
			.filter(block::Column::BlockedId.eq(author_id.into_db()))
			.into_query();

		let keywords: Vec<Keyword> = guild_keyword::Entity::find()
			.filter(
				Condition::all()
					.add(guild_keyword::Column::UserId.ne(author_id.into_db()))
					.add(guild_keyword::Column::GuildId.eq(guild_id.into_db()))
					.add(
						guild_keyword::Column::UserId
							.not_in_subquery(opted_out.clone()),
					)
					.add(
						Expr::expr(Expr::value(author_id.into_db()))
							.not_in_subquery(opted_out.clone()),
					)
					.add(
						guild_keyword::Column::UserId
							.not_in_subquery(users_with_block.clone()),
					)
					.add(
						Expr::expr(Expr::value(channel_id.into_db()))
							.not_in_subquery(muted_channels.clone()),
					),
			)
			.stream(connection())
			.await?
			.map_err(anyhow::Error::from)
			.map_ok(Keyword::from)
			.try_collect()
			.await?;

		drop(entered);
		drop(span);

		let span = info_span!(
			"relevant_channel_keywords",
			author_id = %author_id,
			channel_id = %channel_id
		);

		let _entered = span.enter();

		channel_keyword::Entity::find()
			.filter(
				Condition::all()
					.add(
						channel_keyword::Column::UserId.ne(author_id.into_db()),
					)
					.add(
						channel_keyword::Column::ChannelId
							.eq(channel_id.into_db()),
					)
					.add(
						channel_keyword::Column::UserId
							.not_in_subquery(opted_out.clone()),
					)
					.add(
						Expr::expr(Expr::value(author_id.into_db()))
							.not_in_subquery(opted_out),
					)
					.add(
						channel_keyword::Column::UserId
							.not_in_subquery(users_with_block),
					),
			)
			.stream(connection())
			.await?
			.map_err(anyhow::Error::from)
			.map_ok(Keyword::from)
			.try_fold(keywords, |mut keywords, keyword| async move {
				keywords.push(keyword);
				Ok(keywords)
			})
			.await
	}

	/// Fetches all guild-wide keywords created by the specified user in the
	/// specified guild.
	#[tracing::instrument]
	pub(crate) async fn user_guild_keywords(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Keyword>> {
		guild_keyword::Entity::find()
			.filter(
				Condition::all()
					.add(guild_keyword::Column::UserId.eq(user_id.into_db()))
					.add(guild_keyword::Column::GuildId.eq(guild_id.into_db())),
			)
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Keyword::from)
			.try_collect()
			.await
	}

	/// Fetches all channel-specific keywords created by the specified user.
	#[tracing::instrument]
	pub(crate) async fn user_channel_keywords(
		user_id: UserId,
	) -> Result<Vec<Keyword>> {
		channel_keyword::Entity::find()
			.filter(channel_keyword::Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Keyword::from)
			.try_collect()
			.await
	}

	/// Fetches all guild-wide and channel-specific keywords created by the
	/// specified user.
	#[tracing::instrument]
	pub(crate) async fn user_keywords(user_id: UserId) -> Result<Vec<Keyword>> {
		let keywords: Vec<Keyword> = guild_keyword::Entity::find()
			.filter(guild_keyword::Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(anyhow::Error::from)
			.map_ok(Keyword::from)
			.try_collect()
			.await?;

		channel_keyword::Entity::find()
			.filter(channel_keyword::Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(anyhow::Error::from)
			.map_ok(Keyword::from)
			.try_fold(keywords, |mut keywords, keyword| async move {
				keywords.push(keyword);
				Ok(keywords)
			})
			.await
	}

	/// Checks if this keyword has already been created by this user.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.kind = ?self.kind,
	))]
	pub(crate) async fn exists(self) -> Result<bool> {
		match self.kind {
			KeywordKind::Guild(guild_id) => {
				let count = guild_keyword::Entity::find()
					.select_only()
					.column_as(
						guild_keyword::Column::UserId.count(),
						QueryAs::KeywordCount,
					)
					.filter(
						Condition::all()
							.add(
								guild_keyword::Column::UserId
									.eq(self.user_id.into_db()),
							)
							.add(
								guild_keyword::Column::GuildId
									.eq(guild_id.into_db()),
							)
							.add(
								guild_keyword::Column::Keyword
									.eq(&*self.keyword),
							),
					)
					.into_values::<i64, QueryAs>()
					.one(connection())
					.await?;

				let count =
					count.context("No count for guild keywords returned")?;
				Ok(count == 1)
			}
			KeywordKind::Channel(channel_id) => {
				let count = channel_keyword::Entity::find()
					.select_only()
					.column_as(
						channel_keyword::Column::UserId.count(),
						QueryAs::KeywordCount,
					)
					.filter(
						Condition::all()
							.add(
								channel_keyword::Column::UserId
									.eq(self.user_id.into_db()),
							)
							.add(
								channel_keyword::Column::ChannelId
									.eq(channel_id.into_db()),
							)
							.add(
								channel_keyword::Column::Keyword
									.eq(&*self.keyword),
							),
					)
					.into_values::<i64, QueryAs>()
					.one(connection())
					.await?;

				let count =
					count.context("No count for channel keywords returned")?;
				Ok(count == 1)
			}
		}
	}

	/// Returns the number of keywords this user has created across all guilds
	/// and channels.
	#[tracing::instrument]
	pub(crate) async fn user_keyword_count(user_id: UserId) -> Result<u64> {
		let guild_keywords = guild_keyword::Entity::find()
			.select_only()
			.column_as(
				guild_keyword::Column::UserId.count(),
				QueryAs::KeywordCount,
			)
			.filter(guild_keyword::Column::UserId.eq(user_id.into_db()))
			.into_values::<i64, QueryAs>()
			.one(connection())
			.await?
			.context("No count for guild keywords returned")?;

		let channel_keywords = channel_keyword::Entity::find()
			.select_only()
			.column_as(
				channel_keyword::Column::UserId.count(),
				QueryAs::KeywordCount,
			)
			.filter(channel_keyword::Column::UserId.eq(user_id.into_db()))
			.into_values::<i64, QueryAs>()
			.one(connection())
			.await?
			.context("No count for channel keywords returned")?;

		Ok(guild_keywords as u64 + channel_keywords as u64)
	}

	/// Adds this keyword to the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.kind = ?self.kind,
	))]
	pub(crate) async fn insert(self) -> Result<()> {
		match self.into_model() {
			EitherModel::Guild(model) => {
				guild_keyword::Entity::insert(model.into_active_model())
					.exec(connection())
					.await?;
			}
			EitherModel::Channel(model) => {
				channel_keyword::Entity::insert(model.into_active_model())
					.exec(connection())
					.await?;
			}
		}

		Ok(())
	}

	/// Deletes this keyword from the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.kind = ?self.kind,
	))]
	pub(crate) async fn delete(self) -> Result<()> {
		match self.into_model() {
			EitherModel::Guild(model) => {
				guild_keyword::Entity::delete(model.into_active_model())
					.exec(connection())
					.await?;
			}
			EitherModel::Channel(model) => {
				channel_keyword::Entity::delete(model.into_active_model())
					.exec(connection())
					.await?;
			}
		}

		Ok(())
	}

	/// Deletes all guild-wide keywords created by the specified user in the
	/// specified guild.
	#[tracing::instrument]
	pub(crate) async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<u64> {
		let result = guild_keyword::Entity::delete_many()
			.filter(
				Condition::all()
					.add(guild_keyword::Column::UserId.eq(user_id.into_db()))
					.add(guild_keyword::Column::GuildId.eq(guild_id.into_db())),
			)
			.exec(connection())
			.await?;

		Ok(result.rows_affected)
	}

	/// Deletes all channel-specific keywords created by the specified user in
	/// the specified channel.
	#[tracing::instrument]
	pub(crate) async fn delete_in_channel(
		user_id: UserId,
		channel_id: ChannelId,
	) -> Result<u64> {
		let result = channel_keyword::Entity::delete_many()
			.filter(
				Condition::all()
					.add(channel_keyword::Column::UserId.eq(user_id.into_db()))
					.add(
						channel_keyword::Column::ChannelId
							.eq(channel_id.into_db()),
					),
			)
			.exec(connection())
			.await?;

		Ok(result.rows_affected)
	}
}

#[derive(Clone, Copy, Debug, EnumIter, DeriveColumn)]
enum QueryAs {
	KeywordCount,
	MutedChannel,
}

impl From<guild_keyword::Model> for Keyword {
	fn from(model: guild_keyword::Model) -> Self {
		Self {
			keyword: model.keyword,
			user_id: UserId::from_db(model.user_id),
			kind: KeywordKind::Guild(GuildId::from_db(model.guild_id)),
		}
	}
}

impl From<channel_keyword::Model> for Keyword {
	fn from(model: channel_keyword::Model) -> Self {
		Self {
			keyword: model.keyword,
			user_id: UserId::from_db(model.user_id),
			kind: KeywordKind::Channel(ChannelId::from_db(model.channel_id)),
		}
	}
}
