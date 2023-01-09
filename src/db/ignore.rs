// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for ignored phrases.

use anyhow::{Context as _, Result};
use futures_util::TryStreamExt;
use sea_orm::{
	entity::prelude::{
		DeriveActiveModelBehavior, DeriveColumn, DeriveEntityModel,
		DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter,
		PrimaryKeyTrait,
	},
	ColumnTrait, Condition, IntoActiveModel, QueryFilter, QuerySelect,
};
use serenity::model::id::{GuildId, UserId};

use super::{connection, DbInt, IdDbExt};

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "guild_ignores")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) phrase: String,
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) guild_id: DbInt,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Represents an ignored phrase.
#[derive(Debug, Clone)]
pub(crate) struct Ignore {
	/// The phrase that should be ignored.
	pub(crate) phrase: String,
	/// The user that ignored this phrase.
	pub(crate) user_id: UserId,
	/// The guild in which the user ignored the phrase.
	pub(crate) guild_id: GuildId,
}

impl Ignore {
	/// Fetches the list of ignored phrases of the specified user in the
	/// specified guild from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_guild_ignores(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<Vec<Ignore>> {
		Entity::find()
			.filter(
				Condition::all()
					.add(Column::UserId.eq(user_id.into_db()))
					.add(Column::GuildId.eq(guild_id.into_db())),
			)
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Ignore::from)
			.try_collect()
			.await
	}

	/// Fetches the list of ignored phrases of the specified user across all
	/// guilds from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_ignores(user_id: UserId) -> Result<Vec<Ignore>> {
		Entity::find()
			.filter(Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Ignore::from)
			.try_collect()
			.await
	}

	/// Checks if this ignored phrase already exists in the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn exists(self) -> Result<bool> {
		let count = Entity::find()
			.select_only()
			.column_as(Column::UserId.count(), QueryAs::IgnoreCount)
			.filter(
				Condition::all()
					.add(Column::UserId.eq(self.user_id.into_db()))
					.add(Column::GuildId.eq(self.guild_id.into_db()))
					.add(Column::Phrase.eq(&*self.phrase)),
			)
			.into_values::<i64, QueryAs>()
			.one(connection())
			.await?;

		let count = count.context("No count for ignores returned")?;
		Ok(count == 1)
	}

	/// Adds this ignored phrase to the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn insert(self) -> Result<()> {
		Entity::insert(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Deletes this ignored phrase from the DB.
	#[tracing::instrument(
		skip(self),
		fields(
			self.user_id = %self.user_id,
			self.guild_id = %self.guild_id,
	))]
	pub(crate) async fn delete(self) -> Result<()> {
		Entity::delete(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Deletes all ignored phrases of the specified user in the specified guild
	/// from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete_in_guild(
		user_id: UserId,
		guild_id: GuildId,
	) -> Result<u64> {
		let result = Entity::delete_many()
			.filter(
				Condition::all()
					.add(Column::UserId.eq(user_id.into_db()))
					.add(Column::GuildId.eq(guild_id.into_db())),
			)
			.exec(connection())
			.await?;

		Ok(result.rows_affected)
	}
}

#[derive(Clone, Copy, Debug, EnumIter, DeriveColumn)]
enum QueryAs {
	IgnoreCount,
}

impl From<Model> for Ignore {
	fn from(model: Model) -> Self {
		Self {
			phrase: model.phrase,
			user_id: UserId::from_db(model.user_id),
			guild_id: GuildId::from_db(model.guild_id),
		}
	}
}

impl From<Ignore> for Model {
	fn from(ignore: Ignore) -> Self {
		Self {
			phrase: ignore.phrase,
			user_id: ignore.user_id.into_db(),
			guild_id: ignore.guild_id.into_db(),
		}
	}
}
